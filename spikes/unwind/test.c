#include <stdio.h>
#include <string.h>
#include <stdlib.h>
typedef int (*cfunc)(void *);
extern void spike_error(void *) __attribute__((noreturn));
extern int spike_pcall(void *, cfunc);
extern const char *spike_pushfstring(void *, const char *, ...);
extern int spike_error_f(void *, const char *, ...);
static int cleaned = 0;
static void cleanup(int *p) { (void)p; cleaned++; }   /* __attribute__((cleanup)) runs during unwind if unwind tables present */
__attribute__((noinline)) static int inner(void *L) { int x __attribute__((cleanup(cleanup))) = 0; (void)x; spike_error(L); return 0; }
__attribute__((noinline)) static int mid(void *L) { volatile int r = inner(L); return r + 1; }
__attribute__((noinline)) static int outer(void *L) { volatile int r = mid(L); return r + 1; }
static int fmt_ok(void *L) { const char *s = spike_pushfstring(L, "n=%d s=%s f=%.3f", 42, "hi", 3.14159); return strcmp(s, "n=42 s=hi f=3.142") != 0; }
static int fmt_err(void *L) { return spike_error_f(L, "bad arg #%d (%s expected)", 2, "number"); }
static int noerr(void *L) { (void)L; return 0; }
int main(void) {
  int ok = 1, r;
  r = spike_pcall(0, noerr);  printf("no error        -> %d (want 0)\n", r); ok &= r == 0;
  r = spike_pcall(0, outer);  printf("error thru 3 C frames -> %d (want 1), cleanup ran=%d (info: only runs with -fexceptions)\n", r, cleaned); ok &= r == 1;
  r = spike_pcall(0, fmt_ok); printf("vararg shim fmt -> %d (want 0)\n", r); ok &= r == 0;
  r = spike_pcall(0, fmt_err);printf("vararg shim error -> %d (want 1)\n", r); ok &= r == 1;
  for (int i = 0; i < 200000; i++) spike_pcall(0, outer);  /* cost loop */
  puts(ok ? "SPIKE OK" : "SPIKE FAIL"); return !ok;
}
