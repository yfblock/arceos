#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

extern crate axplat_riscv64_sg2002;

mod imgcat;
mod usb_camera;

#[cfg(feature = "axstd")]
use axstd::println;

#[cfg_attr(feature = "axstd", unsafe(no_mangle))]
fn main() {
    println!("Hello, world!");

    let (cam, sel) = match usb_camera::init() {
        Ok(v) => v,
        Err(msg) => {
            println!("USB/UVC init failed: {msg}");
            loop {
                core::hint::spin_loop();
            }
        }
    };

    match usb_camera::capture_frame(&cam, &sel) {
        Ok(jpeg) => {
            println!("imgcat: 输出 1 帧 JPEG ({} bytes)", jpeg.len());
            imgcat::print_image(jpeg);
            println!("imgcat: 完成");
        }
        Err(msg) => println!("UVC capture failed: {msg}"),
    }

    loop {
        core::hint::spin_loop();
    }
}
