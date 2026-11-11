# Reproducible builds

Goal: the same commit built with the same toolchain gives bit-identical binaries, wherever the
checkout, the target directory and the user's home are. Verified on 2026-10-28 for `rnlua` (Lua 5.5),
the default and the fast (`-C relocation-model=static`) builds, and for the whole `dist/` tree of
`scripts/release.sh` (both Lua versions).

## Toolchain (pinned)

| Component | Version used for the verification |
|---|---|
| rustc / cargo | 1.99.1 (`58cf8f9c3` 2026-10-13), LLVM 23.1.1; `rust-toolchain.toml` says `channel = "stable"` plus `rust-src`, `llvm-tools-preview` |
| C compiler (the `rn-capi` variadic shim, via the `cc` crate) | gcc 16.2.1 |
| linker / archiver | GNU ld and ar 2.47 (binutils) |
| host | x86_64 Linux 7.3 (Arch family), glibc |
| Cargo.lock | committed; `cargo deny` checks sources (crates.io only) |

`rust-toolchain.toml` tracks `stable`, so a fresh checkout picks up a newer compiler over time; for a
bit-identical rebuild of an existing release use the exact toolchain recorded in its
`BUILD-INFO.txt`: `rustup toolchain install 1.99.1 --component rust-src,llvm-tools-preview` and
`RUSTUP_TOOLCHAIN=1.99.1 scripts/release.sh ...` (or `cargo +1.99.1 ...`). Different gcc/binutils
versions change only the C shim and link layout, but they do change the binary.

> **Update (2026-11-11).** The path remapping now lives in one place, `scripts/release-env.sh` (sourced by `release-build.sh`,
> `release.sh`, `pgo.sh`, `build-fast.sh`, `bolt.sh`): checkout -> `/rustynail`, cargo home -> `/cargo`, rustup home -> `/rustup`,
> toolchain sysroot -> `/rustc`, home directory -> `/build-home`, plus the C shim's `-ffile-prefix-map`. It is a superset of the
> flags in the block below (which is the original, still valid, recipe for the `default` and `fast` variants). Shipped executables
> and libraries are stripped (`strip --strip-all` / `--strip-debug`; deterministic), and the scripts check them for the builder's home
> directory, user name and host name. `BUILD-INFO.txt` records only the commit, the date, rustc, the target triple and `uname -sm`.
> The release default is now the PGO build, which is not bit-reproducible (below); `scripts/release-build.sh --no-pgo` and
> `scripts/release.sh --variants default,fast` are the reproducible variants (the verification table below was taken with the earlier, smaller flag set; not repeated).

## Exact commands

```sh
export SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)      # recorded in BUILD-INFO.txt; see below
ROOT=$(pwd); TD=$ROOT/target/dist; SYSROOT=$(rustc --print sysroot)
RUSTFLAGS="--remap-path-prefix=$ROOT=/rustynail --remap-path-prefix=$HOME/.cargo=/cargo \
           --remap-path-prefix=$SYSROOT=/rustc --remap-path-prefix=$TD=/target" \
CFLAGS="-ffile-prefix-map=$ROOT=/rustynail -ffile-prefix-map=$TD=/target" \
CARGO_TARGET_DIR=$TD cargo build --release -p rn-cli -p rn-capi
# 5.4: prepend "--cfg rustynail_lua54" to RUSTFLAGS (and use another target dir)
# fast build: the same environment, then scripts/build-fast.sh (adds -C relocation-model=static to rnlua)
```

`scripts/release.sh --variants default,fast` runs exactly this for each version (target dirs
`target/dist`, `target/dist-fast`, `target-54/...`) and writes `BUILD-INFO.txt` and `SHA256SUMS`.

Environment that matters:
- `RUSTFLAGS` / `CFLAGS`: as above. Any other flag (e.g. `-C target-cpu=native`) changes the output and
  must be recorded; the release builds use the default `x86-64` target CPU.
