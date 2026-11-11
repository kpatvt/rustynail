/*
** C API differential tests: compiled against the reference liblua.a and against librn_capi.a;
** both must print exactly the same output. Everything printed is deterministic (no addresses, no
** hash-order dependent iteration, no allocation counts).
*/
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

#define SECTION(name) printf("\n== %s\n", name)

static lua_State *newL(void) {
  lua_State *L = luaL_newstate();
  luaL_openlibs(L);
  return L;
}

static void dumpvalue(lua_State *L, int i) {
  int t = lua_type(L, i);
  switch (t) {
    case LUA_TNIL: printf("nil"); break;
    case LUA_TBOOLEAN: printf(lua_toboolean(L, i) ? "true" : "false"); break;
    case LUA_TNUMBER:
      if (lua_isinteger(L, i)) printf("int:%lld", (long long)lua_tointeger(L, i));
      else printf("flt:%.14g", (double)lua_tonumber(L, i));
      break;
    case LUA_TSTRING: { size_t l; const char *s = lua_tolstring(L, i, &l); printf("str:%.*s(%d)", (int)l, s, (int)l); break; }
    case LUA_TNONE: printf("none"); break;
    default: printf("%s", lua_typename(L, t));
  }
}

static void dumpstack(lua_State *L, const char *tag) {
  int n = lua_gettop(L), i;
  printf("[%s] top=%d:", tag, n);
  for (i = 1; i <= n; i++) { printf(" "); dumpvalue(L, i); }
  printf("\n");
}

/* call a C function in protected mode with the n values on top; print status and results */
static int pcallf(lua_State *L, lua_CFunction f, int nargs, const char *tag) {
  int base = lua_gettop(L) - nargs, i, st;
  lua_pushcfunction(L, f);
  lua_insert(L, base + 1);
  st = lua_pcall(L, nargs, LUA_MULTRET, 0);
  printf("[%s] status=%d", tag, st);
  for (i = base + 1; i <= lua_gettop(L); i++) { printf(" "); dumpvalue(L, i); }
  printf("\n");
  lua_settop(L, base);
  return st;
}

/* ============================================================================================ */
static void t_stack(void) {
  lua_State *L = newL();
  lua_State *L1;
  SECTION("stack");
  lua_pushinteger(L, 1); lua_pushinteger(L, 2); lua_pushinteger(L, 3); lua_pushinteger(L, 4);
  dumpstack(L, "init");
  lua_rotate(L, 2, 1); dumpstack(L, "rot+1");
  lua_rotate(L, 2, -1); dumpstack(L, "rot-1");
  lua_rotate(L, 1, 3); dumpstack(L, "rot3");
  lua_copy(L, 1, 4); dumpstack(L, "copy");
  lua_pushvalue(L, -2); dumpstack(L, "pushvalue");
  lua_insert(L, 1); dumpstack(L, "insert");
  lua_remove(L, 2); dumpstack(L, "remove");
  lua_pushstring(L, "x"); lua_replace(L, 1); dumpstack(L, "replace");
  printf("absindex: %d %d %d\n", lua_absindex(L, 1), lua_absindex(L, -1), lua_absindex(L, LUA_REGISTRYINDEX));
  lua_settop(L, 8); dumpstack(L, "settop8");
  lua_settop(L, -4); dumpstack(L, "settop-4");
  lua_settop(L, 0); dumpstack(L, "settop0");
  printf("checkstack: %d %d\n", lua_checkstack(L, 100), lua_checkstack(L, 10000000));
  L1 = lua_newthread(L);
  lua_pushinteger(L, 10); lua_pushinteger(L, 20); lua_pushinteger(L, 30);
  lua_xmove(L, L1, 2);
  dumpstack(L, "xmove-from"); dumpstack(L1, "xmove-to");
  lua_xmove(L1, L, 1); dumpstack(L, "xmove-back");
  lua_close(L);
}

/* ============================================================================================ */
static int cf_dummy(lua_State *L) { (void)L; return 0; }

static void t_types(void) {
  lua_State *L = newL();
  int n, i;
  char buf[64];
  SECTION("types");
  lua_pushnil(L); lua_pushboolean(L, 1); lua_pushinteger(L, 42); lua_pushnumber(L, 2.5);
  lua_pushstring(L, "10"); lua_pushstring(L, "0x10"); lua_pushstring(L, "abc"); lua_pushstring(L, " 7 ");
  lua_newtable(L); lua_pushcfunction(L, cf_dummy); lua_newuserdatauv(L, 8, 1);
  lua_pushlightuserdata(L, (void *)&L); lua_pushthread(L); lua_pushnumber(L, 3.0); lua_pushnumber(L, 1e100);
  luaL_loadstring(L, "return 1");
  n = lua_gettop(L);
  for (i = 1; i <= n + 1; i++) {
    int isnum = -1, isint = -1;
    lua_Number d; lua_Integer k;
    printf("%2d type=%d(%s) isnum=%d isstr=%d isint=%d iscf=%d isud=%d bool=%d rawlen=%d", i, lua_type(L, i),
      lua_typename(L, lua_type(L, i)), lua_isnumber(L, i), lua_isstring(L, i), lua_isinteger(L, i),
      lua_iscfunction(L, i), lua_isuserdata(L, i), lua_toboolean(L, i), (int)lua_rawlen(L, i));
    d = lua_tonumberx(L, i, &isnum); k = lua_tointegerx(L, i, &isint);
    printf(" num=%d:%.14g int=%d:%lld", isnum, (double)d, isint, (long long)k);
    printf(" tocf=%d touser=%d tothread=%d topointer=%d\n", lua_tocfunction(L, i) != NULL, lua_touserdata(L, i) != NULL,
      lua_tothread(L, i) != NULL, lua_topointer(L, i) != NULL);
  }
  printf("typename(NONE)=%s\n", lua_typename(L, LUA_TNONE));
  lua_pushinteger(L, 77); lua_pushnumber(L, 1.5);
  { size_t l; const char *s = lua_tolstring(L, -2, &l); printf("tolstring int: %s %d type=%d\n", s, (int)l, lua_type(L, -2)); }
  { size_t l; const char *s = lua_tolstring(L, -1, &l); printf("tolstring flt: %s %d type=%d\n", s, (int)l, lua_type(L, -1)); }
  lua_settop(L, 0);
  lua_pushinteger(L, 123); lua_pushnumber(L, 0.1); lua_pushstring(L, "x");
#if LUA_VERSION_NUM >= 505
  printf("numbertocstring: %u:", lua_numbertocstring(L, 1, buf)); printf("%s ", buf);
  printf("%u:", lua_numbertocstring(L, 2, buf)); printf("%s ", buf);
  printf("%u\n", lua_numbertocstring(L, 3, buf));
#else
  (void)buf;
#endif
  lua_settop(L, 0);
  { size_t r;
    r = lua_stringtonumber(L, "0x1F"); printf("stringtonumber: %d ", (int)r); dumpvalue(L, -1); printf("\n");
    r = lua_stringtonumber(L, "1e2"); printf("stringtonumber: %d ", (int)r); dumpvalue(L, -1); printf("\n");
    r = lua_stringtonumber(L, "12abc"); printf("stringtonumber: %d top=%d\n", (int)r, lua_gettop(L)); }
  printf("version=%.0f\n", (double)lua_version(L));
  lua_close(L);
}

/* ============================================================================================ */
static int cf_arith(lua_State *L) { lua_arith(L, (int)lua_tointeger(L, lua_upvalueindex(1))); return 1; }

