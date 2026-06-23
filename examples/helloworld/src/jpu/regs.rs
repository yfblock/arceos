//! JPU MMIO 寄存器（`tock-registers`）与平台辅助函数。
//!
//! 布局对齐 U-Boot `regdefine.h`；SG2002 JPU 基址 `0x0B00_0000`。

#![allow(dead_code)]

use tock_registers::{
    interfaces::{Readable, Writeable},
    register_bitfields, register_structs,
    registers::{ReadOnly, ReadWrite},
};

// ── 平台基址 ──────────────────────────────────────────────────────────────────

pub const TOP_BASE: usize = 0x0300_0000;
pub const JPU_REG_BASE: usize = 0x0B00_0000;
pub const VC_REG_BASE: usize = 0x0B03_0000;

pub const TOP_DDR_ADDR_MODE_REG: usize = TOP_BASE + 0x64;
pub const TOP_CLK_JPEG_REG: usize = TOP_BASE + 0x2008;
pub const TOP_RST_JPEG_REG: usize = TOP_BASE + 0x3000;

pub const DAMR_REG_VD_REMAP_ADDR_39_32_OFFSET: u32 = 24;

/// JPEG 像素格式（写入 MCU/DPB 相关寄存器）
pub const FORMAT_420: u32 = 0;
pub const FORMAT_422: u32 = 1;
pub const FORMAT_224: u32 = 2;
pub const FORMAT_444: u32 = 3;
pub const FORMAT_400: u32 = 4;

pub const STREAM_BUF_SIZE: usize = 0x40000;
pub const JPU_DRAM_PHYSICAL_SIZE: usize = 0x0010_0000;
pub const VMEM_PAGE_SIZE: usize = 16 * 1024;

const CACHE_LINE: usize = 64;

// ── 位域定义 ──────────────────────────────────────────────────────────────────

register_bitfields! [
    u32,

    /// 通用 32 位值（地址、数据端口等）
    pub VALUE32 [
        VAL OFFSET(0) NUMBITS(32) []
    ],

    /// PIC_START @ 0x000
    pub MJPEG_PIC_START [
        START_PIC OFFSET(0) NUMBITS(1) [],
        START_INIT OFFSET(1) NUMBITS(1) [],
    ],

    /// PIC_STATUS @ 0x004（写回清除）
    pub MJPEG_PIC_STATUS [
        DONE OFFSET(0) NUMBITS(1) [],
        ERROR OFFSET(1) NUMBITS(1) [],
    ],

    /// PIC_CTRL @ 0x010
    pub MJPEG_PIC_CTRL [
        USER_HUFF_TAB OFFSET(6) NUMBITS(1) [],
        HUFF_DC_IDX OFFSET(7) NUMBITS(3) [],
        HUFF_AC_IDX OFFSET(10) NUMBITS(3) [],
    ],

    /// PIC_SIZE @ 0x014
    pub MJPEG_PIC_SIZE [
        HEIGHT OFFSET(0) NUMBITS(16) [],
        WIDTH OFFSET(16) NUMBITS(16) [],
    ],

    /// BBC_STRM_CTRL @ 0x234
    pub MJPEG_BBC_STRM_CTRL [
        PAGES OFFSET(0) NUMBITS(31) [],
        END_FLAG OFFSET(31) NUMBITS(1) [],
    ],

    /// BBC_BUSY @ 0x224
    pub MJPEG_BBC_BUSY [
        BUSY OFFSET(0) NUMBITS(1) [],
    ],
];

// ── JPU 寄存器块 ──────────────────────────────────────────────────────────────

