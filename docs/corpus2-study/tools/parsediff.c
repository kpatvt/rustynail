/* Reference side of the parser differential (crates/rn-std/tests/parser_diff.rs). Test tooling only.
 *
 * Built against the reference library of one version:
 *   cc -O1 -Iref/55/build tools/parsediff.c ref/55/build/liblua.a -lm -o parsediff
 * It includes the reference `luac.c` (with its `main` renamed) to reuse `PrintFunction`, so the listing is
 * exactly `luac -l -l`. Every case is loaded the way `luac -p` loads it: `lua_load` (mode "t") inside one
 * `lua_pcall` from C, so the C-call depth (and with it the parser nesting limit) is the same as luac's.
 *
 * usage: parsediff <input> <output>
 * input:  records  u32 namelen, name, u32 srclen, src, u8 flags (bit 0: produce the listing, bit 1: load
 *         mode "bt" instead of "t")
 * output: per case u8 status (0 ok, 1 error), then
 *           error: u32 len, message
 *           ok:    u32 len, dump (non-stripped), u32 len, dump (stripped), u32 len, listing (may be 0)
 * The output is flushed after every record, so a crash of the reference loses only the current case. */
#define main luac_main
#include "luac.c"
#undef main

#include <unistd.h>

typedef struct { char *p; size_t n, cap; } Buf;

static void bput(Buf *b, const void *p, size_t n) {
  if (b->n + n > b->cap) {
    size_t c = b->cap ? b->cap * 2 : 4096;
    while (c < b->n + n) c *= 2;
    b->p = (char *)realloc(b->p, c);
    if (!b->p) { fprintf(stderr, "parsediff: out of memory\n"); exit(3); }
    b->cap = c;
  }
  memcpy(b->p + b->n, p, n);
  b->n += n;
}

static int bwriter(lua_State *L, const void *p, size_t size, void *u) {
  (void)L;
  bput((Buf *)u, p, size);
  return 0;
}

typedef struct {
  const char *name;
  const char *src;
  size_t len;
  int listing;
  int binary;
  FILE *out;
} Case;

static void wr32(FILE *f, size_t n) {
  unsigned char b[4];
  b[0] = (unsigned char)n; b[1] = (unsigned char)(n >> 8);
  b[2] = (unsigned char)(n >> 16); b[3] = (unsigned char)(n >> 24);
  fwrite(b, 1, 4, f);
}

static void wrbuf(FILE *f, const char *p, size_t n) {
  wr32(f, n);
  if (n) fwrite(p, 1, n, f);
}

/* `PrintFunction` writes to stdout: capture it through a temporary file. */
static void print_listing(const Proto *f, Buf *b) {
  static FILE *tmp = NULL;
  int saved, fd;
  off_t n;
  if (!tmp) tmp = tmpfile();
  if (!tmp) return;
  fd = fileno(tmp);
  fflush(stdout);
  saved = dup(1);
  if (ftruncate(fd, 0) != 0 || lseek(fd, 0, SEEK_SET) != 0) return;
  dup2(fd, 1);
  PrintFunction(f, 1);
  fflush(stdout);
  dup2(saved, 1);
  close(saved);
  n = lseek(fd, 0, SEEK_END);
  if (n > 0) {
    char *p = (char *)malloc((size_t)n);
    if (!p) return;
    if (pread(fd, p, (size_t)n, 0) == n) bput(b, p, (size_t)n);
    free(p);
  }
}

static int docase(lua_State *L) {
  Case *c = (Case *)lua_touserdata(L, 1);
  int st;
  tmname = G(L)->tmname;
  st = luaL_loadbufferx(L, c->src, c->len, c->name, c->binary ? "bt" : "t");
  if (st != LUA_OK) {
    size_t n;
    const char *m = lua_tolstring(L, -1, &n);
    fputc(1, c->out);
    wrbuf(c->out, m ? m : "", m ? n : 0);
  } else {
    Buf full = {0}, strip = {0}, lst = {0};
    lua_dump(L, bwriter, &full, 0);
    lua_dump(L, bwriter, &strip, 1);
    if (c->listing) print_listing(toproto(L, -1), &lst);
    fputc(0, c->out);
    wrbuf(c->out, full.p, full.n);
    wrbuf(c->out, strip.p, strip.n);
    wrbuf(c->out, lst.p, lst.n);
    free(full.p); free(strip.p); free(lst.p);
  }
  fflush(c->out);
  return 0;
}

static int rd32(FILE *f, size_t *n) {
  unsigned char b[4];
  if (fread(b, 1, 4, f) != 4) return 0;
  *n = (size_t)b[0] | ((size_t)b[1] << 8) | ((size_t)b[2] << 16) | ((size_t)b[3] << 24);
  return 1;
}

static char *rdbuf(FILE *f, size_t n) {
  char *p = (char *)malloc(n + 1);
  if (!p) { fprintf(stderr, "parsediff: out of memory\n"); exit(3); }
  if (n && fread(p, 1, n, f) != n) { free(p); return NULL; }
  p[n] = 0;
  return p;
}

int main(int argc, char **argv) {
  FILE *in, *out;
  lua_State *L;
  size_t k = 0;
  if (argc != 3) { fprintf(stderr, "usage: parsediff <input> <output>\n"); return 2; }
  in = fopen(argv[1], "rb");
  out = fopen(argv[2], "ab");
  if (!in || !out) { fprintf(stderr, "parsediff: cannot open files\n"); return 2; }
  L = luaL_newstate();
  if (!L) return 3;
  for (;;) {
    size_t nl, sl;
    char *name, *src;
    int flags;
    Case c;
    if (!rd32(in, &nl)) break;
    name = rdbuf(in, nl);
    if (!name || !rd32(in, &sl)) { fprintf(stderr, "parsediff: truncated input\n"); return 2; }
    src = rdbuf(in, sl);
    flags = fgetc(in);
    if (!src || flags == EOF) { fprintf(stderr, "parsediff: truncated input\n"); return 2; }
    c.name = name; c.src = src; c.len = sl; c.listing = flags & 1; c.binary = (flags & 2) != 0; c.out = out;
    lua_settop(L, 0);
    lua_pushcfunction(L, docase);
    lua_pushlightuserdata(L, &c);
    if (lua_pcall(L, 1, 0, 0) != LUA_OK) {
      fprintf(stderr, "parsediff: case %zu: %s\n", k, lua_tostring(L, -1));
      return 4;
    }
    free(name); free(src);
    if (++k % 256 == 0) lua_gc(L, LUA_GCCOLLECT, 0);
  }
  lua_close(L);
  fclose(out);
  return 0;
}