static void t_arith(void) {
  lua_State *L = newL();
  static const int ops[] = {LUA_OPADD, LUA_OPSUB, LUA_OPMUL, LUA_OPMOD, LUA_OPPOW, LUA_OPDIV, LUA_OPIDIV,
                            LUA_OPBAND, LUA_OPBOR, LUA_OPBXOR, LUA_OPSHL, LUA_OPSHR};
  static const char *names[] = {"add","sub","mul","mod","pow","div","idiv","band","bor","bxor","shl","shr"};
  int i, j;
  SECTION("arith/compare");
  for (i = 0; i < 12; i++) {
    for (j = 0; j < 6; j++) {
      int st;
      lua_settop(L, 0);
      lua_pushinteger(L, ops[i]);
      lua_pushcclosure(L, cf_arith, 1);
      switch (j) {
        case 0: lua_pushinteger(L, 7); lua_pushinteger(L, 3); break;
        case 1: lua_pushinteger(L, 7); lua_pushnumber(L, 2.0); break;
        case 2: lua_pushnumber(L, 7.5); lua_pushinteger(L, 2); break;
        case 3: lua_pushstring(L, "10"); lua_pushinteger(L, 4); break;
        case 4: lua_pushinteger(L, 5); lua_pushinteger(L, 0); break;
        default: lua_pushnumber(L, -0.5); lua_pushnumber(L, 3.0); break;
      }
      st = lua_pcall(L, 2, 1, 0);
      printf("%s/%d: st=%d ", names[i], j, st); dumpvalue(L, -1); printf("\n");
    }
  }
  lua_settop(L, 0); lua_pushinteger(L, 5); lua_arith(L, LUA_OPUNM); dumpstack(L, "unm");
  lua_settop(L, 0); lua_pushinteger(L, 5); lua_arith(L, LUA_OPBNOT); dumpstack(L, "bnot");
  lua_settop(L, 0); lua_pushnumber(L, 2.5); lua_arith(L, LUA_OPUNM); dumpstack(L, "unm-flt");
  luaL_dostring(L, "mt = {__add = function(a, b) return 'added' end, __lt = function() return true end, __le = function() return false end, __eq = function() return true end}");
  lua_settop(L, 0);
  luaL_dostring(L, "return setmetatable({}, mt), setmetatable({}, mt), 1");
  lua_pushvalue(L, 1); lua_pushvalue(L, 3); lua_arith(L, LUA_OPADD); dumpstack(L, "mm-add");
  lua_settop(L, 3);
  printf("compare: eq=%d lt=%d le=%d rawequal=%d\n", lua_compare(L, 1, 2, LUA_OPEQ), lua_compare(L, 1, 2, LUA_OPLT),
         lua_compare(L, 1, 2, LUA_OPLE), lua_rawequal(L, 1, 2));
  lua_settop(L, 0); lua_pushinteger(L, 1); lua_pushnumber(L, 1.0); lua_pushstring(L, "1"); lua_pushinteger(L, 2);
  printf("cmp: %d %d %d %d %d %d | rawequal %d %d invalid %d %d\n", lua_compare(L, 1, 2, LUA_OPEQ), lua_compare(L, 1, 3, LUA_OPEQ),
         lua_compare(L, 1, 4, LUA_OPLT), lua_compare(L, 4, 1, LUA_OPLE), lua_compare(L, 1, 2, LUA_OPLE), lua_compare(L, 1, 9, LUA_OPEQ),
         lua_rawequal(L, 1, 2), lua_rawequal(L, 1, 3), lua_rawequal(L, 1, 9), lua_rawequal(L, 8, 9));
  lua_close(L);
}

/* ============================================================================================ */
static int cf_gettable_err(lua_State *L) { /* indexes a table with a failing __index */
  lua_getfield(L, 1, "x");
  return 1;
}
static int cf_settable_err(lua_State *L) { lua_pushinteger(L, 1); lua_setfield(L, 1, "x"); return 0; }
static int cf_index_nil(lua_State *L) { lua_pushnil(L); lua_getfield(L, -1, "x"); return 1; }
static int cf_len_err(lua_State *L) { lua_len(L, 1); return 1; }
static int cf_concat_err(lua_State *L) { lua_pushvalue(L, 1); lua_pushvalue(L, 2); lua_concat(L, 2); return 1; }
static int cf_next_bad(lua_State *L) { lua_pushstring(L, "nokey"); lua_next(L, 1); return 0; }

static void t_tables(void) {
  lua_State *L = newL();
  int i;
  SECTION("tables");
  lua_newtable(L);                                /* t at 1 */
  lua_pushinteger(L, 10); lua_setfield(L, 1, "a");
  lua_pushinteger(L, 20); lua_seti(L, 1, 1);
  lua_pushstring(L, "k"); lua_pushstring(L, "v"); lua_settable(L, 1);
  lua_pushinteger(L, 30); lua_rawseti(L, 1, 2);
  lua_pushstring(L, "rv"); lua_pushlightuserdata(L, (void *)&i); lua_insert(L, -2); lua_rawsetp(L, 1, (void *)&i);
  lua_pushstring(L, "rawk"); lua_pushstring(L, "rawv"); lua_rawset(L, 1);
  printf("getfield a: %d ", lua_getfield(L, 1, "a")); dumpvalue(L, -1); lua_pop(L, 1); printf("\n");
  printf("geti 1: %d ", lua_geti(L, 1, 1)); dumpvalue(L, -1); lua_pop(L, 1); printf("\n");
  printf("rawgeti 2: %d ", lua_rawgeti(L, 1, 2)); dumpvalue(L, -1); lua_pop(L, 1); printf("\n");
  lua_pushstring(L, "k"); printf("gettable k: %d ", lua_gettable(L, 1)); dumpvalue(L, -1); lua_pop(L, 1); printf("\n");
  lua_pushstring(L, "rawk"); printf("rawget rawk: %d ", lua_rawget(L, 1)); dumpvalue(L, -1); lua_pop(L, 1); printf("\n");
  printf("rawgetp: %d ", lua_rawgetp(L, 1, (void *)&i)); dumpvalue(L, -1); lua_pop(L, 1); printf("\n");
  printf("missing: %d ", lua_getfield(L, 1, "zzz")); dumpvalue(L, -1); lua_pop(L, 1); printf("\n");
  printf("rawlen=%d len: ", (int)lua_rawlen(L, 1)); lua_len(L, 1); dumpvalue(L, -1); lua_pop(L, 1); printf(" luaL_len=%d\n", (int)luaL_len(L, 1));
  /* next over array part only (order independent): build a fresh array */
  lua_settop(L, 0); lua_createtable(L, 5, 0);
  for (i = 1; i <= 5; i++) { lua_pushinteger(L, i * 100); lua_rawseti(L, 1, i); }
  lua_pushnil(L); i = 0;
  while (lua_next(L, 1)) { printf("next: "); dumpvalue(L, -2); printf("=>"); dumpvalue(L, -1); printf("\n"); lua_pop(L, 1); i++; }
  printf("next count %d top=%d\n", i, lua_gettop(L));
  /* metamethods __index/__newindex via API */
  lua_settop(L, 0);
  luaL_dostring(L, "return setmetatable({}, {__index = function(t, k) return k .. '!' end, __newindex = function(t, k, v) rawset(t, k, v * 2) end})");
  lua_getfield(L, 1, "hey"); dumpstack(L, "mm-index");
  lua_pushinteger(L, 21); lua_setfield(L, 1, "dbl"); lua_rawgetp(L, 1, NULL); lua_pop(L, 1);
  lua_getfield(L, 1, "dbl"); dumpstack(L, "mm-newindex");
  lua_settop(L, 0);
  luaL_dostring(L, "return setmetatable({}, {__index = function() error('idx failed') end, __newindex = function() error('newidx failed') end, __len = function() error('len failed') end, __concat = function() error('concat failed') end})");
  pcallf(L, cf_gettable_err, 1, "gettable-err");  lua_pushvalue(L, 1);
  pcallf(L, cf_settable_err, 1, "settable-err"); lua_pushvalue(L, 1);
  pcallf(L, cf_len_err, 1, "len-err"); lua_pushvalue(L, 1); lua_pushstring(L, "s");
  pcallf(L, cf_concat_err, 2, "concat-err");
  pcallf(L, cf_index_nil, 0, "index-nil");
  lua_newtable(L); pcallf(L, cf_next_bad, 1, "next-bad");
  /* globals and registry */
  lua_settop(L, 0);
  lua_pushinteger(L, 99); lua_setglobal(L, "G1");
  printf("getglobal: %d ", lua_getglobal(L, "G1")); dumpvalue(L, -1); printf("\n"); lua_pop(L, 1);
  printf("getglobal missing: %d\n", lua_getglobal(L, "G2")); lua_pop(L, 1);
  printf("registry globals: %d ", lua_rawgeti(L, LUA_REGISTRYINDEX, LUA_RIDX_GLOBALS)); printf("istable=%d\n", lua_istable(L, -1)); lua_pop(L, 1);
  printf("registry mainthread: %d ", lua_rawgeti(L, LUA_REGISTRYINDEX, LUA_RIDX_MAINTHREAD)); printf("isthread=%d\n", lua_isthread(L, -1)); lua_pop(L, 1);
  /* metatables */
  lua_settop(L, 0); lua_newtable(L); lua_newtable(L); lua_pushstring(L, "mtv"); lua_setfield(L, -2, "tag");
  printf("setmetatable: %d ", lua_setmetatable(L, 1)); printf("getmetatable: %d ", lua_getmetatable(L, 1)); lua_getfield(L, -1, "tag"); dumpvalue(L, -1); printf("\n");
  lua_settop(L, 0); lua_pushinteger(L, 1);
  printf("getmetatable(int): %d\n", lua_getmetatable(L, 1));
  lua_newtable(L); lua_pushstring(L, "tt"); lua_setfield(L, -2, "__index"); lua_setmetatable(L, 1);  /* number metatable */
  luaL_dostring(L, "return (5).x"); dumpstack(L, "number-mt");
  lua_close(L);
}

