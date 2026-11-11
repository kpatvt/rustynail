/* An error raised through the API with no protected call: the panic function runs, then abort. */
#include <stdio.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

static int custom_panic(lua_State *L) {
  fprintf(stderr, "custom panic: %s\n", lua_tostring(L, -1));
  return 0;
}

int main(int argc, char **argv) {
  lua_State *L = luaL_newstate();
  luaL_openlibs(L);
  if (argc > 1) lua_atpanic(L, custom_panic);
  fprintf(stderr, "before\n");
  lua_pushstring(L, "unprotected failure");
  lua_error(L);
  fprintf(stderr, "not reached\n");
  return 0;
}
