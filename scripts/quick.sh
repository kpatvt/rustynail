#!/bin/bash
# Fast experiment loop: build release, print instructions/iteration for a few probes (vs the reference).
cd "$(dirname "$0")/.."
export PATH=$HOME/.cargo/bin:$PATH
cargo build --release -p rn-cli 2>&1 | grep -E "^(error|warning: unused)" -A6 | head -20
python3 scripts/micro.py ref=ref/55/build/lua ours=target/release/rnlua 2>&1 | grep -E "^(empty|move|add_int|addi|lt_jump|getfield|geti|call_lua|call_arg|setfield|add_flt|self_call|settable_int|gettable_int|empty loop)" 
for b in fib loops_arith; do
  r=$(perf stat -x, -e instructions:u taskset -c 3 ref/55/build/lua bench/programs/$b.lua 2>&1 >/dev/null | cut -d, -f1)
  o=$(perf stat -x, -e instructions:u taskset -c 3 target/release/rnlua bench/programs/$b.lua 2>&1 >/dev/null | cut -d, -f1)
  python3 -c "print('$b instr ratio %.3f' % ($o/$r))"
done