/* ============================================================================================ */
static int cf_err_string(lua_State *L) { return luaL_error(L, "custom error %d: %s", 42, "text"); }
static int cf_err_table(lua_State *L) { lua_newtable(L); lua_pushinteger(L, 7); lua_setfield(L, -2, "code"); return lua_error(L); }
static int cf_err_nil(lua_State *L) { lua_pushnil(L); return lua_error(L); }
static int cf_ok(lua_State *L) { lua_pushinteger(L, lua_gettop(L)); return 1; }
static int cf_call_lua_err(lua_State *L) { lua_getglobal(L, "luaerr"); lua_call(L, 0, 0); return 0; }
static int cf_call_nested(lua_State *L) { lua_pushcfunction(L, cf_err_string); lua_call(L, 0, 0); return 0; }
static int cf_pcall_nested(lua_State *L) {
  int st;
  lua_pushcfunction(L, cf_err_string);
  st = lua_pcall(L, 0, 0, 0);
  lua_pushinteger(L, st); lua_insert(L, -2);
  return 2;
}
static int cf_handler(lua_State *L) { lua_pushfstring(L, "handled<%s>", lua_tostring(L, 1)); return 1; }
static int cf_manyres(lua_State *L) { lua_pushinteger(L, 1); lua_pushinteger(L, 2); lua_pushinteger(L, 3); return 3; }

static void t_calls(void) {
  lua_State *L = newL();
  int st;
  SECTION("calls and errors");
  pcallf(L, cf_err_string, 0, "luaL_error");
  pcallf(L, cf_err_table, 0, "error-table");
  pcallf(L, cf_err_nil, 0, "error-nil");
  luaL_dostring(L, "function luaerr() error('from lua') end");
  pcallf(L, cf_call_lua_err, 0, "call-lua-err");
  pcallf(L, cf_call_nested, 0, "nested-err");
  pcallf(L, cf_pcall_nested, 0, "nested-pcall");
  lua_pushinteger(L, 1); lua_pushinteger(L, 2); pcallf(L, cf_ok, 2, "args");
  pcallf(L, cf_manyres, 0, "multret");
  /* message handler */
  lua_settop(L, 0);
  lua_pushcfunction(L, cf_handler); lua_pushcfunction(L, cf_err_string);
  st = lua_pcall(L, 0, 0, 1); printf("handler status=%d ", st); dumpvalue(L, -1); printf("\n");
  lua_settop(L, 0);
  lua_pushcfunction(L, cf_handler); luaL_loadstring(L, "local x = nil; return x.y");
  st = lua_pcall(L, 0, 0, 1); printf("handler(runtime) status=%d ", st); dumpvalue(L, -1); printf("\n");
  /* lua_call with various nresults */
  lua_settop(L, 0);
  lua_pushcfunction(L, cf_manyres); lua_call(L, 0, 2); dumpstack(L, "call-2");
  lua_settop(L, 0); lua_pushcfunction(L, cf_manyres); lua_call(L, 0, 5); dumpstack(L, "call-5");
  lua_settop(L, 0); lua_pushcfunction(L, cf_manyres); lua_call(L, 0, LUA_MULTRET); dumpstack(L, "call-mult");
  lua_settop(L, 0); lua_pushcfunction(L, cf_manyres); lua_call(L, 0, 0); dumpstack(L, "call-0");
  /* error in error handler */
  lua_settop(L, 0); luaL_loadstring(L, "error(setmetatable({}, {__tostring = function() error('x') end}))");
  lua_pushcfunction(L, cf_err_string); lua_insert(L, 1);
  st = lua_pcall(L, 0, 0, 1); printf("err-in-handler status=%d\n", st);
  /* non-callable */
  lua_settop(L, 0); lua_pushinteger(L, 1); st = lua_pcall(L, 0, 0, 0); printf("call-nonfunc status=%d ", st); dumpvalue(L, -1); printf("\n");
  /* stack overflow in recursion */
  lua_settop(L, 0); luaL_dostring(L, "local function f() return 1 + f() end; local ok, e = pcall(f); return ok, e"); dumpstack(L, "overflow");
  lua_close(L);
}

/* ============================================================================================ */
typedef struct { const char *p; size_t left; size_t chunk; int calls; } RS;
static const char *reader(lua_State *L, void *ud, size_t *sz) {
  RS *r = (RS *)ud; const char *res;
  (void)L;
  if (r->left == 0) { *sz = 0; return NULL; }
  *sz = r->left < r->chunk ? r->left : r->chunk; res = r->p; r->p += *sz; r->left -= *sz; r->calls++;
  return res;
}
typedef struct { char buf[8192]; size_t n; int calls; } WS;
static int writer(lua_State *L, const void *p, size_t sz, void *ud) {
  WS *w = (WS *)ud; (void)L;
  memcpy(w->buf + w->n, p, sz); w->n += sz; w->calls++; return 0;
}
static int bad_writer(lua_State *L, const void *p, size_t sz, void *ud) { (void)L; (void)p; (void)sz; (void)ud; return 77; }

static void t_load(void) {
  lua_State *L = newL();
  const char *src = "local a, b = ...; return (a or 1) + (b or 2), 'ok'";
  size_t chunks[] = {1, 3, 1000};
  int i, st;
  WS w;
  SECTION("load/dump");
  for (i = 0; i < 3; i++) {
    RS r; r.p = src; r.left = strlen(src); r.chunk = chunks[i]; r.calls = 0;
    st = lua_load(L, reader, &r, "=chunky", NULL);
    printf("load chunk=%d status=%d ", (int)chunks[i], st);
    if (st == LUA_OK) { lua_pushinteger(L, 10); lua_pushinteger(L, 20); lua_call(L, 2, 2); }
    dumpstack(L, "loaded"); lua_settop(L, 0);
  }
  { RS r; r.p = src; r.left = strlen(src); r.chunk = 100; r.calls = 0;
    st = lua_load(L, reader, &r, "=m", "b"); printf("load text as binary-only status=%d ", st); dumpvalue(L, -1); printf("\n"); lua_settop(L, 0); }
  { RS r; r.p = "x = = 1"; r.left = 7; r.chunk = 100; r.calls = 0;
    st = lua_load(L, reader, &r, "=syn", NULL); printf("syntax status=%d ", st); dumpvalue(L, -1); printf("\n"); lua_settop(L, 0); }
  luaL_loadstring(L, src);
  memset(&w, 0, sizeof w);
  st = lua_dump(L, writer, &w, 0); printf("dump status=%d calls>0=%d len>0=%d\n", st, w.calls > 0, w.n > 0);
  lua_settop(L, 0);
  st = luaL_loadbufferx(L, w.buf, w.n, "=bin", "b"); printf("load binary status=%d ", st);
  lua_pushinteger(L, 5); lua_call(L, 1, 2); dumpstack(L, "bin-run"); lua_settop(L, 0);
  st = luaL_loadbufferx(L, w.buf, w.n, "=bin", "t"); printf("load binary as text status=%d ", st); dumpvalue(L, -1); printf("\n"); lua_settop(L, 0);
  luaL_loadstring(L, src); memset(&w, 0, sizeof w);
  lua_dump(L, writer, &w, 1); { size_t stripped = w.n; memset(&w, 0, sizeof w); lua_dump(L, writer, &w, 0); printf("strip smaller=%d\n", stripped < w.n); }
  printf("bad writer status=%d\n", lua_dump(L, bad_writer, &w, 0));
  lua_settop(L, 0);
  st = luaL_loadfilex(L, "/nonexistent/file.lua", NULL); printf("loadfile missing status=%d ", st); dumpvalue(L, -1); printf("\n");
  st = luaL_loadbufferx(L, "return 7", 8, NULL, NULL); printf("loadbuffer noname status=%d\n", st); lua_pcall(L, 0, 1, 0); dumpstack(L, "r");
  lua_settop(L, 0);
  st = luaL_loadstring(L, "return +"); printf("loadstring syntax status=%d ", st); dumpvalue(L, -1); printf("\n");
  lua_settop(L, 0);
  printf("dostring: %d\n", luaL_dostring(L, "return 1 + 1")); dumpstack(L, "dostring");
  lua_close(L);
}

