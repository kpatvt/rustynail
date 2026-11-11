/* Runs a command (stdin/stdout/stderr -> /dev/null) and prints its peak RSS in KiB (ru_maxrss).
 * A tiny parent matters: ru_maxrss of an exec'ed child starts at the high-water mark of the forking process,
 * so forking from python would put a 9-10 MB floor under every number.
 * build: cc -O2 -o maxrss scripts/pprof-maxrss.c ; use: maxrss prog args... */
#include <fcntl.h>
#include <stdio.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
  if (argc < 2) return 2;
  pid_t p = fork();
  if (p == 0) {
    int fd = open("/dev/null", O_RDWR);
    dup2(fd, 0); dup2(fd, 1); dup2(fd, 2);
    execvp(argv[1], argv + 1);
    _exit(127);
  }
  int st;
  struct rusage ru;
  wait4(p, &st, 0, &ru);
  if (!WIFEXITED(st) || WEXITSTATUS(st) != 0) { printf("FAIL\n"); return 1; }
  printf("%ld\n", ru.ru_maxrss);
  return 0;
}
