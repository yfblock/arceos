#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

extern crate axplat_riscv64_sg2002;

mod cpu_jpeg;
mod jpu;
#[cfg(feature = "jpu-c")]
mod jpu_c;

/// Embedded JPEG image for testing
const LOGO_JPEG: &[u8] = include_bytes!("../../../logo.jpeg");

#[cfg(feature = "axstd")]
use axstd::println;

#[cfg_attr(feature = "axstd", unsafe(no_mangle))]
fn main() {
    println!("Hello, world!");

    const BOOT_DELAY_S: u64 = 5;
    const BOOT_TIMER_HZ: u64 = 25_000_000;
    let boot_deadline =
        axhal::time::current_ticks() + BOOT_TIMER_HZ.saturating_mul(BOOT_DELAY_S);
    let mut sec_left = BOOT_DELAY_S;
    while axhal::time::current_ticks() < boot_deadline {
        let remain = boot_deadline.saturating_sub(axhal::time::current_ticks());
        let now_sec = remain / BOOT_TIMER_HZ + 1;
        if now_sec <= sec_left {
            println!("Boot delay: {now_sec}s...");
            sec_left = now_sec.saturating_sub(1);
        }
        core::hint::spin_loop();
    }

    println!("=== JPEG Decode Benchmark: JPU vs CPU ===");
    println!("JPEG size: {} bytes", LOGO_JPEG.len());

    #[cfg(feature = "jpu-c")]
    {
        println!("[JPU] C driver timing not implemented, use Rust decoder");
    }
    #[cfg(not(feature = "jpu-c"))]
    {
        // CPU 软件解码（先跑，避免 JPU init 影响）
        println!("--- CPU decode (zune-jpeg, YCbCr) ---");
        let cpu_result = match cpu_jpeg::decode(LOGO_JPEG) {
            Ok(result) => {
                println!(
                    "OK: {}x{}, pixels {} bytes",
                    result.width,
                    result.height,
                    result.pixels_len
                );
                result.timings.print_report();
                Some(result)
            }
            Err(e) => {
                println!("CPU decode failed: {}", e);
                None
            }
        };

        println!("--- JPU decode run 1 (after init) ---");
        match jpu::JpuDecoder::new() {
            Ok(mut decoder) => {
                match decoder.decode(LOGO_JPEG) {
                    Ok(result) => {
                        println!(
                            "OK: {}x{}, yuv {} bytes",
                            result.width,
                            result.height,
                            result.yuv_data.len()
                        );
                        result.timings.print_report();

                        if let Some(ref cpu) = cpu_result {
                            cpu_jpeg::print_comparison(
                                result.timings.decode_total_us,
                                result.timings.hw_decode_us,
                                &cpu.timings,
                            );
                        }

                        println!("--- JPU decode run 2 (warm) ---");
                        match decoder.decode(LOGO_JPEG) {
                            Ok(result2) => {
                                println!(
                                    "OK: {}x{}, yuv {} bytes",
                                    result2.width,
                                    result2.height,
                                    result2.yuv_data.len()
                                );
                                let mut warm = result2.timings;
                                warm.init_us = 0;
                                warm.print_report();

                                if let Some(ref cpu) = cpu_result {
                                    cpu_jpeg::print_comparison(
                                        warm.decode_total_us,
                                        warm.hw_decode_us,
                                        &cpu.timings,
                                    );
                                }
                            }
                            Err(e) => println!("JPU run 2 failed: {}", e),
                        }
                    }
                    Err(e) => println!("JPU run 1 failed: {}", e),
                }
            }
            Err(e) => println!("[JPU] Failed to create decoder: {}", e),
        }
    }

    loop {
        core::hint::spin_loop();
    }
}