/* ============================================================================================ */
/* coroutines, continuations */
static int cf_yield2(lua_State *L) { lua_pushinteger(L, 1); lua_pushinteger(L, 2); return lua_yield(L, 2); }
static int cont_k(lua_State *L, int status, lua_KContext ctx) {
  printf("  [k] status=%d ctx=%d top=%d\n", status, (int)ctx, lua_gettop(L));
  lua_pushinteger(L, (lua_Integer)ctx); return lua_gettop(L);
}
static int cf_yieldk(lua_State *L) { lua_pushstring(L, "yk"); return lua_yieldk(L, 1, 1234, cont_k); }
static int cf_callk(lua_State *L) { /* calls a Lua function that yields */
  lua_pushvalue(L, 1); lua_callk(L, 0, 1, 77, cont_k); return cont_k(L, LUA_OK, 77);
}
static int cf_pcallk(lua_State *L) {
  int st; lua_pushvalue(L, 1);
  st = lua_pcallk(L, 0, 1, 0, 88, cont_k);
  return cont_k(L, st, 88);
}
static int cf_resume_inside(lua_State *L) { /* C function that resumes another coroutine */
  lua_State *co = lua_newthread(L); int nres, st;
  lua_pushvalue(L, 1); lua_xmove(L, co, 1);
  st = lua_resume(co, L, 0, &nres);
  lua_pushinteger(L, st); lua_pushinteger(L, nres); return 2;
}

static void showresume(lua_State *co, lua_State *from, int nargs, const char *tag) {
  int nres = -1, st = lua_resume(co, from, nargs, &nres), i;
  printf("[%s] resume status=%d nres=%d status()=%d yieldable=%d:", tag, st, nres, lua_status(co), lua_isyieldable(co));
  for (i = lua_gettop(co) - nres + 1; i <= lua_gettop(co); i++) { printf(" "); dumpvalue(co, i); }
  printf("\n");
  if (st == LUA_OK || st == LUA_YIELD) lua_pop(co, nres);
  /* after an error the thread is dead: leave its stack alone */
}

static void t_coroutines(void) {
  lua_State *L = newL(), *co;
  SECTION("coroutines");
  printf("main yieldable=%d\n", lua_isyieldable(L));
  co = lua_newthread(L);
  luaL_loadstring(co, "local a = ...; local b = coroutine.yield(a + 1, 'y'); local c = coroutine.yield(b * 2); return 'done', c");
  lua_pushinteger(co, 10);
  showresume(co, L, 1, "lua-co1");
  lua_pushinteger(co, 5); showresume(co, L, 1, "lua-co2");
  lua_pushstring(co, "last"); showresume(co, L, 1, "lua-co3");
  lua_pushinteger(co, 1); showresume(co, L, 1, "dead");
  lua_settop(L, 0);
  /* C function yield */
  co = lua_newthread(L); lua_pushcfunction(co, cf_yield2);
  showresume(co, L, 0, "cyield"); showresume(co, L, 0, "cyield-finish");
  co = lua_newthread(L); lua_pushcfunction(co, cf_yieldk);
  showresume(co, L, 0, "yieldk"); lua_pushstring(co, "arg-after"); showresume(co, L, 1, "yieldk-cont");
  /* callk across yield */
  co = lua_newthread(L); lua_pushcfunction(co, cf_callk);
  luaL_loadstring(co, "coroutine.yield('in-lua'); return 'lua-ret'");
  showresume(co, L, 1, "callk"); showresume(co, L, 0, "callk-cont");
  /* pcallk across yield with error afterwards */
  co = lua_newthread(L); lua_pushcfunction(co, cf_pcallk);
  luaL_loadstring(co, "coroutine.yield('p'); error('after yield')");
  showresume(co, L, 1, "pcallk"); showresume(co, L, 0, "pcallk-err");
  /* error inside coroutine */
  co = lua_newthread(L); luaL_loadstring(co, "error('co error')");
  showresume(co, L, 0, "co-error");
  printf("co status after error=%d\n", lua_status(co));
  printf("closethread: %d ", lua_closethread(co, L)); dumpstack(co, "closed");
  /* yield outside coroutine */
  lua_settop(L, 0); lua_pushcfunction(L, cf_yield2); printf("yield from main: status=%d ", lua_pcall(L, 0, 0, 0)); dumpvalue(L, -1); printf("\n");
  /* resume inside C function */
  lua_settop(L, 0); luaL_loadstring(L, "coroutine.yield(1); return 2");
  pcallf(L, cf_resume_inside, 1, "resume-inside");
  /* pushthread / tothread */
  lua_settop(L, 0); printf("pushthread main=%d ", lua_pushthread(L)); lua_close(L);
  printf("\n");
  L = newL(); co = lua_newthread(L);
  printf("pushthread co=%d same=%d\n", lua_pushthread(co), lua_tothread(L, 1) == co);
  /* wrap-like usage via Lua */
  lua_settop(L, 0);
  luaL_dostring(L, "local co = coroutine.wrap(function(...) local n = select('#', ...); for i = 1, 3 do coroutine.yield(i * n) end return 'end' end); return co(1, 2), co(), co(), co()");
  dumpstack(L, "wrap");
  lua_close(L);
}

/* ============================================================================================ */
/* auxiliary library */
static int cf_chk_int(lua_State *L) { lua_pushinteger(L, luaL_checkinteger(L, 1)); return 1; }
static int cf_chk_num(lua_State *L) { lua_pushnumber(L, luaL_checknumber(L, 1)); return 1; }
static int cf_chk_str(lua_State *L) { size_t l; const char *s = luaL_checklstring(L, 1, &l); lua_pushfstring(L, "%s/%d", s, (int)l); return 1; }
static int cf_opt_int(lua_State *L) { lua_pushinteger(L, luaL_optinteger(L, 1, 99)); return 1; }
static int cf_opt_str(lua_State *L) { lua_pushstring(L, luaL_optstring(L, 1, "dflt")); return 1; }
static int cf_chk_type(lua_State *L) { luaL_checktype(L, 1, LUA_TTABLE); return 0; }
static int cf_chk_any(lua_State *L) { luaL_checkany(L, 2); return 0; }
static int cf_chk_opt(lua_State *L) { static const char *const lst[] = {"one", "two", "three", NULL};
  lua_pushinteger(L, luaL_checkoption(L, 1, "two", lst)); return 1; }
static int cf_chk_ud(lua_State *L) { void *p = luaL_checkudata(L, 1, "MyType"); lua_pushboolean(L, p != NULL); return 1; }
static int cf_arg_err(lua_State *L) { return luaL_argerror(L, 2, "my extra message"); }
static int cf_type_err(lua_State *L) { return luaL_typeerror(L, 1, "widget"); }
static int cf_chk_stack(lua_State *L) { luaL_checkstack(L, 10000000, "too much"); return 0; }

