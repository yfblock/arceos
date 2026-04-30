//! SG2002 USB UVC 摄像头一体化模块。
//!
//! 将 SG2002 平台 USB 主机初始化（时钟、PHY、VBUS）、DWC2 控制器探测、
//! UVC 摄像头枚举/协商/抓帧集中在一个文件中。
//!
//! **移植指南**：拷贝本文件到新 ArceOS 工程，声明 `mod usb_camera;`，
//! 然后在 `main()` 中调用 `usb_camera::run()` 即可。
//! 也可使用 `init()` + `capture_frame()` 进行细粒度控制。
//!
//! sg200x-bsp 中的 USB/UVC 协议层代码**不**在本文件中复制，保持原样依赖。

#![allow(dead_code)]

use axplat::mem::{PhysAddr, VirtAddr, phys_to_virt, virt_to_phys};
use axstd::println;
use sg200x_bsp::gpio::{Direction, GPIO, GPIO1_BASE, GPIOPort};
use sg200x_bsp::pinmux::{FMUX_USB_VBUS_DET, Pinmux};
use sg200x_bsp::usb::{
    class::uvc,
    error::UsbError,
    host::{self, UvcEnumerated, dwc2, dwc2::ep0 as dwc2_ep0},
    log, platform,
};
use tock_registers::interfaces::Writeable;

// =========================================================================
//  SG2002 平台常量
// =========================================================================

const USB_DWC2_PADDR: usize = 0x0434_0000;
const CLKGEN_PADDR: usize = 0x0300_2000;
const TOP_PADDR: usize = 0x0300_0000;
const IOBLK_G1_PADDR: usize = 0x0300_1800;
const IOBLK_G1_USB_VBUS_DET_OFF: usize = 0x020;

const VBUS_GPIO_PORT: GPIOPort = GPIOPort::GPIO1;
const VBUS_GPIO_PIN: u8 = 6;
const VBUS_GPIO_ACTIVE_HIGH: bool = true;

// =========================================================================
//  平台初始化（时钟、PHY、VBUS）
// =========================================================================

fn ep0_dma_virt_to_phys(p: *const u8) -> u32 {
    virt_to_phys(VirtAddr::from(p as usize)).as_usize() as u32
}

fn usb_log_line(s: &str) {
    println!("{s}");
}

#[inline]
fn spin_udelay_approx(us: u32) {
    for _ in 0..us.saturating_mul(64) {
        core::hint::spin_loop();
    }
}

unsafe fn enable_usb_clocks_cv181x() {
    let b = phys_to_virt(PhysAddr::from_usize(CLKGEN_PADDR)).as_usize();
    let en1 = (b + 0x004) as *mut u32;
    let en2 = (b + 0x008) as *mut u32;
    let byp0 = (b + 0x030) as *mut u32;
    let v1_pre = core::ptr::read_volatile(en1);
    let v2_pre = core::ptr::read_volatile(en2);
    let byp_pre = core::ptr::read_volatile(byp0);
    core::ptr::write_volatile(en1, v1_pre | (0xFu32 << 28));
    core::ptr::write_volatile(en2, v2_pre | 1u32);
    core::ptr::write_volatile(byp0, byp_pre & !((1u32 << 17) | (1u32 << 18)));
}

/// PHY ID pad toggle workaround（见 phy-cv1800-usb.c）：先写 device 再写 host。
unsafe fn cvitek_usb_top_host_bringup() {
    let top = phys_to_virt(PhysAddr::from_usize(TOP_PADDR)).as_usize();
    let rst = (top + 0x3000) as *mut u32;
    let v = core::ptr::read_volatile(rst);
    core::ptr::write_volatile(rst, v & !(1 << 11));
    spin_udelay_approx(50);
    core::ptr::write_volatile(rst, v | (1 << 11));
    spin_udelay_approx(50);

    let usb_pin = (top + 0x48) as *mut u32;
    let x = core::ptr::read_volatile(usb_pin);
    let dev_mode = (x & !0xC0u32) | 0xC0u32 | 0x01u32;
    core::ptr::write_volatile(usb_pin, dev_mode);
    spin_udelay_approx(1_000);
    let host_mode = (x & !0xC0u32) | 0x40u32 | 0x01u32;
    core::ptr::write_volatile(usb_pin, host_mode);
    spin_udelay_approx(1_000);

    let eco = (top + 0xB4) as *mut u32;
    core::ptr::write_volatile(eco, core::ptr::read_volatile(eco) | 0x80);
}

