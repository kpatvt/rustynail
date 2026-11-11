# Build variants and the PGO release build

Moved out of the README. Basic build commands are in the README.

## Build variants

All variants are the same code; they differ in code generation and layout. Every script takes
`LUA_VERSION=5.4` for the 5.4 build (output under `target-54/` instead of `target/`).

| Variant | Command | Binary | When to use |
|---|---|---|---|
| default (dev) | `cargo build --release -p rn-cli` | `target/release/rnlua` | the plain build (JIT on by default): PIE, reproducible; the development and reference build (`scripts/release-build.sh --no-pgo` ships it, stripped). `--no-default-features` builds the interpreter only |
| fast | `scripts/build-fast.sh` | `target/fast/release/rnlua` | absolute (non-PIC) dispatch jump table, ~1-3% faster; non-PIE executable; reproducible |
| PGO (JIT) | `scripts/pgo-build.sh [out-dir]` | `target/pgo-jit/pgo/release/rnlua` | **the default release build** (what `scripts/release-build.sh` and `scripts/release.sh` produce by default; JIT + trace compiler, 0.913 of the plain build's cycles on 223 programs): profile-guided, trained on the list in `scripts/train.sh`; not bit-reproducible (see "PGO release build" below) |
| PGO (no JIT) | `scripts/pgo.sh` | `target/pgo/release/rnlua` | the same without the JIT feature (`PGO_FEATURES=jit scripts/pgo.sh` is what `pgo-build.sh` runs) |
| BOLT | `scripts/bolt.sh` | `target/bolt/rnlua.bolt` | fast build + profile-driven code layout (needs `llvm-bolt`, `perf` with branch sampling: `scripts/setup-bolt.sh`); measured on top of PGO + JIT in BUNDLE-P: no gain (0.999 of PGO cycles), not recommended |
| PGO + BOLT | `BOLT_PGO=1 scripts/bolt.sh` | `target/pgo-bolt/rnlua.bolt` | both |

## PGO release build

```sh
scripts/pgo-build.sh                  # JIT + trace compiler, PGO: target/pgo-jit/pgo/release/rnlua
scripts/pgo-build.sh ~/.cache/rn-pgo  # any out-dir (build dirs, profile and binary live there)
LUA_VERSION=5.4 scripts/pgo-build.sh  # Lua 5.4;  PGO_REPEAT=2: train twice;  PGO_HOLDOUT=1: see below
```

One command from a clean checkout, about 10 minutes: an instrumented build (`-Cprofile-generate`), a training run
over the one list in `scripts/train.sh` (the benchmark sets `bench/{awfy,apps,programs,lib,jit-trace,real}`, the
independent programs `bench/train/*` including GC-heavy and string-heavy ones, `bench/micro`, `tests/vm` and 17 files
of the official suite; all with `RUSTYNAIL_SEED=0`, no clocks, stdin from `/dev/null`), `llvm-profdata merge`,
and the optimised build (`-Cprofile-use`). It needs `llvm-profdata` of rustc's LLVM: the rustup component
`llvm-tools-preview` (listed in `rust-toolchain.toml`, installed on demand), or `LLVM_PROFDATA=/path`. Nothing is
committed: the merged profile (about 2 MB) belongs to one rustc and one source revision, so it is regenerated.
Training writes one raw profile per process (`LLVM_PROFILE_FILE=...%p.profraw`, a few thousand files, ~270 MB under
`<out-dir>/pgo-data/raw`, deleted after the merge) and merges them offline (`llvm-profdata merge --input-files`):
the online `%m` merge loses indirect-call and memop value profiles (about 0.8% cycles).

**The profile must be regenerated after every change to a hot path** (interpreter, tier-1 templates, trace compiler,
GC, tables, strings, the library): re-run `scripts/pgo-build.sh`; it always starts from a fresh profile. A stale profile
still builds, but LLVM drops the profile of every function whose CFG changed and the gain there is silently lost.
Because the benchmark sets are in the training list, speedups measured on them are optimistic; `PGO_HOLDOUT=1`
trains on the independent programs only and keeps the benchmarks as a hold-out (the numbers of both are in
`docs/JIT_TRACE_REF.md`, "BUNDLE-A decisions and results").

`scripts/release-build.sh` builds the PGO binary by default (`--no-pgo` for the plain build, `--lua54` for the 5.4 series;
`--pgo` is accepted and does nothing), strips `rnlua` and `rnluac` and writes `dist/`. The release scripts remap every
build-machine path (source tree, cargo and rustup homes, home directory; `scripts/release-env.sh`) and check the stripped
files for the builder's home directory, user and host names.

`scripts/release.sh [--versions 5.5,5.4] [--variants pgo,default,fast,bolt,pgo-bolt]` (default `pgo,default`; `pgo` is the JIT + PGO build) builds the chosen
variants into `dist/` with the (stripped) C library and headers, `LICENSE` files, `BUILD-INFO.txt` (commit, date, rustc,
target triple, platform) and `SHA256SUMS`. Reproducible builds: `docs/REPRODUCIBLE.md`. Release steps: `docs/RELEASE.md`.

