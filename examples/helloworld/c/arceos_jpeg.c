#include <stdint.h>

#include "jpeg.h"
#include "regdefine.h"

#define mmio_write_32(a, v) writel(v, a)
#define mmio_read_32(a) readl(a)

extern void writel(uint32_t val, volatile void *addr);
extern uint32_t readl(const volatile void *addr);

int arceos_jpeg_decode(void *bs_addr, void *yuv_addr, int size)
{
	mmio_write_32((void *)TOP_DDR_ADDR_MODE_REG,
		      (1 << DAMR_REG_VD_REMAP_ADDR_39_32_OFFSET));
	mmio_write_32((void *)VC_REG_BASE,
		      (mmio_read_32((void *)VC_REG_BASE) | 0x1f));
	return jpeg_decoder(bs_addr, yuv_addr, size);
}
