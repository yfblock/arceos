#ifndef _ARCEOS_ASM_IO_H_
#define _ARCEOS_ASM_IO_H_

#include <stdint.h>

void writel(uint32_t val, volatile void *addr);
uint32_t readl(const volatile void *addr);

#endif
