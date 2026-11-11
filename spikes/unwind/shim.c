#include <stdarg.h>
#include <stdio.h>
void *spike_push_str(void *L, const char *s);
void spike_raise_with_msg(void *L, const char *s) __attribute__((noreturn));
/* analogue of lua_pushfstring / luaL_error: the only C-variadic entry points */
const char *spike_pushfstring(void *L, const char *fmt, ...) {
  char buf[256]; va_list ap; va_start(ap, fmt); vsnprintf(buf, sizeof buf, fmt, ap); va_end(ap);
  return spike_push_str(L, buf);
}
int spike_error_f(void *L, const char *fmt, ...) {
  char buf[256]; va_list ap; va_start(ap, fmt); vsnprintf(buf, sizeof buf, fmt, ap); va_end(ap);
  spike_raise_with_msg(L, buf);
}