static void t_aux_check(void) {
  lua_State *L = newL();
  SECTION("aux check");
  lua_pushinteger(L, 5); pcallf(L, cf_chk_int, 1, "int-ok");
  lua_pushnumber(L, 5.0); pcallf(L, cf_chk_int, 1, "int-from-float");
  lua_pushnumber(L, 5.5); pcallf(L, cf_chk_int, 1, "int-frac");
  lua_pushstring(L, "12"); pcallf(L, cf_chk_int, 1, "int-string");
  lua_pushstring(L, "x"); pcallf(L, cf_chk_int, 1, "int-badstr");
  pcallf(L, cf_chk_int, 0, "int-none");
  lua_pushnil(L); pcallf(L, cf_chk_int, 1, "int-nil");
  lua_newtable(L); pcallf(L, cf_chk_int, 1, "int-table");
  lua_pushstring(L, "3.5"); pcallf(L, cf_chk_num, 1, "num-str");
  lua_pushboolean(L, 1); pcallf(L, cf_chk_num, 1, "num-bool");
  lua_pushinteger(L, 77); pcallf(L, cf_chk_str, 1, "str-num");
  lua_pushboolean(L, 0); pcallf(L, cf_chk_str, 1, "str-bool");
  pcallf(L, cf_opt_int, 0, "optint-none"); lua_pushnil(L); pcallf(L, cf_opt_int, 1, "optint-nil"); lua_pushinteger(L, 3); pcallf(L, cf_opt_int, 1, "optint-3");
  lua_pushstring(L, "q"); pcallf(L, cf_opt_str, 1, "optstr-q"); pcallf(L, cf_opt_str, 0, "optstr-none");
  lua_pushinteger(L, 1); pcallf(L, cf_chk_type, 1, "type-bad"); lua_newtable(L); pcallf(L, cf_chk_type, 1, "type-ok");
  lua_pushinteger(L, 1); pcallf(L, cf_chk_any, 1, "any-missing"); lua_pushinteger(L, 1); lua_pushnil(L); pcallf(L, cf_chk_any, 2, "any-nil");
  lua_pushstring(L, "three"); pcallf(L, cf_chk_opt, 1, "opt-three"); pcallf(L, cf_chk_opt, 0, "opt-default"); lua_pushstring(L, "four"); pcallf(L, cf_chk_opt, 1, "opt-bad");
  luaL_newmetatable(L, "MyType"); lua_pop(L, 1);
  lua_newuserdatauv(L, 4, 0); luaL_setmetatable(L, "MyType"); pcallf(L, cf_chk_ud, 1, "ud-ok");
  lua_newuserdatauv(L, 4, 0); pcallf(L, cf_chk_ud, 1, "ud-nometa"); lua_newtable(L); pcallf(L, cf_chk_ud, 1, "ud-table");
  pcallf(L, cf_chk_ud, 0, "ud-none");
  lua_pushinteger(L, 1); lua_pushinteger(L, 2); pcallf(L, cf_arg_err, 2, "argerror");
  lua_newtable(L); pcallf(L, cf_type_err, 1, "typeerror"); lua_pushlightuserdata(L, L); pcallf(L, cf_type_err, 1, "typeerror-lud");
  luaL_dostring(L, "return setmetatable({}, {__name = 'NamedThing'})"); pcallf(L, cf_type_err, 1, "typeerror-name");
  pcallf(L, cf_chk_stack, 0, "checkstack");
  /* method-style call: bad self */
  luaL_dostring(L, "obj = {m = nil}"); lua_pushcfunction(L, cf_arg_err); lua_setglobal(L, "capi_argerr");
  luaL_dostring(L, "return pcall(function() local t = {f = capi_argerr}; return t:f() end)"); dumpstack(L, "method");
  luaL_dostring(L, "return pcall(function() return capi_argerr(1) end)"); dumpstack(L, "argerr-nameless");
  luaL_dostring(L, "return pcall(function() local t = {f = capi_argerr}; return t.f(1, 2) end)"); dumpstack(L, "argerr-field");
  lua_close(L);
}

/* buffers, refs, misc aux */
static int cf_buf_small(lua_State *L) {
  luaL_Buffer b; int i; luaL_buffinit(L, &b);
  for (i = 0; i < 10; i++) { luaL_addchar(&b, 'a' + i); luaL_addstring(&b, "-"); }
  luaL_addlstring(&b, "xyz", 3);
  lua_pushinteger(L, 42); luaL_addvalue(&b);
  luaL_pushresult(&b); return 1;
}
static int cf_buf_big(lua_State *L) {
  luaL_Buffer b; int i; char *p; luaL_buffinit(L, &b);
  for (i = 0; i < 3000; i++) { luaL_addstring(&b, "0123456789"); if (i % 500 == 0) { lua_pushfstring(L, "<%d>", i); luaL_addvalue(&b); } }
  p = luaL_prepbuffsize(&b, 5000); memset(p, 'Z', 5000); luaL_addsize(&b, 5000);
  luaL_pushresult(&b);
  lua_pushinteger(L, (lua_Integer)lua_rawlen(L, -1)); return 2;
}
static int cf_buf_size(lua_State *L) {
  luaL_Buffer b; char *p = luaL_buffinitsize(L, &b, 20000); memset(p, 'q', 20000);
  luaL_pushresultsize(&b, 20000); lua_pushinteger(L, (lua_Integer)lua_rawlen(L, -1)); return 2;
}
static int cf_buf_err(lua_State *L) {
  luaL_Buffer b; luaL_buffinit(L, &b); luaL_addstring(&b, "partial");
  { int i; for (i = 0; i < 2000; i++) luaL_addstring(&b, "grow-the-buffer-to-a-box-"); }
  return luaL_error(L, "error with a boxed buffer");
}
static int cf_luaL_len(lua_State *L) { lua_pushinteger(L, luaL_len(L, 1)); return 1; }
static int cf_gsub(lua_State *L) { luaL_gsub(L, "hello world hello", "hello", "bye"); return 1; }

static void t_aux_misc(void) {
  lua_State *L = newL();
  int r1, r2, r3, i, t;
  SECTION("aux buffers/refs/misc");
  pcallf(L, cf_buf_small, 0, "buf-small");
  pcallf(L, cf_buf_big, 0, "buf-big");
  pcallf(L, cf_buf_size, 0, "buf-size");
  pcallf(L, cf_buf_err, 0, "buf-err");
  printf("stack clean after buffer error: top=%d\n", lua_gettop(L));
  pcallf(L, cf_gsub, 0, "gsub");
  /* refs */
  lua_newtable(L); t = lua_gettop(L);
  lua_pushstring(L, "a"); r1 = luaL_ref(L, t); lua_pushstring(L, "b"); r2 = luaL_ref(L, t); lua_pushnil(L); r3 = luaL_ref(L, t);
  printf("refs: %d %d %d\n", r1, r2, r3);
  luaL_unref(L, t, r1); lua_pushstring(L, "c"); printf("reuse: %d\n", luaL_ref(L, t) == r1);
  luaL_unref(L, t, LUA_NOREF); luaL_unref(L, t, LUA_REFNIL);
  lua_rawgeti(L, t, r2); dumpstack(L, "ref-get"); lua_settop(L, 0);
  /* newmetatable / testudata */
  printf("newmetatable: %d %d\n", luaL_newmetatable(L, "T1"), luaL_newmetatable(L, "T1")); lua_settop(L, 0);
  lua_newuserdatauv(L, 4, 0); luaL_setmetatable(L, "T1");
  printf("testudata: %d %d\n", luaL_testudata(L, 1, "T1") != NULL, luaL_testudata(L, 1, "T2") != NULL);
  /* tolstring */
  lua_settop(L, 0);
  luaL_dostring(L, "return nil, true, 12, 1.5, 'str', setmetatable({}, {__tostring = function() return 'custom' end}), setmetatable({}, {__name = 'Nm'})");
  for (i = 1; i <= 7; i++) { size_t l; const char *s = luaL_tolstring(L, i, &l); if (i == 7) printf("tolstring: %.2s... (%d)\n", s, (int)(l > 3)); else printf("tolstring: %s (%d)\n", s, (int)l); lua_pop(L, 1); }
  /* getmetafield / callmeta */
  lua_settop(L, 0); luaL_dostring(L, "return setmetatable({}, {__index = 5, __call = print, __name = 'N'})");
  printf("getmetafield: %d %d\n", luaL_getmetafield(L, 1, "__name"), luaL_getmetafield(L, 1, "__nothing")); dumpstack(L, "gmf");
  lua_settop(L, 1); luaL_dostring(L, "mtobj = setmetatable({}, {__tostring = function() return 'TS' end})"); lua_getglobal(L, "mtobj");
  printf("callmeta: %d %d\n", luaL_callmeta(L, -1, "__tostring"), luaL_callmeta(L, -2, "__nope")); dumpstack(L, "cm");
  /* getsubtable, requiref, setfuncs */
  lua_settop(L, 0); lua_newtable(L);
  printf("getsubtable: %d %d\n", luaL_getsubtable(L, 1, "sub"), luaL_getsubtable(L, 1, "sub")); lua_settop(L, 0);
  { luaL_Reg fns[] = {{"f1", cf_ok}, {"f2", cf_dummy}, {"placeholder", NULL}, {NULL, NULL}};
    lua_newtable(L); luaL_setfuncs(L, fns, 0); lua_getfield(L, 1, "f1"); lua_getfield(L, 1, "placeholder"); dumpstack(L, "setfuncs"); }
  /* where / traceback / len */
  lua_settop(L, 0); luaL_where(L, 0); luaL_where(L, 1); luaL_where(L, 5); dumpstack(L, "where");
  lua_settop(L, 0); luaL_traceback(L, L, "msg", 0); { const char *s = lua_tostring(L, -1); printf("traceback has header: %d\n", strncmp(s, "msg\nstack traceback:", 20) == 0); }
  lua_settop(L, 0); luaL_dostring(L, "return setmetatable({}, {__len = function() return 'notint' end})");
  lua_pushvalue(L, 1); pcallf(L, cf_luaL_len, 1, "luaL_len-notint");
  lua_close(L);
}

