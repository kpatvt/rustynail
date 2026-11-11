#!/bin/bash
# usage: build.sh <hash8> <plain|pgo|nojit>
set -eo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
H=$1; B=$HOME/.cache/bm2/$H
cd $B/src
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
case $2 in
 plain) CARGO_TARGET_DIR=$B/tplain $CAP timeout 1200 cargo build --release -p lua-cli --features jit </dev/null ;;
 nojit) CARGO_TARGET_DIR=$B/tnojit $CAP timeout 1200 cargo build --release -p lua-cli </dev/null ;;
 pgo) scripts/pgo-build.sh $B/pgoout </dev/null ;;
esac
