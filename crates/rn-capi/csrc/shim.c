/*
** C shim for the C-variadic entry points of the Lua C API. Stable Rust cannot define C-variadic
** functions (nor consume a `va_list`), so the argument extraction (`va_arg`) lives here; all
** logic is in Rust (`src/fmt.rs`, `src/lapi_call.rs`).
*/
#include <stdarg.h>
#include <stddef.h>
#include <string.h>

typedef struct lua_State lua_State;
typedef long long lua_Integer;
typedef double lua_Number;

/* implemented in Rust */
extern void *rn_fmt_new(void);
extern void rn_fmt_bytes(void *b, const char *s, size_t n);
extern void rn_fmt_cstr(void *b, const char *s);
extern void rn_fmt_char(void *b, int c);
extern void rn_fmt_int(void *b, lua_Integer i);
extern void rn_fmt_num(void *b, lua_Number n);
extern void rn_fmt_ptr(void *b, void *p);
extern void rn_fmt_utf8(void *b, unsigned long x);
extern const char *rn_fmt_push(lua_State *L, void *b);
#if defined(RN_LUA54)
extern void rn_fmt_invalid(lua_State *L, void *b, int c) __attribute__((noreturn));
#endif
extern int rn_gc(lua_State *L, int what, size_t a_size, int a_param, int a_value);
extern void luaL_where(lua_State *L, int lvl);
extern void lua_concat(lua_State *L, int n);
extern int lua_error(lua_State *L) __attribute__((noreturn));

const char *lua_pushvfstring(lua_State *L, const char *fmt, va_list argp) {
  void *b = rn_fmt_new();
  const char *e = fmt;
  for (;;) {
    const char *p = strchr(e, '%');
    if (p == NULL) {
      rn_fmt_bytes(b, e, strlen(e));
      break;
    }
    rn_fmt_bytes(b, e, (size_t)(p - e));
    switch (p[1]) {
      case 's': rn_fmt_cstr(b, va_arg(argp, const char *)); break;
      case 'c': rn_fmt_char(b, va_arg(argp, int)); break;
      case 'd': rn_fmt_int(b, (lua_Integer)va_arg(argp, int)); break;
      case 'I': rn_fmt_int(b, va_arg(argp, lua_Integer)); break;
      case 'f': rn_fmt_num(b, va_arg(argp, lua_Number)); break;
      case 'p': rn_fmt_ptr(b, va_arg(argp, void *)); break;
      case 'U': rn_fmt_utf8(b, (unsigned long)va_arg(argp, long)); break;
      case '%': rn_fmt_bytes(b, "%", 1); break;
      default: /* unknown conversion: keep it in the result (5.5; 5.4 raises an error) */
#if defined(RN_LUA54)
        rn_fmt_invalid(L, b, p[1]);
#endif
        rn_fmt_bytes(b, p, p[1] ? 2 : 1);
        if (p[1] == '\0') return rn_fmt_push(L, b);
        break;
    }
    e = p + 2;
  }
  return rn_fmt_push(L, b);
}

const char *lua_pushfstring(lua_State *L, const char *fmt, ...) {
  const char *r;
  va_list argp;
  va_start(argp, fmt);
  r = lua_pushvfstring(L, fmt, argp);
  va_end(argp);
  return r;
}

int luaL_error(lua_State *L, const char *fmt, ...) {
  va_list argp;
  va_start(argp, fmt);
  luaL_where(L, 1);
  lua_pushvfstring(L, fmt, argp);
  va_end(argp);
  lua_concat(L, 2);
  return lua_error(L);
}

#define RN_GCSTEP 5
#define RN_GCPARAM 9

#if defined(RN_LUA54)
/* 5.4: STEP/SETPAUSE/SETSTEPMUL take one int, GEN two, INC three (in a_param, a_value, a_size) */
int lua_gc(lua_State *L, int what, ...) {
  va_list argp;
  size_t a_size = 0;
  int a_param = 0, a_value = 0;
  va_start(argp, what);
  switch (what) {
    case 5: case 6: case 7: a_param = va_arg(argp, int); break;
    case 10: a_param = va_arg(argp, int); a_value = va_arg(argp, int); break;
    case 11:
      a_param = va_arg(argp, int); a_value = va_arg(argp, int);
      a_size = (size_t)(unsigned)va_arg(argp, int);
      break;
    default: break;
  }
  va_end(argp);
  return rn_gc(L, what, a_size, a_param, a_value);
}
#else
int lua_gc(lua_State *L, int what, ...) {
  va_list argp;
  size_t a_size = 0;
  int a_param = 0, a_value = 0;
  va_start(argp, what);
  if (what == RN_GCSTEP) {
    a_size = va_arg(argp, size_t);
  } else if (what == RN_GCPARAM) {
    a_param = va_arg(argp, int);
    a_value = va_arg(argp, int);
  }
  va_end(argp);
  return rn_gc(L, what, a_size, a_param, a_value);
}
#endif
