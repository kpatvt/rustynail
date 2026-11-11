#!/bin/bash
# bm3 follow-up: split t1q vs amt for regex_nfa, sel_loop, tonumber_num (+ parser_combinator, gsub_tbl as controls)
set -u
OUT=$1
WT=/path/to/worktree
cd $WT
B=$HOME/.cache/bm3
e="env RUSTYNAIL_SEED=0"
N=$B/1e6ec4ca/tplain/release/lua-cli; NP=$B/1e6ec4ca/pgoout/pgo/release/lua-cli
P=$B/80f1d96e/tplain/release/lua-cli; PP=$B/80f1d96e/pgoout/pgo/release/lua-cli
L() { echo "$1=$e RUSTYNAIL_JIT_TRACE_LIMITS=$2 $3"; }
COLS="base=$e $N;$(L amt0 amt=0 $N);$(L t1q1 t1q=1 $N);$(L t1q5 t1q=5 $N);$(L t1q9 t1q=9 $N);prev=$e $P;pbase=$e $NP;$(L pamt0 amt=0 $NP);$(L pt1q1 t1q=1 $NP);pprev=$e $PP"
CPUS=4 RUNS=15 OUT=$OUT/knobs2_t1q_amt_cpu4_15runs.json SETS=corpus,corpus2/dev,lib ONLY=lib/sel_loop,corpus2/dev/regex_nfa,lib/tonumber_num SPIN_MS=300 COLS="$COLS" python3 scripts/bench_session.py
echo KNOBS-DONE
