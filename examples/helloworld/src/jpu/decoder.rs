//! JPU 硬件 JPEG 解码器（Baseline，轮询模式）。
//!
//! 软件负责解析 JPEG 头（SOF/DHT/DQT/SOS），将 Huffman/量化表与 bitstream 写入
//! DMA 缓冲，配置 BBC/GBU/DPB 寄存器后启动 `JPG_START_PIC`，轮询 `PIC_STATUS` 完成。

use super::mem::{init_jpu_memory, jpu_alloc, jpu_free};
use super::regs::{
    bbc_strm_ctrl_end, dcache_clean_range, dcache_inv_range, jpu_regs, pic_ctrl_value,
    pic_status_clear, top_read_reg, top_write_reg, wait_bbc_idle, wait_sw_reset_done,
    DAMR_REG_VD_REMAP_ADDR_39_32_OFFSET, FORMAT_400, FORMAT_420, FORMAT_422, FORMAT_224,
    FORMAT_444, MJPEG_PIC_SIZE, MJPEG_PIC_START, MJPEG_PIC_STATUS, STREAM_BUF_SIZE,
    TOP_CLK_JPEG_REG, TOP_DDR_ADDR_MODE_REG, TOP_RST_JPEG_REG, VC_REG_BASE,
};
use super::timing::{DecodeTimings, elapsed_us, now_ticks};
use tock_registers::interfaces::{Readable, Writeable};

// ── 公开类型 ──────────────────────────────────────────────────────────────────

/// 解码结果：YUV420 planar，数据位于 DMA 帧缓冲（`'static` 生命周期至下次 decode/Drop）
pub struct DecodeResult {
    pub width: u32,
    pub height: u32,
    pub yuv_data: &'static [u8],
    pub yuv_phys_addr: usize,
    pub timings: DecodeTimings,
}

/// JPU 解码器实例（持有 stream/frame DMA 缓冲）
pub struct JpuDecoder {
    stream_buf_phys: usize,
    stream_buf_size: usize,
    frame_buf_phys: usize,
    frame_buf_size: usize,
    initialized: bool,
    pub init_us: u64,
}

// ── JPEG 头解析辅助结构 ───────────────────────────────────────────────────────

/// Huffman 表：从 DHT 段解析后生成 MIN/MAX/PTR 供硬件上传
struct HuffTable {
    bits: [u8; 16],
    values: [u8; 256],
    num_values: usize,
    min_codes: [u32; 16],
    max_codes: [u32; 16],
    ptrs: [u8; 16],
}

impl HuffTable {
    fn new() -> Self {
        Self {
            bits: [0; 16],
            values: [0; 256],
            num_values: 0,
            min_codes: [0xFFFF; 16],
            max_codes: [0xFFFF; 16],
            ptrs: [0xFF; 16],
        }
    }

    fn sign_extend_16(huff_data: u32) -> u32 {
        if huff_data & 0x8000 != 0 {
            0xFFFF
        } else {
            0
        }
    }

    fn sign_extend_8(huff_data: u32) -> u32 {
        if huff_data & 0x80 != 0 {
            0xFFFFFF
        } else {
            0
        }
    }

    /// 由 BITS/VALUES 生成 canonical Huffman MIN/MAX/PTR（对齐 `JpgDecHuffTabSetUp`）
    fn generate(&mut self) {
        let mut ptr_cnt: usize = 0;
        let mut huff_code: u32 = 0;
        let mut zero_flag = false;
        let mut data_flag = false;

        for i in 0..16 {
            if self.bits[i] != 0 {
                self.ptrs[i] = ptr_cnt as u8;
                ptr_cnt += self.bits[i] as usize;
                self.min_codes[i] = huff_code;
                self.max_codes[i] = huff_code + (self.bits[i] as u32 - 1);
                data_flag = true;
                zero_flag = false;
            } else {
                self.ptrs[i] = 0xFF;
                self.min_codes[i] = 0xFFFF;
                self.max_codes[i] = 0xFFFF;
                zero_flag = true;
            }

            if data_flag {
                if zero_flag {
                    huff_code <<= 1;
                } else {
                    huff_code = (self.max_codes[i] + 1) << 1;
                }
            }
        }
    }
}