fn pinmux_usb_vbus_det_gpio_output_prep() {
    let pinmux = Pinmux::new();
    pinmux.fmux().usb_vbus_det.write(FMUX_USB_VBUS_DET::FSEL::XGPIOB_6);
    let iob = phys_to_virt(PhysAddr::from_usize(IOBLK_G1_PADDR)).as_usize();
    let r = (iob + IOBLK_G1_USB_VBUS_DET_OFF) as *mut u32;
    unsafe {
        let v = core::ptr::read_volatile(r);
        core::ptr::write_volatile(r, v | (7 << 5));
    }
}

fn enable_usb_vbus_gpio() {
    let gpio_va = phys_to_virt(PhysAddr::from_usize(GPIO1_BASE)).as_usize();
    let gpio = unsafe { GPIO::from_base_address(gpio_va, VBUS_GPIO_PORT) };
    gpio.set_direction(VBUS_GPIO_PIN, Direction::Output);
    gpio.set(VBUS_GPIO_PIN, VBUS_GPIO_ACTIVE_HIGH);
}

// =========================================================================
//  USB 主机初始化 + UVC 摄像头枚举
// =========================================================================

/// 初始化 USB 主机控制器并枚举 UVC 摄像头，完成 PROBE/COMMIT/SET_INTERFACE。
///
/// 成功返回 `(cam, sel)`，可反复调用 [`capture_frame`] 抓帧。
/// 内部已做 1 帧 warmup 丢弃首个不完整帧。
pub fn init() -> Result<(UvcEnumerated, uvc::UvcStreamSelection), &'static str> {
    // --- 平台初始化 ---
    unsafe {
        enable_usb_clocks_cv181x();
        cvitek_usb_top_host_bringup();
    }
    pinmux_usb_vbus_det_gpio_output_prep();
    enable_usb_vbus_gpio();
    spin_udelay_approx(2_000_000);

    let vbase = phys_to_virt(PhysAddr::from_usize(USB_DWC2_PADDR)).as_usize();
    platform::set_dwc2_base_virt(vbase);
    platform::set_usb_dma_to_phys_fn(Some(ep0_dma_virt_to_phys));
    log::set_usb_log_fn(usb_log_line);
    dwc2::ep0::debug_log_ep0_dma_info();

    unsafe {
        dwc2::dwc2_probe().map_err(|e| {
            println!("USB DWC2 probe failed: {e:?}");
            "DWC2 probe 失败"
        })?;
    }

    // --- 拓扑扫描（含重试）---
    let mut last_err = None;
    let extras = (0..4).find_map(|attempt| {
        if attempt > 0 {
            spin_udelay_approx(1_500_000 * attempt as u32);
        }
        match host::enumerate_topology_only() {
            Ok(ex) => Some(ex),
            Err(e) => {
                println!("USB: 枚举失败 #{}: {:?}", attempt + 1, e);
                last_err = Some(e);
                None
            }
        }
    }).ok_or_else(|| {
        println!("USB: 枚举重试全部失败: {:?}", last_err);
        "USB 拓扑扫描失败"
    })?;

    let cam = extras.uvc.ok_or("未检测到 UVC 摄像头")?;
    println!(
        "UVC: addr={} VID={:04x} PID={:04x} ep0_mps={}",
        cam.addr, cam.vid, cam.pid, cam.ep0_mps
    );

    // --- UVC 配置描述符解析 + 流协商 ---
    let dev = u32::from(cam.addr);
    let ep0 = cam.ep0_mps;
    let cfg_buf = uvc::read_configuration_descriptor(dev, ep0, 1).map_err(|e| {
        println!("UVC: read_configuration_descriptor err={:?}", e);
        "读取配置描述符失败"
    })?;
    let cfg_total = u16::from_le_bytes([cfg_buf[2], cfg_buf[3]]) as usize;
    let mut sel = uvc::parse_uvc_video_stream(&cfg_buf[..cfg_total.min(cfg_buf.len())], cfg_total)
        .map_err(|e| match e {
            UsbError::NotImplemented => "未找到 VS Bulk/Isoch 视频端点",
            _ => {
                println!("UVC: parse_uvc_video_stream err={:?}", e);
                "解析 UVC 流参数失败"
            }
        })?;

    if let Some(entities) = uvc::parse_uvc_control_entities(
        &cfg_buf[..cfg_total.min(cfg_buf.len())],
        cfg_total,
    ) {
        let tune = uvc::UvcImageTuning {
            brightness: Some(96),
            ..uvc::UvcImageTuning::default()
        };
        let _ = uvc::uvc_init_camera_controls(dev, ep0, &entities, &tune);
    }

    uvc::uvc_start_video_stream(dev, ep0, &mut sel).map_err(|e| {
        println!("UVC: uvc_start_video_stream err={:?}", e);
        "UVC PROBE/COMMIT 或 SET_INTERFACE 失败"
    })?;
    println!(
        "UVC: 视频流就绪 {}x{} payload={} frame_size={}",
        sel.frame_w, sel.frame_h, sel.negotiated_payload_size, sel.negotiated_frame_size
    );

    let _ = uvc::uvc_capture_one_frame(dev, ep0, &sel);

    Ok((cam, sel))
}

