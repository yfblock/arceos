#ifndef _ARCEOS_ASSERT_H_
#define _ARCEOS_ASSERT_H_

void assert_fail(const char *expr, const char *file, int line, const char *func);

#undef assert
#define assert(x) ((void)((x) || (assert_fail(#x, __FILE__, __LINE__, __func__), 0)))

#endif