register_structs! {
    /// CVitek MJPEG/JPU NPT 寄存器组（基址 [`JPU_REG_BASE`]）
    pub JpuRegisters {
        (0x000 => pub pic_start: ReadWrite<u32, MJPEG_PIC_START::Register>),
        (0x004 => pub pic_status: ReadWrite<u32, MJPEG_PIC_STATUS::Register>),
        (0x008 => pub pic_errmb: ReadWrite<u32, VALUE32::Register>),
        (0x00C => _reserved_pic_setmb),
        (0x010 => pub pic_ctrl: ReadWrite<u32, MJPEG_PIC_CTRL::Register>),
        (0x014 => pub pic_size: ReadWrite<u32, MJPEG_PIC_SIZE::Register>),
        (0x018 => pub mcu_info: ReadWrite<u32, VALUE32::Register>),
        (0x01C => pub rot_info: ReadWrite<u32, VALUE32::Register>),
        (0x020 => pub scl_info: ReadWrite<u32, VALUE32::Register>),
        (0x024 => _reserved_if_info),
        (0x028 => pub clp_info: ReadWrite<u32, VALUE32::Register>),
        (0x02C => pub op_info: ReadWrite<u32, VALUE32::Register>),
        (0x030 => pub dpb_config: ReadWrite<u32, VALUE32::Register>),
        (0x034 => pub dpb_base_y: ReadWrite<u32, VALUE32::Register>),
        (0x038 => pub dpb_base_cb: ReadWrite<u32, VALUE32::Register>),
        (0x03C => pub dpb_base_cr: ReadWrite<u32, VALUE32::Register>),
        (0x040 => _reserved_dpb_extra: [u8; 0x24]),
        (0x064 => pub dpb_ystride: ReadWrite<u32, VALUE32::Register>),
        (0x068 => pub dpb_cstride: ReadWrite<u32, VALUE32::Register>),
        (0x06C => _reserved_wresp: [u8; 0x14]),
        (0x080 => pub huff_ctrl: ReadWrite<u32, VALUE32::Register>),
        (0x084 => pub huff_addr: ReadWrite<u32, VALUE32::Register>),
        (0x088 => pub huff_data: ReadWrite<u32, VALUE32::Register>),
        (0x08C => _reserved_huff_pad),
        (0x090 => pub qmat_ctrl: ReadWrite<u32, VALUE32::Register>),
        (0x094 => _reserved_qmat_addr),
        (0x098 => pub qmat_data: ReadWrite<u32, VALUE32::Register>),
        (0x09C => _reserved_coef: [u8; 0x14]),
        (0x0B0 => pub rst_intval: ReadWrite<u32, VALUE32::Register>),
        (0x0B4 => pub rst_index: ReadWrite<u32, VALUE32::Register>),
        (0x0B8 => pub rst_count: ReadWrite<u32, VALUE32::Register>),
        (0x0BC => _reserved_rst_pad: [u8; 0x34]),
        (0x0F0 => pub dpcm_diff_y: ReadWrite<u32, VALUE32::Register>),
        (0x0F4 => pub dpcm_diff_cb: ReadWrite<u32, VALUE32::Register>),
        (0x0F8 => pub dpcm_diff_cr: ReadWrite<u32, VALUE32::Register>),
        (0x0FC => _reserved_dpcm_pad),
        (0x100 => pub gbu_ctrl: ReadWrite<u32, VALUE32::Register>),
        (0x104 => _reserved_gbu_mid: [u8; 0x10]),
        (0x114 => pub gbu_wd_ptr: ReadWrite<u32, VALUE32::Register>),
        (0x118 => pub gbu_tt_cnt: ReadWrite<u32, VALUE32::Register>),
        (0x11C => pub gbu_tt_cnt_h: ReadWrite<u32, VALUE32::Register>),
        (0x120 => _reserved_gbu_pbit: [u8; 0x20]),
        (0x140 => pub gbu_bbsr: ReadWrite<u32, VALUE32::Register>),
        (0x144 => pub gbu_bber: ReadWrite<u32, VALUE32::Register>),
        (0x148 => pub gbu_bbir: ReadWrite<u32, VALUE32::Register>),
        (0x14C => pub gbu_bbhr: ReadWrite<u32, VALUE32::Register>),
        (0x150 => _reserved_gbu_tail: [u8; 0x10]),
        (0x160 => pub gbu_ff_rptr: ReadWrite<u32, VALUE32::Register>),
        (0x164 => _reserved_bbc_gap: [u8; 0xA4]),
        (0x208 => pub bbc_end_addr: ReadWrite<u32, VALUE32::Register>),
        (0x20C => pub bbc_wr_ptr: ReadWrite<u32, VALUE32::Register>),
        (0x210 => pub bbc_rd_ptr: ReadWrite<u32, VALUE32::Register>),
        (0x214 => pub bbc_ext_addr: ReadWrite<u32, VALUE32::Register>),
        (0x218 => pub bbc_int_addr: ReadWrite<u32, VALUE32::Register>),
        (0x21C => pub bbc_data_cnt: ReadWrite<u32, VALUE32::Register>),
        (0x220 => pub bbc_command: ReadWrite<u32, VALUE32::Register>),
        (0x224 => pub bbc_busy: ReadOnly<u32, MJPEG_BBC_BUSY::Register>),
        (0x228 => pub bbc_ctrl: ReadWrite<u32, VALUE32::Register>),
        (0x22C => pub bbc_cur_pos: ReadWrite<u32, VALUE32::Register>),
        (0x230 => pub bbc_bas_addr: ReadWrite<u32, VALUE32::Register>),
        (0x234 => pub bbc_strm_ctrl: ReadWrite<u32, MJPEG_BBC_STRM_CTRL::Register>),
        (0x238 => @END),
    }
}

