/*
** A sample C module ("samplemod") for the `require`/`package.loadlib` tests. It is built as a
** shared object WITHOUT linking against any Lua library: the `lua_*` symbols are resolved from the
** host executable, as for real-world modules.
*/
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include "lua.h"
#include "lauxlib.h"

#define VEC "samplemod.vec"

typedef struct { int n; double v[1]; } Vec;
static int gc_count = 0;

static Vec *checkvec(lua_State *L, int i) { return (Vec *)luaL_checkudata(L, i, VEC); }

static int vec_new(lua_State *L) {
  int n = (int)luaL_checkinteger(L, 1), i;
  Vec *v;
  luaL_argcheck(L, n >= 0 && n < 100000, 1, "invalid size");
  v = (Vec *)lua_newuserdatauv(L, sizeof(Vec) + (n > 0 ? n - 1 : 0) * sizeof(double), 1);
  v->n = n;
  for (i = 0; i < n; i++) v->v[i] = 0;
  luaL_setmetatable(L, VEC);
  lua_pushstring(L, "tag"); lua_setiuservalue(L, -2, 1);
  return 1;
}
static int vec_get(lua_State *L) {
  Vec *v = checkvec(L, 1); lua_Integer i = luaL_checkinteger(L, 2);
  luaL_argcheck(L, 1 <= i && i <= v->n, 2, "index out of range");
  lua_pushnumber(L, v->v[i - 1]); return 1;
}
static int vec_set(lua_State *L) {
  Vec *v = checkvec(L, 1); lua_Integer i = luaL_checkinteger(L, 2);
  luaL_argcheck(L, 1 <= i && i <= v->n, 2, "index out of range");
  v->v[i - 1] = luaL_checknumber(L, 3); return 0;
}
static int vec_len(lua_State *L) { lua_pushinteger(L, checkvec(L, 1)->n); return 1; }
static int vec_tostring(lua_State *L) {
  Vec *v = checkvec(L, 1); luaL_Buffer b; int i;
  luaL_buffinit(L, &b); luaL_addchar(&b, '<');
  for (i = 0; i < v->n; i++) { if (i) luaL_addstring(&b, ","); lua_pushnumber(L, v->v[i]); luaL_addvalue(&b); }
  luaL_addchar(&b, '>'); luaL_pushresult(&b); return 1;
}
static int vec_gc(lua_State *L) { (void)checkvec(L, 1); gc_count++; return 0; }
static int vec_tag(lua_State *L) { checkvec(L, 1); lua_getiuservalue(L, 1, 1); return 1; }
static int vec_eq(lua_State *L) {
  Vec *a = checkvec(L, 1), *b = checkvec(L, 2); int i;
  if (a->n != b->n) { lua_pushboolean(L, 0); return 1; }
  for (i = 0; i < a->n; i++) if (a->v[i] != b->v[i]) { lua_pushboolean(L, 0); return 1; }
  lua_pushboolean(L, 1); return 1;
}

static int sum(lua_State *L) { /* sum of all numbers in a table (array and hash part) */
  double s = 0;
  luaL_checktype(L, 1, LUA_TTABLE);
  lua_pushnil(L);
  while (lua_next(L, 1)) { if (lua_isnumber(L, -1)) s += lua_tonumber(L, -1); lua_pop(L, 1); }
  lua_pushnumber(L, s); return 1;
}

static int counter_fn(lua_State *L) {
  lua_Integer n = lua_tointeger(L, lua_upvalueindex(1)) + lua_tointeger(L, lua_upvalueindex(2));
  lua_pushinteger(L, n); lua_copy(L, -1, lua_upvalueindex(1)); return 1;
}
static int counter(lua_State *L) {
  lua_pushinteger(L, luaL_optinteger(L, 1, 0)); lua_pushinteger(L, luaL_optinteger(L, 2, 1));
  lua_pushcclosure(L, counter_fn, 2); return 1;
}

static int callback(lua_State *L) { /* callback(f, ...) -> status, results... with a message handler */
  int n = lua_gettop(L), st;
  luaL_checktype(L, 1, LUA_TFUNCTION);
  lua_getglobal(L, "tostring"); lua_insert(L, 1);          /* handler at 1 */
  st = lua_pcall(L, n - 1, LUA_MULTRET, 1);
  lua_pushinteger(L, st); lua_replace(L, 1);
  return lua_gettop(L);
}

static int gen_k(lua_State *L, int status, lua_KContext ctx) {
  lua_Integer i = (lua_Integer)ctx;
  lua_Integer n = lua_tointeger(L, 1);
  (void)status;
  if (i >= n) { lua_pushstring(L, "done"); return 1; }
  lua_pushinteger(L, i + 1);
  return lua_yieldk(L, 1, (lua_KContext)(i + 1), gen_k);
}
static int gen(lua_State *L) { luaL_checkinteger(L, 1); return gen_k(L, LUA_OK, 0); }

