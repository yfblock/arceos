//! SG2002 JPU（JPEG Processing Unit）纯 Rust 驱动。
//!
//! 对照 U-Boot CVitek 驱动实现（`drivers/jpeg/`），在裸机 ArceOS 上以轮询方式完成
//! Baseline JPEG 硬件解码，输出 YUV420 planar。
//!
//! # 模块结构
//!
//! - [`regs`]   — `tock-registers` MMIO 布局、位域、TOP 时钟/D-Cache
//! - [`mem`]    — DMA 物理内存池（bitstream / frame buffer）
//! - [`timing`] — 各阶段耗时统计
//! - [`decoder`] — 初始化、JPEG 头解析、寄存器配置、解码主流程
//!
//! # 解码流程概览
//!
//! ```text
//! init() → decode():
//!   1. 软件解析 SOF/DHT/DQT/SOS（marker scan）
//!   2. 拷贝 bitstream 到 DMA 缓冲 + D-Cache clean
//!   3. 分配 Y/Cb/Cr 帧缓冲
//!   4. 写 BBC/GBU/PIC 寄存器，上传 Huffman / 量化表
//!   5. GRAM 预加载熵编码起点（ECS）
//!   6. 写 DPB 地址，启动 JPG_START_PIC，轮询 PIC_STATUS
//!   7. D-Cache invalidate 后 CPU 可读 YUV
//! ```

mod decoder;
mod mem;
pub mod regs;
mod timing;

pub use decoder::{DecodeResult, JpuDecoder};
pub use timing::{DecodeTimings, elapsed_us, now_ticks};