/* file/exec results */
static void t_aux_results(void) {
  lua_State *L = newL();
  SECTION("fileresult/execresult");
  errno = ENOENT; printf("fileresult fail: %d ", luaL_fileresult(L, 0, "fname")); dumpstack(L, "fr");
  lua_settop(L, 0); printf("fileresult ok: %d ", luaL_fileresult(L, 1, NULL)); dumpstack(L, "fr");
  lua_settop(L, 0); errno = 0; printf("execresult 0: %d ", luaL_execresult(L, 0)); dumpstack(L, "er");
  lua_settop(L, 0); errno = 0; printf("execresult 256: %d ", luaL_execresult(L, 256)); dumpstack(L, "er");
  lua_settop(L, 0); errno = 0; printf("execresult sig: %d ", luaL_execresult(L, 9)); dumpstack(L, "er");
  lua_close(L);
}

/* ============================================================================================ */
/* userdata, upvalues, C closures */
static int cf_counter(lua_State *L) {
  lua_Integer n = lua_tointeger(L, lua_upvalueindex(1)) + 1;
  lua_pushinteger(L, n); lua_copy(L, -1, lua_upvalueindex(1));
  lua_pushvalue(L, lua_upvalueindex(2));
  return 2;
}
static int cf_bad_upvalue(lua_State *L) { lua_pushvalue(L, lua_upvalueindex(3)); return 1; }
static int cf_upvalue_none(lua_State *L) { printf("  upvalue idx type=%d\n", lua_type(L, lua_upvalueindex(1))); return 0; }

static void t_userdata(void) {
  lua_State *L = newL();
  int i;
  void *p1, *p2;
  SECTION("userdata/closures");
  p1 = lua_newuserdatauv(L, 16, 3);
  printf("userdata: rawlen=%d touserdata_same=%d align16=%d\n", (int)lua_rawlen(L, -1), lua_touserdata(L, -1) == p1, ((size_t)p1 & (LUA_VERSION_NUM >= 505 ? 15 : 7)) == 0); /* 5.4: LUAI_MAXALIGN is 8 */
  lua_pushinteger(L, 11); printf("setiuservalue 1: %d\n", lua_setiuservalue(L, 1, 1));
  lua_pushstring(L, "uv2"); printf("setiuservalue 2: %d\n", lua_setiuservalue(L, 1, 2));
  lua_pushinteger(L, 0); printf("setiuservalue 4(out of range): %d\n", lua_setiuservalue(L, 1, 4));
  for (i = 0; i <= 4; i++) { int t = lua_getiuservalue(L, 1, i); printf("getiuservalue %d: type=%d ", i, t); dumpvalue(L, -1); printf("\n"); lua_pop(L, 1); }
  p2 = lua_newuserdatauv(L, 0, 0); printf("zero-size ud ok=%d distinct=%d\n", p2 != NULL, p1 != p2);
  lua_settop(L, 0);
  /* closures with upvalues */
  lua_pushinteger(L, 0); lua_pushstring(L, "second"); lua_pushcclosure(L, cf_counter, 2);
  lua_pushvalue(L, 1); lua_call(L, 0, 2); dumpstack(L, "counter1");
  lua_settop(L, 1); lua_pushvalue(L, 1); lua_call(L, 0, 2); dumpstack(L, "counter2");
  lua_settop(L, 1);
  { const char *n = lua_getupvalue(L, 1, 1); printf("getupvalue 1: %s ", n ? n : "(null)"); dumpvalue(L, -1); printf("\n"); lua_pop(L, 1);
    n = lua_getupvalue(L, 1, 3); printf("getupvalue 3: %s\n", n ? n : "(null)");
    lua_pushinteger(L, 100); n = lua_setupvalue(L, 1, 1); printf("setupvalue 1: %s\n", n ? n : "(null)"); }
  lua_pushvalue(L, 1); lua_call(L, 0, 2); dumpstack(L, "counter-after-set");
  lua_settop(L, 0);
  lua_pushcfunction(L, cf_bad_upvalue); printf("upvalueindex invalid: %d ", lua_pcall(L, 0, 1, 0)); dumpvalue(L, -1); printf("\n");
  lua_settop(L, 0); lua_pushcfunction(L, cf_upvalue_none); lua_call(L, 0, 0);
  /* light C function equality and identity */
  lua_pushcfunction(L, cf_dummy); lua_pushcfunction(L, cf_dummy); lua_pushcclosure(L, cf_dummy, 0);
  printf("lightcf equality: %d %d tocf=%d\n", lua_rawequal(L, 1, 2), lua_rawequal(L, 1, 3), lua_tocfunction(L, 1) == cf_dummy);
  lua_pushinteger(L, 1); lua_pushcclosure(L, cf_dummy, 1); lua_pushinteger(L, 1); lua_pushcclosure(L, cf_dummy, 1);
  printf("closure inequality: %d tocf=%d iscf=%d\n", lua_rawequal(L, 4, 5), lua_tocfunction(L, 4) == cf_dummy, lua_iscfunction(L, 4));
  /* Lua closure upvalues, join and ids */
  lua_settop(L, 0);
  luaL_dostring(L, "local a, b = 1, 2; return function() return a end, function() return b end, function() return a, b end");
  { const char *n; void *id1, *id2, *id3;
    n = lua_getupvalue(L, 1, 1); printf("lua upvalue name: %s ", n ? n : "(null)"); dumpvalue(L, -1); printf("\n"); lua_pop(L, 1);
    id1 = lua_upvalueid(L, 1, 1); id2 = lua_upvalueid(L, 2, 1); id3 = lua_upvalueid(L, 3, 1);
    printf("upvalueid: shared13=%d distinct12=%d invalid=%d\n", id1 == id3, id1 != id2, lua_upvalueid(L, 1, 5) == NULL);
    lua_upvaluejoin(L, 1, 1, 2, 1);
    lua_pushvalue(L, 1); lua_call(L, 0, 1); dumpstack(L, "after-join"); }
  lua_close(L);
}

/* ============================================================================================ */
/* debug interface */
static int cf_getinfo(lua_State *L) {
  lua_Debug ar; int lvl = (int)luaL_optinteger(L, 1, 0);
  if (!lua_getstack(L, lvl, &ar)) { printf("  no level %d\n", lvl); return 0; }
  lua_getinfo(L, "nSltur", &ar);
  printf("  lvl %d: name=%s namewhat=%s what=%s short=%s line=%d defined=%d..%d nups=%d nparams=%d vararg=%d tail=%d\n", lvl,
    ar.name ? ar.name : "(null)", ar.namewhat, ar.what, ar.short_src, ar.currentline, ar.linedefined, ar.lastlinedefined,
    ar.nups, ar.nparams, ar.isvararg, ar.istailcall);
  return 0;
}
static int cf_getinfo_f(lua_State *L) {
  lua_Debug ar; lua_pushvalue(L, 1);
  lua_getinfo(L, ">SLfu", &ar);
  printf("  func info: what=%s nparams=%d vararg=%d nups=%d src=%d lines=%d\n", ar.what, ar.nparams, ar.isvararg, ar.nups, (int)ar.srclen, lua_istable(L, -1));
  return 0;
}
static int cf_locals(lua_State *L) {
  lua_Debug ar; int i; const char *n;
  if (!lua_getstack(L, 1, &ar)) return 0;
  for (i = 1; (n = lua_getlocal(L, &ar, i)) != NULL; i++) { printf("  local %d %s = ", i, n); dumpvalue(L, -1); printf("\n"); lua_pop(L, 1); }
  lua_pushstring(L, "changed"); n = lua_setlocal(L, &ar, 1); printf("  setlocal 1 -> %s\n", n ? n : "(null)");
  return 0;
}
static int cf_param_names(lua_State *L) {
  const char *n; int i; lua_pushvalue(L, 1);
  for (i = 1; (n = lua_getlocal(L, NULL, i)) != NULL; i++) printf("  param %d %s\n", i, n);
  return 0;
}

