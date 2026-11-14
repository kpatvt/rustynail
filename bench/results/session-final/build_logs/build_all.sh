#!/bin/bash
# session-final builds. s55 = HEAD tree (108b935), s9 = 9a9b8ed (bm5 subject), sb = 8a18eb3 (previous release table baseline).
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_BUILD_JOBS=8
V=<cache>; CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
st() { echo "$(date +%T) $*"; }
rb() { t=$1; o=$2; shift 2; st "release $o"; (cd $V/$t && scripts/release-build.sh "$@" --out $V/dist-$o > $V/logs/rel-$o.log 2>&1 </dev/null) || st "FAIL rel-$o"; }
rb s55 p55 --no-pgo
rb s55 p54 --no-pgo --lua54
st "nojit 5.5"; (cd $V/s55; CARGO_TARGET_DIR=$V/tnj55 $CAP timeout 1800 cargo build --release -p rn-cli --no-default-features </dev/null > $V/logs/nj55.log 2>&1) || st "FAIL nj55"
st "nojit 5.4"; (cd $V/s55; RUSTFLAGS="--cfg rustynail_lua54" LUA_VERSION=5.4 CARGO_TARGET_DIR=$V/tnj54 $CAP timeout 1800 cargo build --release -p rn-cli --no-default-features </dev/null > $V/logs/nj54.log 2>&1) || st "FAIL nj54"
rb s9 b9p --no-pgo
rb sb b8p --no-pgo
rb s55 g55
rb s55 g54 --lua54
rb s9 b9g
rb sb b8g
st "ALL DONE"
