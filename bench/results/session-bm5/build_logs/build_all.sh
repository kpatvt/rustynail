#!/bin/bash
# session-bm5 builds. s55 = HEAD tree (9a9b8ed), sb = baseline tree (8a18eb3: the squashed equivalent of session-release's 8f422153).
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_BUILD_JOBS=8
V=$HOME/.cache/rnbm5; CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
st() { echo "$(date +%T) $*"; }
cd $V/s55
st "release plain 5.5"; scripts/release-build.sh --no-pgo --out $V/dist-p55 > $V/logs/rel-p55.log 2>&1 || st "FAIL rel-p55"
st "release plain 5.4"; scripts/release-build.sh --no-pgo --lua54 --out $V/dist-p54 > $V/logs/rel-p54.log 2>&1 || st "FAIL rel-p54"
st "nojit 5.5"; CARGO_TARGET_DIR=$V/tnj55 $CAP timeout 1800 cargo build --release -p rn-cli --no-default-features </dev/null > $V/logs/nj55.log 2>&1 || st "FAIL nj55"
st "nojit 5.4"; RUSTFLAGS="--cfg rustynail_lua54" LUA_VERSION=5.4 CARGO_TARGET_DIR=$V/tnj54 $CAP timeout 1800 cargo build --release -p rn-cli --no-default-features </dev/null > $V/logs/nj54.log 2>&1 || st "FAIL nj54"
cd $V/sb
st "baseline plain 5.5"; scripts/release-build.sh --no-pgo --out $V/dist-b3p > $V/logs/rel-b3p.log 2>&1 || st "FAIL rel-b3p"
cd $V/s55
st "release pgo 5.5"; scripts/release-build.sh --out $V/dist-g55 > $V/logs/rel-g55.log 2>&1 || st "FAIL rel-g55"
st "release pgo 5.4"; scripts/release-build.sh --lua54 --out $V/dist-g54 > $V/logs/rel-g54.log 2>&1 || st "FAIL rel-g54"
cd $V/sb
st "baseline pgo 5.5"; scripts/release-build.sh --out $V/dist-b3g > $V/logs/rel-b3g.log 2>&1 || st "FAIL rel-b3g"
st "ALL DONE"