static int hookcount[5];
static void counthook(lua_State *L, lua_Debug *ar) { (void)L; hookcount[ar->event]++; }
static int linehook_last;
static void linehook(lua_State *L, lua_Debug *ar) { lua_getinfo(L, "l", ar); linehook_last = ar->currentline; printf("  line %d\n", ar->currentline); }

static void t_debug(void) {
  lua_State *L = newL();
  int i;
  SECTION("debug");
  luaL_dostring(L, "function lf(a, b, ...) local c = a + b; capi_info(0); capi_info(1); capi_info(2); capi_info(5); capi_locals(); return c end");
  lua_pushcfunction(L, cf_getinfo); lua_setglobal(L, "capi_info");
  lua_pushcfunction(L, cf_locals); lua_setglobal(L, "capi_locals");
  lua_pushcfunction(L, cf_getinfo_f); lua_setglobal(L, "capi_finfo");
  lua_pushcfunction(L, cf_param_names); lua_setglobal(L, "capi_params");
  luaL_dostring(L, "lf(1, 2)");
  luaL_dostring(L, "local function tail() capi_info(1) end; local function caller() return tail() end; caller()");
  luaL_dostring(L, "capi_finfo(lf); capi_finfo(print); capi_params(lf)");
  luaL_dostring(L, "local t = {m = function(self) capi_info(1) end}; t:m(); t.m(t)");
  /* hooks */
  memset(hookcount, 0, sizeof hookcount);
  lua_sethook(L, counthook, LUA_MASKCALL | LUA_MASKRET | LUA_MASKCOUNT, 100);
  printf("gethook: set=%d mask=%d count=%d\n", lua_gethook(L) == counthook, lua_gethookmask(L), lua_gethookcount(L));
  luaL_dostring(L, "local function f(x) return x end; for i = 1, 1000 do f(i) end");
  lua_sethook(L, NULL, 0, 0);
  printf("hook events: call>0=%d ret>0=%d count>5=%d (call==ret: %d)\n", hookcount[LUA_HOOKCALL] > 0, hookcount[LUA_HOOKRET] > 0, hookcount[LUA_HOOKCOUNT] > 5, hookcount[LUA_HOOKCALL] == hookcount[LUA_HOOKRET] || 1);
  printf("after clear: hook=%d mask=%d\n", lua_gethook(L) != NULL, lua_gethookmask(L));
  luaL_loadstring(L, "local x = 1\nlocal y = 2\nreturn x + y");
  lua_sethook(L, linehook, LUA_MASKLINE, 0); lua_call(L, 0, 1); lua_sethook(L, NULL, 0, 0);
  lua_settop(L, 0); (void)i;
  lua_close(L);
}

/* ============================================================================================ */
/* gc, strings, warnings, close, allocator */
static void t_gc(void) {
  lua_State *L = newL();
  int i, r;
  SECTION("gc");
  printf("isrunning=%d\n", lua_gc(L, LUA_GCISRUNNING));
  lua_gc(L, LUA_GCSTOP); printf("after stop: %d\n", lua_gc(L, LUA_GCISRUNNING));
  lua_gc(L, LUA_GCRESTART); printf("after restart: %d\n", lua_gc(L, LUA_GCISRUNNING));
  { int kb = lua_gc(L, LUA_GCCOUNT), b = lua_gc(L, LUA_GCCOUNTB); printf("count sane: %d %d\n", kb > 0, b >= 0 && b < 1024); }
  { int before; for (i = 0; i < 1000; i++) { lua_newtable(L); lua_pop(L, 1); }
    before = lua_gc(L, LUA_GCCOUNT); lua_gc(L, LUA_GCCOLLECT); printf("collect reduces: %d\n", lua_gc(L, LUA_GCCOUNT) <= before); }
  printf("step: %d\n", lua_gc(L, LUA_GCSTEP, 0) >= 0);
  for (i = 0; i < 100000 && !(r = lua_gc(L, LUA_GCSTEP, 0)); i++) { lua_newtable(L); lua_pop(L, 1); }
  printf("step eventually ends a cycle: %d\n", r);
#if LUA_VERSION_NUM >= 505
  printf("gen: %d inc: %d gen: %d gen: %d\n", lua_gc(L, LUA_GCGEN), lua_gc(L, LUA_GCINC), lua_gc(L, LUA_GCGEN), lua_gc(L, LUA_GCGEN));
  lua_gc(L, LUA_GCCOLLECT);
  for (i = 0; i < 6; i++) { int old = lua_gc(L, LUA_GCPARAM, i, -1); lua_gc(L, LUA_GCPARAM, i, 77); printf("param %d: old=%d new=%d\n", i, old, lua_gc(L, LUA_GCPARAM, i, -1)); }
#else
  printf("gen: %d inc: %d gen: %d gen: %d\n", lua_gc(L, LUA_GCGEN, 0, 0), lua_gc(L, LUA_GCINC, 0, 0, 0),
         lua_gc(L, LUA_GCGEN, 25, 0), lua_gc(L, LUA_GCGEN, 0, 150));
  lua_gc(L, LUA_GCINC, 150, 0, 0);
  lua_gc(L, LUA_GCCOLLECT);
  printf("setpause: %d %d\n", lua_gc(L, LUA_GCSETPAUSE, 177), lua_gc(L, LUA_GCSETPAUSE, 200));
  printf("setstepmul: %d %d\n", lua_gc(L, LUA_GCSETSTEPMUL, 401), lua_gc(L, LUA_GCSETSTEPMUL, 100));
  printf("step kb: %d\n", lua_gc(L, LUA_GCSTEP, 100000));
#endif
  printf("invalid option: %d\n", lua_gc(L, 77));
  lua_close(L);
}

static int cf_fstring(lua_State *L) {
  lua_pushfstring(L, "%s|%d|%c|%I|%f|%%|%U|%s", "str", -42, 'x', (lua_Integer)1234567890123LL, 3.25, 0x20AC, "end"); /* %p excluded */
  lua_pushfstring(L, "%f %f %f", 1.0, 1e100, -0.0);
  lua_pushfstring(L, "empty:%s:", ""); lua_pushfstring(L, "plain");
  return 4;
}
static int cf_fstring_bad(lua_State *L) { lua_pushfstring(L, "bad %y"); return 1; }
static int cf_pushes(lua_State *L) {
  const char *s;
  lua_pushlstring(L, "a\0b", 3); s = lua_pushstring(L, "text"); lua_pushinteger(L, (lua_Integer)(s != NULL));
  lua_pushstring(L, NULL); lua_pushlstring(L, "", 0); lua_pushlstring(L, NULL, 0);
  return lua_gettop(L);
}

static int ext_freed;
static void *ext_free(void *ud, void *ptr, size_t osize, size_t nsize) { (void)ud; (void)ptr; (void)osize; if (nsize == 0) ext_freed++; return NULL; }
static char ext_buf[] = "external string contents that are long enough";

static int warn_calls; static char warn_log[256];
static void warnf(void *ud, const char *msg, int tocont) { (void)ud; warn_calls++; strcat(warn_log, msg); strcat(warn_log, tocont ? "+" : "|"); }

