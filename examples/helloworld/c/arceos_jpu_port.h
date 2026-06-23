#ifndef _ARCEOS_JPU_PORT_H_
#define _ARCEOS_JPU_PORT_H_

#include <stddef.h>

/* Provide JDI video memory pool from Rust to avoid multi-MB C .bss. */
void arceos_jpu_set_pool(void *addr, size_t size);

#endif