struct QuantTable {
    values: [u16; 64],
}

impl QuantTable {
    fn new() -> Self {
        Self { values: [0; 64] }
    }
}

/// 从 JPEG bitstream 扫描得到的头信息
struct JpegHeaderInfo {
    width: u32,
    height: u32,
    num_components: u32,
    format: u32,
    /// ECS（熵编码段）在 bitstream 中的字节偏移
    ecs_offset: usize,
    restart_interval: u32,
    dc_huff_tbl: [usize; 3],
    ac_huff_tbl: [usize; 3],
    quant_tbl: [usize; 3],
    huff_tables: [HuffTable; 4],
    quant_tables: [QuantTable; 4],
    huff_table_count: usize,
    quant_table_count: usize,
}

impl JpegHeaderInfo {
    fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            num_components: 0,
            format: FORMAT_420,
            ecs_offset: 0,
            restart_interval: 0,
            dc_huff_tbl: [0; 3],
            ac_huff_tbl: [0; 3],
            quant_tbl: [0; 3],
            huff_tables: [HuffTable::new(), HuffTable::new(), HuffTable::new(), HuffTable::new()],
            quant_tables: [QuantTable::new(), QuantTable::new(), QuantTable::new(), QuantTable::new()],
            huff_table_count: 0,
            quant_table_count: 0,
        }
    }
}

// ── 初始化 ────────────────────────────────────────────────────────────────────

impl JpuDecoder {
    pub fn new() -> Result<Self, &'static str> {
        let mut decoder = Self {
            stream_buf_phys: 0,
            stream_buf_size: STREAM_BUF_SIZE,
            frame_buf_phys: 0,
            frame_buf_size: 0,
            initialized: false,
            init_us: 0,
        };