static int fail(lua_State *L) {
  const char *kind = luaL_checkstring(L, 1);
  if (strcmp(kind, "string") == 0) return luaL_error(L, "failure %d (%s)", 7, "text");
  if (strcmp(kind, "table") == 0) { lua_newtable(L); lua_pushstring(L, "data"); lua_setfield(L, -2, "info"); return lua_error(L); }
  if (strcmp(kind, "arg") == 0) return luaL_argerror(L, 1, "custom problem");
  if (strcmp(kind, "type") == 0) return luaL_typeerror(L, 2, "gadget");
  return 0;
}

static int fmt(lua_State *L) {
  luaL_Buffer b; int i, n = lua_gettop(L);
  luaL_buffinit(L, &b);
  for (i = 1; i <= n; i++) {
    luaL_tolstring(L, i, NULL);   /* uses __tostring / __name */
    luaL_addvalue(&b);
    if (i < n) luaL_addchar(&b, '|');
  }
  luaL_pushresult(&b); return 1;
}
static int bigstring(lua_State *L) { /* exercises buffer growth into a box */
  luaL_Buffer b; int i, n = (int)luaL_checkinteger(L, 1);
  luaL_buffinit(L, &b);
  for (i = 0; i < n; i++) luaL_addstring(&b, "0123456789abcdef");
  luaL_pushresult(&b); return 1;
}

static int refs(lua_State *L) { /* store values in the registry and read them back */
  int r1, r2;
  lua_pushstring(L, "first"); r1 = luaL_ref(L, LUA_REGISTRYINDEX);
  lua_pushvalue(L, 1); r2 = luaL_ref(L, LUA_REGISTRYINDEX);
  lua_rawgeti(L, LUA_REGISTRYINDEX, r1); lua_rawgeti(L, LUA_REGISTRYINDEX, r2);
  luaL_unref(L, LUA_REGISTRYINDEX, r1); luaL_unref(L, LUA_REGISTRYINDEX, r2);
  return 2;
}

static int newfile(lua_State *L) { /* an io-compatible file handle created by C */
  const char *name = luaL_checkstring(L, 1);
  luaL_Stream *p = (luaL_Stream *)lua_newuserdatauv(L, sizeof(luaL_Stream), 0);
  p->closef = NULL; luaL_setmetatable(L, LUA_FILEHANDLE);
  p->f = fopen(name, "w"); if (!p->f) return luaL_fileresult(L, 0, name);
  p->closef = (lua_CFunction)0; /* set below */
  return 1;
}
static int newfile_close(lua_State *L) { luaL_Stream *p = (luaL_Stream *)luaL_checkudata(L, 1, LUA_FILEHANDLE); int r = fclose(p->f); return luaL_fileresult(L, r == 0, NULL); }
static int newfile2(lua_State *L) { int n = newfile(L); if (n == 1) ((luaL_Stream *)lua_touserdata(L, -1))->closef = newfile_close; return n; }

static int gccount(lua_State *L) { lua_pushinteger(L, gc_count); return 1; }
static int version(lua_State *L) { lua_pushnumber(L, lua_version(L)); lua_pushstring(L, LUA_VERSION); return 2; }
static int info(lua_State *L) { /* debug interface from C */
  lua_Debug ar;
  if (!lua_getstack(L, 1, &ar)) return 0;
  lua_getinfo(L, "nSl", &ar);
  lua_pushstring(L, ar.short_src); lua_pushinteger(L, ar.currentline); lua_pushstring(L, ar.name ? ar.name : "?");
  return 3;
}

static const luaL_Reg vec_methods[] = {
  {"get", vec_get}, {"set", vec_set}, {"len", vec_len}, {"tag", vec_tag},
  {"__len", vec_len}, {"__tostring", vec_tostring}, {"__gc", vec_gc}, {"__eq", vec_eq}, {NULL, NULL}};

static const luaL_Reg funcs[] = {
  {"new", vec_new}, {"sum", sum}, {"counter", counter}, {"callback", callback}, {"gen", gen}, {"fail", fail},
  {"fmt", fmt}, {"bigstring", bigstring}, {"refs", refs}, {"newfile", newfile2}, {"gccount", gccount},
  {"version", version}, {"info", info}, {NULL, NULL}};

int luaopen_samplemod(lua_State *L) {
  luaL_checkversion(L);
  if (luaL_newmetatable(L, VEC)) {
    luaL_setfuncs(L, vec_methods, 0);
    lua_pushvalue(L, -1); lua_setfield(L, -2, "__index");
  }
  lua_pop(L, 1);
  luaL_newlib(L, funcs);
  return 1;
}
