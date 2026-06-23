#ifndef _ARCEOS_ASM_CACHE_H_
#define _ARCEOS_ASM_CACHE_H_

void flush_dcache_range(unsigned long start, unsigned long end);
void invalidate_dcache_range(unsigned long start, unsigned long end);

#endif