// =========================================================================
//  帧抓取
// =========================================================================

/// 抓取 1 帧 MJPEG，返回 DMA 缓冲区中的 JPEG 字节切片。
/// 切片在下一次调用 `capture_frame` 之前有效。
/// 内部对 SOI/EOI 做校验，无效帧会自动重试（最多 8 次）。
pub fn capture_frame(
    cam: &UvcEnumerated,
    sel: &uvc::UvcStreamSelection,
) -> Result<&'static [u8], &'static str> {
    let dev = u32::from(cam.addr);
    let ep0 = cam.ep0_mps;

    const MAX_TRIES: u32 = 8;
    const MIN_VALID_BYTES: usize = 4096;
    let mut last_n: usize = 0;
    let mut last_msg: Option<&'static str> = None;
    for attempt in 0..MAX_TRIES {
        let n = uvc::uvc_capture_one_frame(dev, ep0, sel).map_err(|e| {
            println!("UVC: capture err={:?}", e);
            "抓帧失败"
        })?;
        last_n = n;
        let s = dwc2_ep0::dma_rx_slice(uvc::UVC_ASSEMBLED_JPEG_DMA_OFF, n)
            .ok_or("DMA 切片越界")?;
        let starts_jpeg = n >= 2 && s[0] == 0xff && s[1] == 0xd8;
        let ends_jpeg = n >= 2 && s[n - 2] == 0xff && s[n - 1] == 0xd9;
        if starts_jpeg && ends_jpeg && n >= MIN_VALID_BYTES {
            return Ok(s);
        }
        last_msg = Some(if !starts_jpeg {
            "首字节非 ff d8"
        } else if !ends_jpeg {
            "末字节非 ff d9（被截断）"
        } else {
            "尺寸过小"
        });
        println!(
            "UVC: 帧无效 (try #{}/{}, size={}, {}); 重置 FID",
            attempt + 1, MAX_TRIES, n, last_msg.unwrap_or("?")
        );
        uvc::reset_frame_continuity();
    }
    println!(
        "UVC: 重试 {} 次仍未拿到完整 JPEG，size={} {}",
        MAX_TRIES, last_n, last_msg.unwrap_or("?")
    );
    dwc2_ep0::dma_rx_slice(uvc::UVC_ASSEMBLED_JPEG_DMA_OFF, last_n).ok_or("DMA 切片越界")
}

// =========================================================================
//  顶层入口
// =========================================================================

/// 一键启动：初始化 USB → 枚举 UVC 摄像头 → 持续抓帧（不返回）。
///
/// 移植到新 ArceOS 时，只需 `mod usb_camera;` 然后 `usb_camera::run();`。
pub fn run() -> ! {
    println!("=== SG2002 USB UVC 摄像头 ===");
    let (cam, sel) = match init() {
        Ok(v) => v,
        Err(msg) => {
            println!("USB/UVC 初始化失败: {msg}");
            loop {
                core::hint::spin_loop();
            }
        }
    };
    println!("UVC: 进入持续抓帧循环 (USB addr={} {}x{})", cam.addr, sel.frame_w, sel.frame_h);
    loop {
        match capture_frame(&cam, &sel) {
            Ok(jpeg) => println!("UVC: 抓帧成功 size={} bytes", jpeg.len()),
            Err(msg) => println!("UVC: 抓帧失败 - {msg}"),
        }
    }
}
