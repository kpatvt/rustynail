/* Deterministic reference runner: like `lua script.lua args...` but the state is created with a FIXED hash seed (0),
 * so string-key table iteration order is reproducible and can be compared with rustynail's Heap::new(0).
 * Built by `cargo xtask ref-build` against the vendored reference library. Test tooling only. */
#include <stdio.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
int main(int argc, char **argv) {
  lua_State *L;
  int i, st;
  if (argc < 2) { fprintf(stderr, "usage: refrun script.lua [args]\n"); return 2; }
#if LUA_VERSION_NUM >= 505
  L = lua_newstate(luaL_alloc, NULL, 0u);
#else
  L = luaL_newstate(); /* library built with -Dluai_makeseed()=0 */
#endif
  if (!L) return 1;
  luaL_openlibs(L);
  lua_newtable(L);
  for (i = 2; i < argc; i++) { lua_pushstring(L, argv[i]); lua_rawseti(L, -2, i - 1); }
  lua_setglobal(L, "arg");
  st = luaL_loadfile(L, argv[1]);
  if (st == LUA_OK) st = lua_pcall(L, 0, 0, 0);
  if (st != LUA_OK) { fprintf(stderr, "%s\n", lua_tostring(L, -1)); return 1; }
  lua_close(L);
  return 0;
}
