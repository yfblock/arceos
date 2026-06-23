//! 简易 imgcat：使用 iTerm2 Inline Images Protocol 把图片字节流写到控制台。
//!
//! 协议格式：`ESC ] 1337 ; File = inline=1 : <base64> BEL`
//!
//! 终端支持情况：
//! - 支持：iTerm2、WezTerm、mintty、tio、kitty (兼容模式) 等。
//! - 不支持：picocom / 裸 cat —— 会把 base64 当作乱码输出（无害但不显示图）。
//!
//! 串口直接送出，不经过 tmux/screen，因此使用裸 OSC 1337 序列即可。
//! 流式编码 + 局部缓冲，避免分配大块内存。

#![allow(dead_code)]

use axplat::console::write_bytes;

use crate::serial_export;

/// 把任意字节流（JPEG/PNG/BMP 等）以 iTerm2 inline image 协议写入控制台。
pub fn print_image(image: &[u8]) {
    print_image_named("image.bin", image);
}

/// 指定文件名 hint，便于终端识别 BMP/JPEG 等格式。
pub fn print_image_named(name: &str, image: &[u8]) {
    write_bytes(b"\x1b]1337;File=inline=1;name=");
    write_bytes(name.as_bytes());
    write_bytes(b":");
    serial_export::write_base64(image);
    write_bytes(b"\x07\n");
}

/// 仅写出 base64 编码后的内容（不带协议头/尾），方便复用。
#[allow(dead_code)]
fn base64_stream(input: &[u8]) {
    serial_export::write_base64(input);
}
