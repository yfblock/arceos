//! JPU 解码各阶段耗时（基于 RISC-V `time` 寄存器，25 MHz）。

const TIMER_HZ: u64 = 25_000_000;

#[inline]
pub fn now_ticks() -> u64 {
    axhal::time::current_ticks()
}

#[inline]
pub fn ticks_to_us(ticks: u64) -> u64 {
    ticks.saturating_mul(1_000_000) / TIMER_HZ
}

#[inline]
pub fn elapsed_us(since: u64) -> u64 {
    ticks_to_us(now_ticks().saturating_sub(since))
}

/// 单次 `decode()` 各阶段耗时（微秒）。
pub struct DecodeTimings {
    /// `JpuDecoder::new()` 中 init 耗时（仅首次有意义）
    pub init_us: u64,
    /// 软件 marker 扫描（SOF/DHT/DQT/SOS）
    pub parse_header_us: u64,
    /// JPEG → stream buffer + D-Cache clean
    pub stream_copy_us: u64,
    /// 帧缓冲 free/alloc + D-Cache invalidate
    pub frame_buffer_us: u64,
    /// BBC/PIC/MCU/OP 等寄存器（不含表上传）
    pub register_setup_us: u64,
    /// Huffman MIN/MAX/PTR/VAL 表写入
    pub huff_tables_us: u64,
    /// 量化矩阵写入
    pub quant_tables_us: u64,
    /// BBC 预取 + GBU 比特流指针（ECS 起点）
    pub gram_setup_us: u64,
    /// DPB 地址/stride + 启动 JPG_START_PIC
    pub dpb_start_us: u64,
    /// 硬件解码轮询等待（PIC_STATUS bit0）
    pub hw_decode_us: u64,
    /// 输出 YUV D-Cache invalidate
    pub cache_inv_us: u64,
    /// decode() 总耗时
    pub decode_total_us: u64,
}

impl DecodeTimings {
    pub fn print_report(&self) {
        axstd::println!("=== JPU Decode Timing ===");
        self.print_line("init", self.init_us);
        self.print_line("parse_header", self.parse_header_us);
        self.print_line("stream_copy", self.stream_copy_us);
        self.print_line("frame_mem", self.frame_buffer_us);
        self.print_line("register_setup", self.register_setup_us);
        self.print_line("huff_tables", self.huff_tables_us);
        self.print_line("quant_tables", self.quant_tables_us);
        self.print_line("gram_setup", self.gram_setup_us);
        self.print_line("dpb_start", self.dpb_start_us);
        self.print_line("hw_decode", self.hw_decode_us);
        self.print_line("cache_inv", self.cache_inv_us);
        self.print_line("decode_total", self.decode_total_us);

        let sw_setup = self.parse_header_us
            + self.stream_copy_us
            + self.frame_buffer_us
            + self.register_setup_us
            + self.huff_tables_us
            + self.quant_tables_us
            + self.gram_setup_us
            + self.dpb_start_us
            + self.cache_inv_us;
        self.print_line("sw_setup_sum", sw_setup);
        self.print_line("init+decode", self.init_us + self.decode_total_us);
    }

    fn print_line(&self, name: &str, us: u64) {
        axstd::println!("  {name:16} {us:>8} us  ({:>7.3} ms)", us as f64 / 1000.0);
    }
}