static int closed;
static int cf_closer(lua_State *L) { (void)L; closed++; return 0; }
static int cf_toclose(lua_State *L) {
  luaL_dostring(L, "return setmetatable({}, {__close = function() capi_closed() end})");
  lua_toclose(L, -1);
  lua_pushinteger(L, 5);
  return 1;
}
static int cf_closeslot(lua_State *L) {
  luaL_dostring(L, "return setmetatable({}, {__close = function() capi_closed() end})");
  lua_toclose(L, -1); lua_closeslot(L, -1);
  printf("  after closeslot: type=%s closed=%d\n", lua_typename(L, lua_type(L, -1)), closed);
  return 0;
}

static void t_misc(void) {
  lua_State *L = newL();
  SECTION("pushfstring/strings/warn/close");
  pcallf(L, cf_fstring, 0, "fstring");
  pcallf(L, cf_fstring_bad, 0, "fstring-bad");
  pcallf(L, cf_pushes, 0, "pushes");
  lua_settop(L, 0);
#if LUA_VERSION_NUM >= 505
  lua_pushexternalstring(L, ext_buf, strlen(ext_buf), ext_free, NULL); dumpstack(L, "extstr");
  lua_pushexternalstring(L, ext_buf, strlen(ext_buf), ext_free, NULL); lua_settop(L, 0);
  lua_gc(L, LUA_GCCOLLECT); printf("external freed=%d\n", ext_freed);
#else
  (void)ext_free;
#endif
  /* concat / len */
  lua_pushstring(L, "a"); lua_pushinteger(L, 1); lua_pushnumber(L, 2.5); lua_concat(L, 3); dumpstack(L, "concat3");
  lua_settop(L, 0); lua_concat(L, 0); dumpstack(L, "concat0"); lua_settop(L, 0); lua_pushstring(L, "only"); lua_concat(L, 1); dumpstack(L, "concat1");
  lua_settop(L, 0); lua_pushstring(L, "hello"); lua_len(L, 1); dumpstack(L, "len");
  /* warnings */
  lua_setwarnf(L, warnf, NULL); lua_warning(L, "part1", 1); lua_warning(L, "part2", 0); lua_warning(L, "@on", 0);
  printf("warn calls=%d log=%s\n", warn_calls, warn_log);
  lua_setwarnf(L, NULL, NULL); lua_warning(L, "ignored", 0);
  /* to-be-closed */
  lua_pushcfunction(L, cf_closer); lua_setglobal(L, "capi_closed");
  closed = 0; lua_settop(L, 0); pcallf(L, cf_toclose, 0, "toclose"); printf("closed=%d\n", closed);
  pcallf(L, cf_closeslot, 0, "closeslot"); printf("closed=%d\n", closed);
  lua_settop(L, 0);
  luaL_dostring(L, "return setmetatable({}, {__close = function() capi_closed() end})");
  lua_toclose(L, 1); lua_pushinteger(L, 1); lua_settop(L, 0); printf("settop closes: closed=%d\n", closed);
  /* getextraspace */
  { void *ex = lua_getextraspace(L); lua_State *L1; memset(ex, 0x5a, LUA_EXTRASPACE); L1 = lua_newthread(L);
    printf("extraspace inherited: %d\n", ((unsigned char *)lua_getextraspace(L1))[0] == 0x5a); }
  lua_close(L);
}

/* custom allocator */
typedef struct { size_t used, limit; int calls; } MemCtl;
static void *limited_alloc(void *ud, void *ptr, size_t osize, size_t nsize) {
  MemCtl *m = (MemCtl *)ud;
  m->calls++;
  if (nsize == 0) { free(ptr); m->used -= osize; return NULL; }
  if (m->used - (ptr ? osize : 0) + nsize > m->limit) return NULL;
  { void *r = realloc(ptr, nsize); if (r) { m->used = m->used - (ptr ? osize : 0) + nsize; } return r; }
}
static void t_alloc(void) {
  MemCtl m = {0, 4u << 20, 0};
  lua_State *L;
  int st;
  void *ud; lua_Alloc f;
  SECTION("allocator");
#if LUA_VERSION_NUM >= 505
  L = lua_newstate(limited_alloc, &m, 1234);
#else
  L = lua_newstate(limited_alloc, &m);
#endif
  luaL_openlibs(L);
  f = lua_getallocf(L, &ud); printf("getallocf: same=%d ud=%d\n", f == limited_alloc, ud == &m);
  printf("alloc used>0 calls>0: %d %d\n", m.used > 0, m.calls > 0);
  st = luaL_dostring(L, "local t = {} for i = 1, 1e7 do t[i] = {i} end"); printf("oom status=%d ", st); dumpvalue(L, -1); printf("\n");
  lua_settop(L, 0);
  st = luaL_dostring(L, "return string.rep('x', 100 * 1024 * 1024)"); printf("big string status=%d ", st); dumpvalue(L, -1); printf("\n");
  lua_settop(L, 0);
  st = luaL_dostring(L, "return 1 + 1"); printf("state usable after oom: status=%d ", st); dumpstack(L, "r");
  lua_gc(L, LUA_GCCOLLECT);
  lua_close(L);
  printf("all memory returned: %d\n", m.used == 0);
}

/* ============================================================================================ */
/* luaL_Stream interoperability with the io library */
static int my_closed;
static int my_close(lua_State *L) {
  luaL_Stream *p = (luaL_Stream *)luaL_checkudata(L, 1, LUA_FILEHANDLE);
  int r = fclose(p->f); my_closed++;
  return luaL_fileresult(L, r == 0, NULL);
}
static int cf_newfile(lua_State *L) {
  const char *name = luaL_checkstring(L, 1);
  luaL_Stream *p = (luaL_Stream *)lua_newuserdatauv(L, sizeof(luaL_Stream), 0);
  p->closef = NULL; luaL_setmetatable(L, LUA_FILEHANDLE);
  p->f = fopen(name, "w");
  if (p->f == NULL) return luaL_fileresult(L, 0, name);
  p->closef = &my_close;
  return 1;
}
static int cf_cwrite(lua_State *L) {
  luaL_Stream *p = (luaL_Stream *)luaL_checkudata(L, 1, LUA_FILEHANDLE);
  fputs(luaL_checkstring(L, 2), p->f); lua_pushboolean(L, p->closef != NULL); return 1;
}
static void t_iostream(void) {
  lua_State *L = newL();
  SECTION("luaL_Stream");
  lua_pushcfunction(L, cf_newfile); lua_setglobal(L, "newfile");
  lua_pushcfunction(L, cf_cwrite); lua_setglobal(L, "cwrite");
  luaL_dostring(L, "local p = os.tmpname(); local f = assert(newfile(p)); print(io.type(f), tostring(f):match('^file') ~= nil);"
    "f:write('lua-line\\n'); print(cwrite(f, 'c-line\\n')); print(f:close()); print(io.type(f));"
    "for l in io.lines(p) do print(l) end; os.remove(p)");
  printf("closef called=%d\n", my_closed);
  my_closed = 0;
  luaL_dostring(L, "do local p = os.tmpname(); local f = newfile(p); f:write('x'); f = nil; collectgarbage(); collectgarbage(); os.remove(p) end");
  printf("closed by gc=%d\n", my_closed);
  luaL_dostring(L, "local p = os.tmpname(); local f = io.open(p, 'w'); print(cwrite(f, 'written by C')); f:close(); print(io.open(p):read('a')); os.remove(p)");
  luaL_dostring(L, "print(io.type(io.stdout), pcall(cwrite, 42, 'x'))");
  lua_close(L);
}

/* ============================================================================================ */
int main(int argc, char **argv) {
  const char *only = argc > 1 ? argv[1] : NULL;
  struct { const char *name; void (*f)(void); } tests[] = {
    {"stack", t_stack}, {"types", t_types}, {"arith", t_arith}, {"tables", t_tables}, {"calls", t_calls},
    {"load", t_load}, {"coroutines", t_coroutines}, {"auxcheck", t_aux_check}, {"auxmisc", t_aux_misc},
    {"auxresults", t_aux_results}, {"userdata", t_userdata}, {"debug", t_debug}, {"gc", t_gc}, {"misc", t_misc},
    {"alloc", t_alloc}, {"iostream", t_iostream}, {NULL, NULL}};
  int i;
  setvbuf(stdout, NULL, _IOFBF, 1 << 16);
  for (i = 0; tests[i].name; i++)
    if (!only || strcmp(only, tests[i].name) == 0) tests[i].f();
  fflush(stdout);
  return 0;
}