- `SOURCE_DATE_EPOCH`: nothing in the build embeds a date or time (no `__DATE__`/`__TIME__` in the C
  shim, no build-time timestamps in Rust, the GNU `ar` used by the `cc` crate and rustc's own staticlib
  writer are deterministic). It is set anyway, for tools that honour it, and recorded.
- `CARGO_HOME`, the rustup home and the checkout location only reach the binary through paths, which
  the remapping removes (below).
- No network access is needed once the registry crates are in the cargo cache (`cargo fetch`).

## What was verified (2026-10-28, commit of this document)

| Case | Result |
|---|---|
| default build, two clean target directories, same checkout, **no** remapping | identical |
| default build, two clean target directories, remapping | identical |
| default build, remapping, a second checkout at another path (`git archive` copy) | identical to the first |
| fast build (`relocation-model=static`), two clean target directories, no remapping | identical |
| fast build, two clean target directories, remapping | identical |
| `scripts/release.sh --versions 5.5,5.4 --variants default,fast`, twice from clean target dirs | identical `SHA256SUMS` (all binaries, `librn_capi.a`, headers, licenses) |
| `.text` of a plain clean build vs `target/release/rnlua` | identical |

What differs without remapping, and why (found with a section-by-section comparison):
- **Another checkout location**: only `.debug_info`/`.debug_str` (DWARF `DW_AT_comp_dir` is the
  absolute workspace path; source file names themselves are workspace-relative) and therefore the GNU
  build-id note (a hash of the file). Code and data sections are identical.
- **Another toolchain location / user home**: panic-location strings of generic standard-library code
  instantiated in our crates carry the absolute path of the local `rust-src` component
  (`~/.rustup/toolchains/<name>/lib/rustlib/src/rust/library/...`, 5 such strings in `.rodata`).
  Removing them shrinks `.rodata` by ~300 bytes, which shifts addresses, so `.text` changes too
  (same size, different displacements). `--remap-path-prefix=$SYSROOT=/rustc` makes this independent
  of where rustup lives; remapping `$HOME/.cargo` does the same for registry crates (`libc`).
- The target directory does not reach the binary at all (the `cc` crate's object paths are not part
  of the linked executable's sections), so `-ffile-prefix-map=$TD=...` is only a precaution.

Consequently a remapped build is *not* `.text`-identical to a plain `cargo build --release` (the
`.rodata` strings differ), but it is the same code; the performance numbers in `docs/PERF.md` are for
plain builds.

## Not reproducible by design

- **PGO** (`scripts/pgo.sh`, not checked bit for bit): the profile consists of execution counters, but
  each training run uses a random string-hash seed (`luai_makeseed`; `scripts/train.sh` pins `RUSTYNAIL_SEED=0` for the
  benchmark and training programs since BUNDLE-A, but timing-dependent JIT decisions and the official-suite part still vary), which changes table layouts,
  iteration orders and collector work, so the counters and hence the binary are expected to differ
  between two runs. Record the `.profdata` (`target/pgo-data/merged.profdata`)
  to rebuild an exact PGO binary: `RUSTFLAGS="-Cprofile-use=<file>" cargo rustc --release -p rn-cli
  --bin rnlua -- -C relocation-model=static`.
- **BOLT** (`scripts/bolt.sh`): `perf` samples are statistical; the layout depends on them. Keep
  `target/bolt/perf.fdata` to re-run `llvm-bolt` deterministically on the same base binary.
- Debug builds, `RUSTFLAGS` with `-C target-cpu=native`, and different gcc/binutils versions.

## How to check a build

```sh
sha256sum -c SHA256SUMS                                  # inside a dist/ tree
cmp a/rnlua b/rnlua && echo identical
readelf -n rnlua | grep "Build ID"                     # a hash of the contents
# which sections differ (objcopy -O binary would skip the non-allocated debug sections):
for s in .text .rodata .data.rel.ro .debug_info .debug_str .debug_line; do
  objcopy --dump-section $s=/tmp/a$s a/rnlua /dev/null; objcopy --dump-section $s=/tmp/b$s b/rnlua /dev/null
  cmp -s /tmp/a$s /tmp/b$s || echo "$s differs"
done
```
