/* ArceOS port layer for U-Boot CVitek JPEG driver */

#include <stddef.h>
#include <stdint.h>

#include "asm/cache.h"
#include "arceos_jpu_port.h"

#define CACHE_LINE 64
#define JPU_POOL_SIZE 0x00200000

static uint8_t *jpu_video_mem;
static size_t jpu_video_pool_size;
static int jpu_video_mem_used;

void arceos_jpu_set_pool(void *addr, size_t size)
{
	jpu_video_mem = (uint8_t *)addr;
	jpu_video_pool_size = size;
	jpu_video_mem_used = 0;
}

void *memcpy(void *dest, const void *src, size_t n)
{
	return __builtin_memcpy(dest, src, n);
}

void *memset(void *s, int c, size_t n)
{
	return __builtin_memset(s, c, n);
}

void *memmove(void *dest, const void *src, size_t n)
{
	return __builtin_memmove(dest, src, n);
}

int memcmp(const void *s1, const void *s2, size_t n)
{
	return __builtin_memcmp(s1, s2, n);
}

size_t strlen(const char *s)
{
	const char *p = s;

	while (*p)
		p++;
	return (size_t)(p - s);
}

void flush_dcache_range(unsigned long start, unsigned long end)
{
	unsigned long addr = start & ~(CACHE_LINE - 1);

	while (addr < end) {
		__asm__ volatile(".word 0x0295000b" : : "r"(addr));
		addr += CACHE_LINE;
	}
	__asm__ volatile(".word 0x0190000b");
}

void invalidate_dcache_range(unsigned long start, unsigned long end)
{
	unsigned long addr = start & ~(CACHE_LINE - 1);

	while (addr < end) {
		__asm__ volatile(".word 0x02a5000b" : : "r"(addr));
		addr += CACHE_LINE;
	}
	__asm__ volatile(".word 0x0190000b");
}

void writel(uint32_t val, volatile void *addr)
{
	*(volatile uint32_t *)addr = val;
	flush_dcache_range((unsigned long)addr, (unsigned long)addr + 4);
}

uint32_t readl(const volatile void *addr)
{
	invalidate_dcache_range((unsigned long)addr, (unsigned long)addr + 4);
	return *(const volatile uint32_t *)addr;
}

void *malloc(size_t size)
{
	if (jpu_video_mem_used || !jpu_video_mem)
		return NULL;
	if (size > jpu_video_pool_size)
		return NULL;
	jpu_video_mem_used = 1;
	return jpu_video_mem;
}

void free(void *ptr)
{
	if (jpu_video_mem && ptr == jpu_video_mem)
		jpu_video_mem_used = 0;
}

int printf(const char *fmt, ...)
{
	(void)fmt;
	return 0;
}

int usleep(unsigned int usec)
{
	volatile unsigned int i;

	for (i = 0; i < usec * 10; i++)
		;
	return 0;
}

void assert_fail(const char *expr, const char *file, int line, const char *func)
{
	(void)expr;
	(void)file;
	(void)line;
	(void)func;
	for (;;)
		;
}

void assert(int condition)
{
	if (!condition)
		assert_fail("0", "unknown", 0, "assert");
}
