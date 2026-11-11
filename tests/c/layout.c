/* Prints the ABI layout of the C API structs; compared with the Rust definitions by the tests. */
#include <stdio.h>
#include <stddef.h>
#include "lua.h"
#include "lauxlib.h"

int main(void) {
  printf("lua_Debug size=%zu align=%zu\n", sizeof(lua_Debug), _Alignof(lua_Debug));
#define F(s, f) printf(#s "." #f " offset=%zu size=%zu\n", offsetof(s, f), sizeof(((s *)0)->f))
  F(lua_Debug, event); F(lua_Debug, name); F(lua_Debug, namewhat); F(lua_Debug, what); F(lua_Debug, source);
  F(lua_Debug, srclen); F(lua_Debug, currentline); F(lua_Debug, linedefined); F(lua_Debug, lastlinedefined);
  F(lua_Debug, nups); F(lua_Debug, nparams); F(lua_Debug, isvararg);
#if LUA_VERSION_NUM >= 505
  F(lua_Debug, extraargs);
#endif
  F(lua_Debug, istailcall);
  F(lua_Debug, ftransfer); F(lua_Debug, ntransfer); F(lua_Debug, short_src); F(lua_Debug, i_ci);
  printf("luaL_Buffer size=%zu align=%zu\n", sizeof(luaL_Buffer), _Alignof(luaL_Buffer));
  F(luaL_Buffer, b); F(luaL_Buffer, size); F(luaL_Buffer, n); F(luaL_Buffer, L); F(luaL_Buffer, init);
  printf("luaL_Reg size=%zu\n", sizeof(luaL_Reg)); F(luaL_Reg, name); F(luaL_Reg, func);
  printf("luaL_Stream size=%zu\n", sizeof(luaL_Stream)); F(luaL_Stream, f); F(luaL_Stream, closef);
  printf("LUAL_BUFFERSIZE=%d LUA_IDSIZE=%d LUA_EXTRASPACE=%zu LUA_REGISTRYINDEX=%d\n", (int)LUAL_BUFFERSIZE, LUA_IDSIZE,
         (size_t)LUA_EXTRASPACE, LUA_REGISTRYINDEX);
  printf("lua_Integer=%zu lua_Number=%zu lua_KContext=%zu LUA_MINSTACK=%d MULTRET=%d\n", sizeof(lua_Integer), sizeof(lua_Number),
         sizeof(lua_KContext), LUA_MINSTACK, LUA_MULTRET);
  printf("RIDX globals=%d mainthread=%d\n", LUA_RIDX_GLOBALS, LUA_RIDX_MAINTHREAD);
  return 0;
}