/// JPU MMIO 寄存器块引用
#[inline]
pub fn jpu_regs() -> &'static JpuRegisters {
    unsafe { &*(JPU_REG_BASE as *const JpuRegisters) }
}

// ── TOP / VC 访问（非连续布局，保留按地址读写）──────────────────────────────

#[inline]
pub fn top_read_reg(addr: usize) -> u32 {
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

#[inline]
pub fn top_write_reg(addr: usize, value: u32) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
}

// ── D-Cache（C906 custom CSR）────────────────────────────────────────────────

pub fn dcache_clean_range(start: usize, size: usize) {
    let mut addr = start & !(CACHE_LINE - 1);
    let end = start.saturating_add(size);
    while addr < end {
        unsafe {
            core::arch::asm!(".word 0x0295000b", in("a0") addr);
        }
        addr += CACHE_LINE;
    }
    unsafe {
        core::arch::asm!(".word 0x0190000b");
    }
}

pub fn dcache_inv_range(start: usize, size: usize) {
    let mut addr = start & !(CACHE_LINE - 1);
    let end = start.saturating_add(size);
    while addr < end {
        unsafe {
            core::arch::asm!(".word 0x02a5000b", in("a0") addr);
        }
        addr += CACHE_LINE;
    }
    unsafe {
        core::arch::asm!(".word 0x0190000b");
    }
}

// ── 辅助操作 ──────────────────────────────────────────────────────────────────

/// 写回 PIC_STATUS 清除状态位（W1C 语义）
#[inline]
pub fn pic_status_clear(status: u32) {
    jpu_regs().pic_status.set(status);
}

/// 等待 JPU 软件复位完成
pub fn wait_sw_reset_done() {
    jpu_regs().pic_start.write(MJPEG_PIC_START::START_INIT::SET);
    for _ in 0..100_000 {
        if !jpu_regs().pic_start.is_set(MJPEG_PIC_START::START_INIT) {
            return;
        }
        core::hint::spin_loop();
    }
}

/// 等待 BBC 命令完成
pub fn wait_bbc_idle() {
    for _ in 0..100_000 {
        if !jpu_regs().bbc_busy.is_set(MJPEG_BBC_BUSY::BUSY) {
            return;
        }
        core::hint::spin_loop();
    }
}

/// 组装 PIC_CTRL：三通道 DC/AC Huffman 表索引 + userHuffTab
#[inline]
pub fn pic_ctrl_value(dc_idx: u32, ac_idx: u32) -> u32 {
    (MJPEG_PIC_CTRL::HUFF_AC_IDX.val(ac_idx)
        + MJPEG_PIC_CTRL::HUFF_DC_IDX.val(dc_idx)
        + MJPEG_PIC_CTRL::USER_HUFF_TAB::SET)
    .into()
}

/// BBC stream end + 页数
#[inline]
pub fn bbc_strm_ctrl_end(pages: u32) -> u32 {
    (MJPEG_BBC_STRM_CTRL::END_FLAG::SET + MJPEG_BBC_STRM_CTRL::PAGES.val(pages)).into()
}