        let t0 = now_ticks();
        decoder.init()?;
        decoder.init_us = elapsed_us(t0);
        Ok(decoder)
    }

    /// 时钟/复位、DDR remap、DMA 池与 stream buffer 分配
    fn init(&mut self) -> Result<(), &'static str> {
        init_jpu_memory();

        // TOP：JPEG + VC 时钟
        let clk_val = top_read_reg(TOP_CLK_JPEG_REG);
        top_write_reg(TOP_CLK_JPEG_REG, clk_val | 0x3300);

        // TOP：释放 JPEG 复位
        let rst_val = top_read_reg(TOP_RST_JPEG_REG);
        top_write_reg(TOP_RST_JPEG_REG, rst_val | (1 << 4));

        // DDR 地址 remap（VD 39:32）
        let damr = top_read_reg(TOP_DDR_ADDR_MODE_REG);
        top_write_reg(
            TOP_DDR_ADDR_MODE_REG,
            damr | (1 << DAMR_REG_VD_REMAP_ADDR_39_32_OFFSET),
        );

        // VC 子块时钟使能
        let vc_val = top_read_reg(VC_REG_BASE);
        top_write_reg(VC_REG_BASE, vc_val | 0x1F);
        let _ = top_read_reg(VC_REG_BASE);

        let _ = jpu_regs().pic_status.get();

        // U-Boot 参考驱动中的预热写（固定物理地址，与后续 DMA 基址无关）
        jpu_regs().bbc_bas_addr.set(0x8026C000);
        let _ = jpu_regs().bbc_bas_addr.get();

        wait_sw_reset_done();

        self.stream_buf_phys = jpu_alloc(STREAM_BUF_SIZE).ok_or("Failed to allocate stream buffer")?;
        self.initialized = true;
        Ok(())
    }

    // ── JPEG marker 扫描 ────────────────────────────────────────────────────────

    /// 扫描 SOF/DHT/DQT/SOS 等 marker，填充 [`JpegHeaderInfo`]
    fn parse_jpeg_header(&self, data: &[u8]) -> Result<JpegHeaderInfo, &'static str> {
        let mut i = 0;
        let mut header_info = JpegHeaderInfo::new();

        while i < data.len() - 1 {
            if data[i] == 0xFF {
                let marker = data[i + 1];

                if marker == 0xFF {
                    i += 1;
                    continue;
                }
                if marker == 0x00 {
                    i += 2;
                    continue;
                }

                match marker {
                    // SOF0 / SOF2
                    0xC0 | 0xC2 => {
                        if i + 10 >= data.len() {
                            return Err("SOF too short");
                        }

                        header_info.height = ((data[i + 5] as u32) << 8) | (data[i + 6] as u32);
                        header_info.width = ((data[i + 7] as u32) << 8) | (data[i + 8] as u32);
                        header_info.num_components = data[i + 9] as u32;

                        if header_info.num_components == 3 {
                            let comp_start = i + 10;
                            if comp_start + 9 <= data.len() {
                                let h1 = (data[comp_start + 1] >> 4) & 0x0F;
                                let v1 = data[comp_start + 1] & 0x0F;
                                let h2 = (data[comp_start + 4] >> 4) & 0x0F;
                                let v2 = data[comp_start + 4] & 0x0F;

                                header_info.quant_tbl[0] = data[comp_start + 2] as usize;
                                header_info.quant_tbl[1] = data[comp_start + 5] as usize;
                                header_info.quant_tbl[2] = data[comp_start + 8] as usize;

                                header_info.format = if h1 == 2 && v1 == 2 && h2 == 1 && v2 == 1 {
                                    FORMAT_420
                                } else if h1 == 2 && v1 == 1 {
                                    FORMAT_422
                                } else if h1 == 1 && v1 == 2 {
                                    FORMAT_224
                                } else {
                                    FORMAT_444
                                };
                            }
                        } else {
                            header_info.format = FORMAT_400;
                        }

                        if i + 3 < data.len() {
                            let length = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
                            i += 2 + length;
                            continue;
                        }
                    }
                    // DHT
                    0xC4 => {
                        if i + 3 < data.len() {
                            let length = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
                            self.parse_dht(data, i + 4, i + 2 + length, &mut header_info)?;
                            i += 2 + length;
                            continue;
                        }
                    }
                    // SOS — ECS 起点 = marker + 2 + Ls
                    0xDA => {
                        if i + 3 < data.len() {
                            let sos_length = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);

                            if i + 5 < data.len() {
                                let num_scan_components = data[i + 4] as usize;
                                let mut comp_offset = i + 5;
                                for comp_idx in 0..num_scan_components.min(3) {
                                    if comp_offset + 2 <= data.len() {
                                        let tables = data[comp_offset + 1];
                                        header_info.dc_huff_tbl[comp_idx] =
                                            ((tables >> 4) & 0x0F) as usize;
                                        header_info.ac_huff_tbl[comp_idx] = (tables & 0x0F) as usize;
                                        comp_offset += 2;
                                    }
                                }
                            }

                            header_info.ecs_offset = i + 2 + sos_length;
                            return Ok(header_info);
                        }
                    }
                    // DQT
                    0xDB => {
                        if i + 3 < data.len() {
                            let length = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
                            self.parse_dqt(data, i + 4, i + 2 + length, &mut header_info)?;
                            i += 2 + length;
                            continue;
                        }
                    }
                    // DRI
                    0xDD => {
                        if i + 6 <= data.len() {
                            header_info.restart_interval =
                                ((data[i + 4] as u32) << 8) | (data[i + 5] as u32);
                        }
                        if i + 3 < data.len() {
                            let length = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
                            i += 2 + length;
                            continue;
                        }
                    }
                    0xD8 => {
                        i += 2;
                        continue;
                    }
                    0xD9 => break,
                    _ => {
                        if marker >= 0xC0 && i + 3 < data.len() {
                            let length = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
                            i += 2 + length;
                            continue;
                        }
                        i += 2;
                        continue;
                    }
                }
            }
            i += 1;
        }

        Err("SOS not found")
    }

    fn parse_dht(
        &self,
        data: &[u8],
        start: usize,
        end: usize,
        header_info: &mut JpegHeaderInfo,
    ) -> Result<(), &'static str> {
        let mut offset = start;

        while offset < end && offset + 1 < data.len() {
            let tc_th = data[offset];
            let tc = (tc_th >> 4) & 0x0F;
            let th = tc_th & 0x0F;
            // table_idx = (Th&1)<<1 | (Tc&1) → DC0/AC0/DC1/AC1
            let table_idx: usize = (((th & 1) << 1) | (tc & 1)) as usize;

            let mut num_values = 0;
            for j in 0..16 {
                if offset + 1 + j < data.len() {
                    header_info.huff_tables[table_idx].bits[j] = data[offset + 1 + j];
                    num_values += data[offset + 1 + j] as usize;
                }
            }

            for j in 0..num_values {
                if offset + 17 + j < data.len() {
                    header_info.huff_tables[table_idx].values[j] = data[offset + 17 + j];
                }
            }
            header_info.huff_tables[table_idx].num_values = num_values;
            header_info.huff_tables[table_idx].generate();

            if table_idx >= header_info.huff_table_count {
                header_info.huff_table_count = table_idx + 1;
            }

            offset += 17 + num_values;
        }

        Ok(())
    }

    fn parse_dqt(
        &self,
        data: &[u8],
        start: usize,
        end: usize,
        header_info: &mut JpegHeaderInfo,
    ) -> Result<(), &'static str> {
        let mut offset = start;

        while offset < end && offset + 1 < data.len() {
            let pq_tq = data[offset];
            let tq: usize = (pq_tq & 0x0F) as usize;

            if pq_tq >> 4 == 0 {
                for j in 0..64 {
                    if offset + 1 + j < data.len() {
                        header_info.quant_tables[tq].values[j] = data[offset + 1 + j] as u16;
                    }
                }
                offset += 1 + 64;
            } else {
                for j in 0..64 {
                    if offset + 1 + j * 2 + 1 < data.len() {
                        header_info.quant_tables[tq].values[j] = ((data[offset + 1 + j * 2] as u16)
                            << 8)
                            | (data[offset + 1 + j * 2 + 1] as u16);
                    }
                }
                offset += 1 + 128;
            }

            if tq >= header_info.quant_table_count {
                header_info.quant_table_count = tq + 1;
            }
        }

        Ok(())
    }

    // ── 解码主流程 ──────────────────────────────────────────────────────────────

    pub fn decode(&mut self, jpeg_data: &[u8]) -> Result<DecodeResult, &'static str> {
        if !self.initialized {
            return Err("JPU not initialized");
        }

        let decode_t0 = now_ticks();
        let mut timings = DecodeTimings {
            init_us: self.init_us,
            parse_header_us: 0,
            stream_copy_us: 0,
            frame_buffer_us: 0,
            register_setup_us: 0,
            huff_tables_us: 0,
            quant_tables_us: 0,
            gram_setup_us: 0,
            dpb_start_us: 0,
            hw_decode_us: 0,
            cache_inv_us: 0,
            decode_total_us: 0,
        };

        // 1. 软件解析 JPEG 头
        let t = now_ticks();
        let header_info = self.parse_jpeg_header(jpeg_data)?;
        timings.parse_header_us = elapsed_us(t);

        // 2. 拷贝 bitstream 到 DMA 缓冲并 clean cache
        let t = now_ticks();
        let copy_len = jpeg_data.len().min(self.stream_buf_size);
        unsafe {
            core::ptr::copy_nonoverlapping(
                jpeg_data.as_ptr(),
                self.stream_buf_phys as *mut u8,
                copy_len,
            );
        }
        dcache_clean_range(self.stream_buf_phys, copy_len);
        timings.stream_copy_us = elapsed_us(t);

        // 3. 计算对齐后的帧尺寸与 stride（对齐 mixer.c AllocateFrameBuffer）
        let aligned_width = match header_info.format {
            FORMAT_420 | FORMAT_422 => ((header_info.width + 15) / 16) * 16,
            _ => ((header_info.width + 7) / 8) * 8,
        };
        let aligned_height = match header_info.format {
            FORMAT_420 | FORMAT_224 => ((header_info.height + 15) / 16) * 16,
            _ => ((header_info.height + 7) / 8) * 8,
        };
        let stride_y = aligned_width;
        let stride_c = match header_info.format {
            FORMAT_420 | FORMAT_422 => aligned_width / 2,
            FORMAT_400 => 0,
            _ => aligned_width,
        };

        let luma_size = (stride_y * aligned_height) as usize;
        let chroma_size = match header_info.format {
            FORMAT_420 => (stride_c * aligned_height / 2) as usize,
            FORMAT_422 | FORMAT_224 => luma_size / 2,
            FORMAT_444 => luma_size,
            FORMAT_400 => 0,
            _ => (stride_c * aligned_height / 2) as usize,
        };
        let frame_size = luma_size + chroma_size * 2;

        // 4. 分配/复用帧缓冲
        let t = now_ticks();
        if self.frame_buf_phys != 0 {
            jpu_free(self.frame_buf_phys, self.frame_buf_size);
            self.frame_buf_phys = 0;
            self.frame_buf_size = 0;
        }
        self.frame_buf_phys = jpu_alloc(frame_size).ok_or("Failed to alloc frame buf")?;
        self.frame_buf_size = frame_size;
        dcache_inv_range(self.frame_buf_phys, frame_size);
        timings.frame_buffer_us = elapsed_us(t);

        // 5. BBC / PIC / MCU 寄存器（对齐 JPU_DecStartOneFrame）
        let t = now_ticks();
        let r = jpu_regs();
        let stream_phys = self.stream_buf_phys as u32;
        let stream_end = (self.stream_buf_phys + copy_len) as u32;

        r.bbc_bas_addr.set(stream_phys);
        r.bbc_end_addr.set(stream_end);
        r.bbc_rd_ptr.set(stream_phys);
        r.bbc_wr_ptr.set(stream_end);

        let mut strm_pages = copy_len / 256;
        if copy_len % 256 != 0 {
            strm_pages += 1;
        }
        r.bbc_strm_ctrl.set(bbc_strm_ctrl_end(strm_pages as u32));

        r.gbu_tt_cnt.set(0);
        r.gbu_tt_cnt_h.set(0);
        r.pic_errmb.set(0);

        // 三通道 DC/AC Huffman 索引打包（jpuapifunc.c JpegDecodeHeader）
        let mut huff_dc_idx = 0u32;
        let mut huff_ac_idx = 0u32;
        for i in 0..3 {
            huff_dc_idx = (huff_dc_idx << 1) | header_info.dc_huff_tbl[i] as u32;
            huff_ac_idx = (huff_ac_idx << 1) | header_info.ac_huff_tbl[i] as u32;
        }
        r.pic_ctrl.set(pic_ctrl_value(huff_dc_idx, huff_ac_idx));

        r.pic_size.write(
            MJPEG_PIC_SIZE::WIDTH.val(aligned_width) + MJPEG_PIC_SIZE::HEIGHT.val(aligned_height),
        );
        r.rot_info.set(0);

        let (mcu_block_num, comp_info) = match header_info.format {
            FORMAT_420 => (6, (10 << 8) | (5 << 4) | 5),
            FORMAT_422 => (4, (9 << 8) | (5 << 4) | 5),
            FORMAT_224 => (4, (6 << 8) | (5 << 4) | 5),
            FORMAT_444 => (3, (5 << 8) | (5 << 4) | 5),
            FORMAT_400 => (1, (5 << 8) | 0),
            _ => (6, (10 << 8) | (5 << 4) | 5),
        };
        r.mcu_info
            .set((mcu_block_num << 16) | (header_info.num_components << 12) | comp_info);

        r.dpb_config.set(0); // 4:2:0 planar, little endian
        r.rst_intval.set(header_info.restart_interval);
        r.scl_info.set(0);

        let bus_req_num = match header_info.format {
            FORMAT_420 => 2,
            FORMAT_422 | FORMAT_224 => 3,
            FORMAT_444 | FORMAT_400 => 4,
            _ => 2,
        };
        r.op_info.set(bus_req_num);
        timings.register_setup_us = elapsed_us(t);

        // 6. Huffman / 量化表上传
        let t = now_ticks();
        self.set_huff_tables(&header_info)?;
        timings.huff_tables_us = elapsed_us(t);

        let t = now_ticks();
        self.set_quant_tables(&header_info)?;
        timings.quant_tables_us = elapsed_us(t);

        // 7. GRAM 预加载（BBC 从 ECS 起点取 2 页）
        let t = now_ticks();
        self.gram_setup(&header_info)?;
        timings.gram_setup_us = elapsed_us(t);

        // 8. DPB 地址、GBU 比特指针、启动解码
        let t = now_ticks();
        let r = jpu_regs();
        r.rst_index.set(0);
        r.rst_count.set(0);
        r.dpcm_diff_y.set(0);
        r.dpcm_diff_cb.set(0);
        r.dpcm_diff_cr.set(0);

        let bit_ptr = (header_info.ecs_offset & 0xF) << 3;
        r.gbu_ff_rptr.set(bit_ptr as u32);
        r.gbu_ctrl.set(3);

        r.dpb_base_y.set(self.frame_buf_phys as u32);
        let cb_phys = self.frame_buf_phys + luma_size;
        r.dpb_base_cb.set(cb_phys as u32);
        let cr_phys = cb_phys + chroma_size;
        r.dpb_base_cr.set(cr_phys as u32);

        r.dpb_ystride.set(stride_y);
        r.dpb_cstride.set(stride_c);
        // 非 ROI 解码时 CLP_INFO=0（与 U-Boot 一致）
        r.clp_info.set(0);

        pic_status_clear(r.pic_status.get());
        r.pic_start.write(MJPEG_PIC_START::START_PIC::SET);
        timings.dpb_start_us = elapsed_us(t);

        // 9. 轮询硬件完成
        let t = now_ticks();
        self.poll_decode_done()?;
        timings.hw_decode_us = elapsed_us(t);

        // 10. invalidate 帧缓冲供 CPU 读取
        let t = now_ticks();
        dcache_inv_range(self.frame_buf_phys, frame_size);
        timings.cache_inv_us = elapsed_us(t);
        timings.decode_total_us = elapsed_us(decode_t0);

        Ok(DecodeResult {
            width: header_info.width,
            height: header_info.height,
            yuv_data: unsafe {
                core::slice::from_raw_parts(self.frame_buf_phys as *const u8, frame_size)
            },
            yuv_phys_addr: self.frame_buf_phys,
            timings,
        })
    }

    /// 轮询 PIC_STATUS 直至完成或报错
    fn poll_decode_done(&self) -> Result<(), &'static str> {
        let mut count = 0u32;
        const MAX_POLLS: u32 = 500_000;
        let r = jpu_regs();

        loop {
            if r.pic_status.is_set(MJPEG_PIC_STATUS::DONE) {
                pic_status_clear(r.pic_status.get());
                return Ok(());
            }

            if r.pic_status.is_set(MJPEG_PIC_STATUS::ERROR) {
                let status = r.pic_status.get();
                let err_mb = r.pic_errmb.get();
                axstd::println!("[JPU] Error! status=0x{:x}, err_mb=0x{:x}", status, err_mb);
                pic_status_clear(status);
                return Err("JPU decode error");
            }

            for _ in 0..1000 {
                core::hint::spin_loop();
            }
            count += 1;

            if count >= MAX_POLLS {
                let status = r.pic_status.get();
                axstd::println!("[JPU] Timeout! status=0x{:x}, polls={}", status, count);
                return Err("JPU decode timeout");
            }
        }
    }

    // ── 表上传 / GRAM ───────────────────────────────────────────────────────────

    /// 上传 MIN/MAX/PTR/VAL 四组 Huffman 表（PTR 为 8 位格式，见 JpgDecHuffTabSetUp）
    fn set_huff_tables(&self, header_info: &JpegHeaderInfo) -> Result<(), &'static str> {
        let r = jpu_regs();

        // MIN
        r.huff_ctrl.set(0x003);
        for table_idx in [0, 2, 1, 3] {
            for j in 0..16 {
                let huff_data = header_info.huff_tables[table_idx].min_codes[j];
                let temp = HuffTable::sign_extend_16(huff_data);
                r.huff_data.set(((temp & 0xFFFF) << 16) | huff_data);
            }
        }

        // MAX
        r.huff_ctrl.set(0x403);
        r.huff_addr.set(0x440);
        for table_idx in [0, 2, 1, 3] {
            for j in 0..16 {
                let huff_data = header_info.huff_tables[table_idx].max_codes[j];
                let temp = HuffTable::sign_extend_16(huff_data);
                r.huff_data.set(((temp & 0xFFFF) << 16) | huff_data);
            }
        }

        // PTR（8-bit：(sign24<<8)|byte，0xFF → 0xFFFFFFFF 在 VAL 填充）
        r.huff_ctrl.set(0x803);
        r.huff_addr.set(0x880);
        for table_idx in [0, 2, 1, 3] {
            for j in 0..16 {
                let huff_data = header_info.huff_tables[table_idx].ptrs[j] as u32;
                let temp = HuffTable::sign_extend_8(huff_data);
                r.huff_data.set(((temp & 0xFFFFFF) << 8) | huff_data);
            }
        }

        // VAL
        r.huff_ctrl.set(0xC03);
        for &table_idx in &[0, 2, 1, 3] {
            let is_dc = table_idx == 0 || table_idx == 2;
            let max_count = if is_dc { 12 } else { 162 };
            let bits_len = if is_dc { 12 } else { 16 };
            let count: usize = header_info.huff_tables[table_idx].bits[..bits_len]
                .iter()
                .map(|&b| b as usize)
                .sum();

            for j in 0..count.min(header_info.huff_tables[table_idx].num_values) {
                let val = header_info.huff_tables[table_idx].values[j] as u32;
                let temp = HuffTable::sign_extend_8(val);
                r.huff_data.set(((temp & 0xFFFFFF) << 8) | val);
            }
            for _ in count..max_count {
                r.huff_data.set(0xFFFFFFFF);
            }
        }

        r.huff_ctrl.set(0x000);
        Ok(())
    }

    fn set_quant_tables(&self, header_info: &JpegHeaderInfo) -> Result<(), &'static str> {
        let r = jpu_regs();
        let qmat_ctrl_values = [0x03u32, 0x43, 0x83];
        for comp_idx in 0..3.min(header_info.num_components as usize) {
            let table_idx = header_info.quant_tbl[comp_idx];
            if table_idx >= 4 || table_idx >= header_info.quant_table_count {
                continue;
            }

            r.qmat_ctrl.set(qmat_ctrl_values[comp_idx]);
            for j in 0..64 {
                r.qmat_data
                    .set(header_info.quant_tables[table_idx].values[j] as u32);
            }
            r.qmat_ctrl.set(0x00);
        }
        Ok(())
    }

    /// BBC 预加载 ECS 起点 2 页，设置 GBU 字/比特指针（JpgDecGramSetup）
    fn gram_setup(&self, header_info: &JpegHeaderInfo) -> Result<(), &'static str> {
        let r = jpu_regs();
        let ecs_offset = header_info.ecs_offset;
        let page_ptr = ecs_offset >> 8;
        let mut word_ptr = (ecs_offset & 0xF0) >> 2;
        let bit_ptr = (ecs_offset & 0xF) << 3;

        if page_ptr & 1 != 0 {
            word_ptr += 64;
        }
        if word_ptr & 1 != 0 {
            word_ptr -= 1;
        }

        for i in 0..2 {
            let cur_page = page_ptr + i;
            r.bbc_cur_pos.set(cur_page as u32);
            r.bbc_ext_addr
                .set((self.stream_buf_phys as u32) + ((cur_page as u32) << 8));
            r.bbc_int_addr.set(((cur_page & 1) as u32) << 6);
            r.bbc_data_cnt.set(256 / 4);
            r.bbc_command.set(0);
            wait_bbc_idle();
        }

        r.bbc_cur_pos.set((page_ptr + 2) as u32);
        r.bbc_ctrl.set(1);

        r.gbu_wd_ptr.set(word_ptr as u32);
        r.gbu_bbsr.set(0);
        r.gbu_bber.set(((256 / 4) * 2) - 1);

        if page_ptr & 1 != 0 {
            r.gbu_bbir.set(0);
            r.gbu_bbhr.set(0);
        } else {
            r.gbu_bbir.set(256 / 4);
            r.gbu_bbhr.set(256 / 4);
        }

        r.gbu_ctrl.set(4);
        r.gbu_ff_rptr.set(bit_ptr as u32);
        Ok(())
    }
}

impl Drop for JpuDecoder {
    fn drop(&mut self) {
        if self.stream_buf_phys != 0 {
            jpu_free(self.stream_buf_phys, self.stream_buf_size);
        }
        if self.frame_buf_phys != 0 {
            jpu_free(self.frame_buf_phys, self.frame_buf_size);
        }
    }
}
