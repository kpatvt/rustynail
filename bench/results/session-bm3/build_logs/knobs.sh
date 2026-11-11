#!/bin/bash
# bm3 knob attribution: same binary (1e6ec4ca) with bundles' knobs OFF via RUSTYNAIL_JIT_TRACE_LIMITS. usage: knobs.sh <out-dir>
set -u
OUT=$1
WT=/path/to/worktree
cd $WT
B=$HOME/.cache/bm3
e="env RUSTYNAIL_SEED=0"
N=$B/1e6ec4ca/tplain/release/lua-cli; NP=$B/1e6ec4ca/pgoout/pgo/release/lua-cli
P=$B/80f1d96e/tplain/release/lua-cli; PP=$B/80f1d96e/pgoout/pgo/release/lua-cli
L() { echo "$1=$e RUSTYNAIL_JIT_TRACE_LIMITS=$2 $3"; }
COLS="base=$e $N;base2=$e $N;$(L libcall7 libcall=7 $N);$(L t1q1amt0 t1q=1,amt=0 $N);$(L retf0 retf=0,sidenest=0,tfmt=0 $N);$(L rearm0 rearmside=0 $N);$(L alloff libcall=7,t1q=1,amt=0,retf=0,sidenest=0,tfmt=0,rearmside=0 $N);prev=$e $P;pbase=$e $NP;$(L palloff libcall=7,t1q=1,amt=0,retf=0,sidenest=0,tfmt=0,rearmside=0 $NP);pprev=$e $PP"
python3 scripts/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
CPUS=4 RUNS=15 OUT=$OUT/knobs_attribution_cpu4_15runs.json SETS=corpus,corpus2/dev,corpus2/heldout,lib ONLY=lib/gsub_tbl,corpus/parser_combinator,lib/sel_loop,corpus2/dev/regex_nfa,corpus/csv_ini_yaml,lib/tonumber_int,lib/tonumber_num,lib/error_tbl,corpus2/dev/varargs_util,lib/concat_op_str,lib/pcall_err,lib/char SPIN_MS=300 COLS="$COLS" python3 scripts/bench_session.py
python3 scripts/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
echo KNOBS-DONE
