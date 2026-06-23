//! JPU DMA 物理内存分配器。
//!
//! JPU 通过 AXI 直接访问物理地址，buffer 必须位于连续物理 RAM 且做好 cache 维护。
//! 本模块在静态数组 `JPU_DMA_BUFFER` 上实现简单的页位图分配（16 KiB 页）。

use super::regs::{JPU_DRAM_PHYSICAL_SIZE, VMEM_PAGE_SIZE};

/// 位图页分配器
struct JpuMemoryPool {
    base_addr: usize,
    size: usize,
    num_pages: usize,
    /// 1 = 空闲，0 = 已分配
    bitmap: [u64; 32],
}

impl JpuMemoryPool {
    const fn new() -> Self {
        Self {
            base_addr: 0,
            size: 0,
            num_pages: 0,
            bitmap: [0; 32],
        }
    }

    fn init(&mut self, base: usize, size: usize) {
        self.base_addr = (base + VMEM_PAGE_SIZE - 1) & !(VMEM_PAGE_SIZE - 1);
        self.size = size & !(VMEM_PAGE_SIZE - 1);
        self.num_pages = self.size / VMEM_PAGE_SIZE;
        for word in &mut self.bitmap {
            *word = u64::MAX;
        }
    }

    fn alloc(&mut self, size: usize) -> Option<usize> {
        let npages = size.div_ceil(VMEM_PAGE_SIZE);
        let mut consecutive = 0usize;
        let mut start_page = 0usize;

        for page_idx in 0..self.num_pages {
            let word_idx = page_idx / 64;
            let bit_idx = page_idx % 64;
            if word_idx >= self.bitmap.len() {
                break;
            }

            if self.bitmap[word_idx] & (1 << bit_idx) != 0 {
                if consecutive == 0 {
                    start_page = page_idx;
                }
                consecutive += 1;
                if consecutive >= npages {
                    for i in 0..npages {
                        let p = start_page + i;
                        self.bitmap[p / 64] &= !(1 << (p % 64));
                    }
                    return Some(self.base_addr + start_page * VMEM_PAGE_SIZE);
                }
            } else {
                consecutive = 0;
            }
        }
        None
    }

    fn free(&mut self, addr: usize, size: usize) {
        if addr < self.base_addr || addr >= self.base_addr + self.size {
            return;
        }
        let start_page = (addr - self.base_addr) / VMEM_PAGE_SIZE;
        let npages = size.div_ceil(VMEM_PAGE_SIZE);
        for i in 0..npages {
            let p = start_page + i;
            if p >= self.num_pages {
                break;
            }
            self.bitmap[p / 64] |= 1 << (p % 64);
        }
    }
}

static mut JPU_MEM_POOL: *mut JpuMemoryPool = core::ptr::null_mut();
static mut POOL_INSTANCE: JpuMemoryPool = JpuMemoryPool::new();

#[repr(C, align(4096))]
struct AlignedMem<const N: usize>([u8; N]);

static mut JPU_DMA_BUFFER: AlignedMem<{ JPU_DRAM_PHYSICAL_SIZE }> = AlignedMem([0u8; JPU_DRAM_PHYSICAL_SIZE]);

/// 初始化 DMA 内存池（在 `JpuDecoder::init` 最开始调用）
pub fn init_jpu_memory() {
    unsafe {
        JPU_MEM_POOL = core::ptr::addr_of_mut!(POOL_INSTANCE);
        let buf_addr = core::ptr::addr_of!(JPU_DMA_BUFFER) as *const u8 as usize;
        (*JPU_MEM_POOL).init(buf_addr, JPU_DRAM_PHYSICAL_SIZE);
    }
}

pub fn jpu_alloc(size: usize) -> Option<usize> {
    unsafe {
        if JPU_MEM_POOL.is_null() {
            None
        } else {
            (*JPU_MEM_POOL).alloc(size)
        }
    }
}

pub fn jpu_free(addr: usize, size: usize) {
    unsafe {
        if !JPU_MEM_POOL.is_null() {
            (*JPU_MEM_POOL).free(addr, size);
        }
    }
}
