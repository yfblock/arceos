#ifndef __CONFIG_H__
#define __CONFIG_H__

#define PLATFORM_NON_OS

#define VPU_DELAY_MS(X) do { volatile int _jdi_i; for (_jdi_i = 0; _jdi_i < (int)(X) * 1000; _jdi_i++) { ; } } while (0)
#define VPU_DELAY_US(X) do { volatile int _jdi_i; for (_jdi_i = 0; _jdi_i < (int)(X); _jdi_i++) { ; } } while (0)

#define API_VERSION 165
#define HAVE_STDIN_H 1

#endif
