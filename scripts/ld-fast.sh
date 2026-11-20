#!/bin/sh
# Linker driver used through CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER (scripts/fast-build-env.sh): the system C compiler
# driver with mold. Falls back to the plain driver when this compiler does not know -fuse-ld=mold.
t=$(mktemp -d) || exec cc "$@"
printf 'int main(void){return 0;}\n' > "$t/a.c"
if cc -fuse-ld=mold -o "$t/a" "$t/a.c" >/dev/null 2>&1; then rm -rf "$t"; exec cc "$@" -fuse-ld=mold; fi
rm -rf "$t"; exec cc "$@"
