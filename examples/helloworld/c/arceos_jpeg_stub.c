#include "arceos_jpeg.h"

int arceos_jpeg_decode(void *bs_addr, void *yuv_addr, int size)
{
	(void)bs_addr;
	(void)yuv_addr;
	(void)size;
	return 0;
}

int get_jpeg_size(int *width_addr, int *height_addr)
{
	*width_addr = 31;
	*height_addr = 240;
	return 0;
}
