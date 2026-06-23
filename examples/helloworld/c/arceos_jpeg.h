#ifndef _ARCEOS_JPEG_H_
#define _ARCEOS_JPEG_H_

/* Returns 0 on success, non-zero on failure (same as jpeg_decoder). */
int arceos_jpeg_decode(void *bs_addr, void *yuv_addr, int size);

#endif
