//! JPU JPEG decode via U-Boot CVitek C driver (FFI).

use core::ptr::addr_of_mut;

use crate::jpu::regs::{dcache_clean_range, dcache_inv_range};

const BS_BUF_SIZE: usize = 256 * 1024;
const YUV_BUF_SIZE: usize = 64 * 1024;

static mut BS_BUF: [u8; BS_BUF_SIZE] = [0; BS_BUF_SIZE];
static mut YUV_BUF: [u8; YUV_BUF_SIZE] = [0; YUV_BUF_SIZE];
static mut JPU_POOL: [u8; 2 * 1024 * 1024] = [0; 2 * 1024 * 1024];
static mut JPU_POOL_INIT: bool = false;

unsafe extern "C" {
    fn arceos_jpeg_decode(bs_addr: *mut u8, yuv_addr: *mut u8, size: i32) -> i32;
    fn get_jpeg_size(width_addr: *mut i32, height_addr: *mut i32) -> i32;
    fn arceos_jpu_set_pool(addr: *mut u8, size: usize);
}

pub struct DecodeResult {
    pub width: u32,
    pub height: u32,
    pub yuv_data: &'static [u8],
    pub yuv_phys_addr: usize,
}

pub enum DecodeError {
    TooLarge,
    DriverFailed,
    InvalidSize,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "JPEG bitstream too large"),
            Self::DriverFailed => write!(f, "CVitek JPEG driver failed"),
            Self::InvalidSize => write!(f, "driver returned invalid image size"),
        }
    }
}

unsafe fn ensure_jpu_pool() {
    if !JPU_POOL_INIT {
        let pool_ptr = addr_of_mut!(JPU_POOL);
        arceos_jpu_set_pool(
            (*pool_ptr).as_mut_ptr(),
            core::mem::size_of_val(&*pool_ptr),
        );
        JPU_POOL_INIT = true;
    }
}

pub fn decode(jpeg: &[u8]) -> Result<DecodeResult, DecodeError> {
    if jpeg.len() > BS_BUF_SIZE {
        return Err(DecodeError::TooLarge);
    }

    unsafe {
        ensure_jpu_pool();

        let bs_ptr = addr_of_mut!(BS_BUF);
        let yuv_ptr = addr_of_mut!(YUV_BUF);
        let bs_phys = bs_ptr as usize;
        let yuv_phys = yuv_ptr as usize;

        core::ptr::copy_nonoverlapping(jpeg.as_ptr(), (*bs_ptr).as_mut_ptr(), jpeg.len());
        dcache_clean_range(bs_phys, jpeg.len());

        let ret = arceos_jpeg_decode(
            (*bs_ptr).as_mut_ptr(),
            (*yuv_ptr).as_mut_ptr(),
            jpeg.len() as i32,
        );
        if ret != 0 {
            return Err(DecodeError::DriverFailed);
        }

        let mut w: i32 = 0;
        let mut h: i32 = 0;
        get_jpeg_size(&mut w, &mut h);
        if w <= 0 || h <= 0 {
            return Err(DecodeError::InvalidSize);
        }

        let stride_y = ((w as u32 + 15) / 16) * 16;
        let stride_c = stride_y / 2;
        let aligned_h = ((h as u32 + 15) / 16) * 16;
        let yuv_len = (stride_y * aligned_h + stride_c * aligned_h) as usize;

        dcache_inv_range(yuv_phys, yuv_len.min(YUV_BUF_SIZE));

        Ok(DecodeResult {
            width: w as u32,
            height: h as u32,
            yuv_data: core::slice::from_raw_parts((*yuv_ptr).as_ptr(), yuv_len.min(YUV_BUF_SIZE)),
            yuv_phys_addr: yuv_phys,
        })
    }
}
