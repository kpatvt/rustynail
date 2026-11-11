#!/bin/bash
# VC builds. s55 = HEAD (8f422153) tree, sb3 = bm3 main 1e6ec4ca tree. Release script variants + nojit + bm3 rebuilds.
export PATH="$HOME/.cargo/bin:$PATH"
V=$HOME/.cache/vcBuild; CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
st() { echo "$(date +%T) $*"; }
cd $V/s55
st "release plain 5.5"; scripts/release-build.sh --out $V/dist-p55 > $V/logs/rel-p55.log 2>&1 || st "FAIL rel-p55"
st "release plain 5.4"; scripts/release-build.sh --lua54 --out $V/dist-p54 > $V/logs/rel-p54.log 2>&1 || st "FAIL rel-p54"
st "release pgo 5.5"; scripts/release-build.sh --pgo --out $V/dist-g55 > $V/logs/rel-g55.log 2>&1 || st "FAIL rel-g55"
st "release pgo 5.4"; scripts/release-build.sh --pgo --lua54 --out $V/dist-g54 > $V/logs/rel-g54.log 2>&1 || st "FAIL rel-g54"
st "nojit 5.5"; CARGO_TARGET_DIR=$V/tnj55 $CAP timeout 1800 cargo build --release -p lua-cli --no-default-features </dev/null > $V/logs/nj55.log 2>&1 || st "FAIL nj55"
st "nojit 5.4"; RUSTFLAGS="--cfg rustynail_lua54" LUA_VERSION=5.4 CARGO_TARGET_DIR=$V/tnj54 $CAP timeout 1800 cargo build --release -p lua-cli --no-default-features </dev/null > $V/logs/nj54.log 2>&1 || st "FAIL nj54"
cd $V/sb3
st "bm3 plain"; CARGO_TARGET_DIR=$V/tb3 $CAP timeout 1800 cargo build --release -p lua-cli --features jit </dev/null > $V/logs/b3p.log 2>&1 || st "FAIL b3p"
st "bm3 pgo"; scripts/pgo-build.sh $V/b3pgo </dev/null > $V/logs/b3g.log 2>&1 || st "FAIL b3g"
st "ALL DONE"
