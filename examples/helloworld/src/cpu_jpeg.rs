//! CPU 软件 JPEG 解码（`zune-jpeg`），用于与 JPU 硬件解码对比耗时。

use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use crate::jpu::{elapsed_us, now_ticks};

/// CPU 解码各阶段耗时（微秒）。
pub struct CpuDecodeTimings {
    /// 解析 JPEG 头（`decode_headers`）
    pub parse_headers_us: u64,
    /// 熵解码 + IDCT + 色彩转换（`decode`）
    pub decode_core_us: u64,
    /// `decode()` 总耗时
    pub decode_total_us: u64,
}

impl CpuDecodeTimings {
    pub fn print_report(&self) {
        axstd::println!("=== CPU Decode Timing (zune-jpeg) ===");
        self.print_line("parse_headers", self.parse_headers_us);
        self.print_line("decode_core", self.decode_core_us);
        self.print_line("decode_total", self.decode_total_us);
    }

    fn print_line(&self, name: &str, us: u64) {
        axstd::println!("  {name:16} {us:>8} us  ({:>7.3} ms)", us as f64 / 1000.0);
    }
}

pub struct CpuDecodeResult {
    pub width: u32,
    pub height: u32,
    /// 输出像素字节数（YCbCr planar-ish packed by zune）
    pub pixels_len: usize,
    pub timings: CpuDecodeTimings,
}

/// Baseline JPEG → YCbCr 输出（软件全路径解码）
pub fn decode(jpeg: &[u8]) -> Result<CpuDecodeResult, &'static str> {
    let total_t0 = now_ticks();

    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::YCbCr);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(jpeg), options);

    let t = now_ticks();
    decoder.decode_headers().map_err(|_| "CPU: decode_headers failed")?;
    let info = decoder.info().ok_or("CPU: missing image info")?;
    let timings_parse = elapsed_us(t);

    let t = now_ticks();
    let pixels = decoder.decode().map_err(|_| "CPU: decode failed")?;
    let timings_core = elapsed_us(t);

    Ok(CpuDecodeResult {
        width: info.width as u32,
        height: info.height as u32,
        pixels_len: pixels.len(),
        timings: CpuDecodeTimings {
            parse_headers_us: timings_parse,
            decode_core_us: timings_core,
            decode_total_us: elapsed_us(total_t0),
        },
    })
}

/// JPU vs CPU 汇总对比
pub fn print_comparison(jpu_total_us: u64, jpu_hw_us: u64, cpu: &CpuDecodeTimings) {
    axstd::println!("=== JPU vs CPU Summary ===");
    axstd::println!(
        "  {:16} {:>8} us  ({:>7.3} ms)",
        "cpu_total",
        cpu.decode_total_us,
        cpu.decode_total_us as f64 / 1000.0
    );
    axstd::println!(
        "  {:16} {:>8} us  ({:>7.3} ms)",
        "jpu_total",
        jpu_total_us,
        jpu_total_us as f64 / 1000.0
    );
    axstd::println!(
        "  {:16} {:>8} us  ({:>7.3} ms)",
        "jpu_hw_only",
        jpu_hw_us,
        jpu_hw_us as f64 / 1000.0
    );

    if jpu_total_us > 0 {
        let ratio = cpu.decode_total_us as f64 / jpu_total_us as f64;
        axstd::println!("  cpu/jpu total:   {ratio:.2}x");
    }
    if jpu_hw_us > 0 {
        let ratio = cpu.decode_core_us as f64 / jpu_hw_us as f64;
        axstd::println!("  cpu_core/jpu_hw: {ratio:.2}x");
    }
}
