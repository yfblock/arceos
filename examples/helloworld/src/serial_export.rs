//! 通过串口导出文件：ASCII 标记 + base64，供主机脚本解析保存。

use axplat::console::write_bytes;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const FLUSH_BYTES: usize = 1024;

/// 导出二进制文件（标记内为 base64，主机解码为原始字节）。
pub fn export_binary(name: &str, data: &[u8]) {
    axstd::println!("=== ARCEOS_FILE_BEGIN {name} {} bin ===", data.len());
    write_base64(data);
    axstd::println!("\n=== ARCEOS_FILE_END {name} ===");
}

/// 导出 imgcat 使用的 base64 纯文本（不含 OSC 转义序列）。
pub fn export_base64_text(name: &str, data: &[u8]) {
    axstd::println!("=== ARCEOS_FILE_BEGIN {name} b64text ===");
    write_base64(data);
    axstd::println!("\n=== ARCEOS_FILE_END {name} ===");
}

pub fn write_base64(input: &[u8]) {
    let mut buf = [0u8; FLUSH_BYTES];
    let mut pos = 0usize;

    let mut chunks = input.chunks_exact(3);
    for c in chunks.by_ref() {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        buf[pos] = ALPHABET[((n >> 18) & 0x3f) as usize];
        buf[pos + 1] = ALPHABET[((n >> 12) & 0x3f) as usize];
        buf[pos + 2] = ALPHABET[((n >> 6) & 0x3f) as usize];
        buf[pos + 3] = ALPHABET[(n & 0x3f) as usize];
        pos += 4;
        if pos == FLUSH_BYTES {
            write_bytes(&buf);
            pos = 0;
        }
    }

    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            buf[pos] = ALPHABET[((n >> 18) & 0x3f) as usize];
            buf[pos + 1] = ALPHABET[((n >> 12) & 0x3f) as usize];
            buf[pos + 2] = b'=';
            buf[pos + 3] = b'=';
            pos += 4;
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            buf[pos] = ALPHABET[((n >> 18) & 0x3f) as usize];
            buf[pos + 1] = ALPHABET[((n >> 12) & 0x3f) as usize];
            buf[pos + 2] = ALPHABET[((n >> 6) & 0x3f) as usize];
            buf[pos + 3] = b'=';
            pos += 4;
        }
        _ => {}
    }

    if pos > 0 {
        write_bytes(&buf[..pos]);
    }
}
