#!/usr/bin/env bash
# JIT differential tester. See docs/JIT_TESTING.md.
#   scripts/jit-diff.sh <seed-from> <seed-to> [bin]
# Env: JITDIFF_HOOK=none|count|line|both|all   hook-semantics variants (default none)
#      JITDIFF_ON=1        also run RUSTYNAIL_JIT=on with RUSTYNAIL_JIT_THRESHOLD=2 and 10
#      JITDIFF_REF=<lua>   also compare the baseline (off) against a reference lua binary ("ref" mode)
#      JITDIFF_MODES="off always ..."  explicit list of modes (first is the baseline)
#      JITDIFF_GEN=<bin>   binary used to run gen.lua (default: bin)
#      JITDIFF_ENTERS=1    counter assertion: every compiled mode is run once more with RUSTYNAIL_JIT_VERBOSE=1 and the
#                          entries into compiled code (and T15 promotions / region recompiles) are summed per
#                          (hook variant, mode); the script FAILS (NOENTERS) when a mode never entered compiled code,
#                          i.e. when its comparison with the baseline would have tested nothing
#      F9: the J4 modes pin RUSTYNAIL_JIT_TRACE=off; trdef / trdefon = the unpinned shipped defaults (JIT=always / JIT=on THRESHOLD=2)
#      trace modes (need a `rnlua --features trace` binary; JIT=always T15=off unless noted):
#        tr      TRACE=always                       tron   JIT=on THRESHOLD=2 TRACE=on TRACE_HOT=2,2
#        trstack tr + JIT_STRESS=stackmove          trstorm tr + +exitstorm=3 (trstormN: N)
#        tremu   tr + +emurun (the trace runs in the emulator)    trrand tr + +randra=<seed>
#        trverify tr + +verify     trnoopt tr + -fold,-cse,-loop,-fwd,-abc     trnosink tr + -sink
#        trj4 / tronj4 / trreg / trblack (F5: traces together with J4 regions, T15_HOT=4; region-first; every head Black)
#        tr:<opts> / tron:<opts>   any TRACE_OPT string, e.g. JITDIFF_MODES="off tr:+exitstorm=2,+randra=5 tron:-loop"
#      JITDIFF_TSET=tr|oop|q|sink|cmp|bc|gm|ba|cc|bd|bf|pd|bg|bk|bl|bp|bq|bs|bt|br|bv|none  generator template set (default: tr when a trace mode is listed, otherwise the
#                          original set, so the old modes reproduce old seeds unchanged)
#      JITDIFF_ENTERS=1 also sums `[trace] recorded/installed/entered/exits` per (hook, mode) and per seed: the script
#                          FAILS with NOTRACES when a trace mode never entered a trace (entered or exits > 0: a +exitstorm flush drops the entry counters) in the whole range and with
#                          FEWTRACES when fewer than 25% of the seeds installed AND entered a trace in `tr` or `tron`
#                          (hook=none only: with hooks traces refuse in the hooked snippets). NOTRACES_LIST=n prints
#                          the first n seeds (per hook and mode) that ran no trace
#      JITDIFF_NOULIMIT=1  do not `ulimit -v` the runs (ASan binaries map terabytes of shadow: scripts/asan-jit.sh)
#      JOBS=6 (capped at 6, JITDIFF_ALLOW_MANY=1 lifts it)  TMO=20 (seconds per run)  KEEP=1 (keep temp dir)  MINIMIZE=0 to skip minimisation
#      BUNDLE-M: the trace modes run with the recorder's lockstep cross-check ON (`RUSTYNAIL_JIT_TRACE_LIMITS` defaults to `lockstep=1` here,
#                          the shipped process default may be off); an explicit RUSTYNAIL_JIT_TRACE_LIMITS (e.g. `lockstep=1,hdrlib=0`) is
#                          used as given; `trdef` / `trdefon` always run with the shipped defaults (limits unset).
set -u
export RUSTYNAIL_JIT_TRACE_LIMITS="${RUSTYNAIL_JIT_TRACE_LIMITS-lockstep=1}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GEN="$HERE/tests/jitdiff/gen.lua"
FAILDIR="$HERE/tests/jitdiff/failures"

# one-seed worker: jit-diff.sh --one <seed> <bin> <workroot>
if [ "${1:-}" = "--one" ]; then
  seed=$2; BIN=$3; ROOT=$4
  GENBIN=${JITDIFF_GEN:-$BIN}
  TMO=${TMO:-20}
  # stackmove moves the stack at every helper call: deep recursion (the `deep` template, 150000 frames) is quadratic, 40+ s
  tmo_of() { case $1 in *stack*) echo $((TMO * 3));; *) echo "$TMO";; esac; }
  MODES=${JITDIFF_MODES:-"off always"}
  [ "${JITDIFF_ON:-0}" = 1 ] && MODES="$MODES on2 on10"
  [ -n "${JITDIFF_REF:-}" ] && MODES="$MODES ref"
  HOOKS=${JITDIFF_HOOK:-none}
  [ "$HOOKS" = all ] && HOOKS="none count line both"
  base=${MODES%% *}
  TSET=${JITDIFF_TSET:-}
  if [ -z "$TSET" ]; then case " $MODES " in *" tr"*) TSET=tr;; *) TSET=none;; esac; fi

  modeenv() { # echo env assignments for mode
    case $1 in
      off) echo "RUSTYNAIL_JIT=off";;
      always) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=off";;
      stack) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_STRESS=stackmove RUSTYNAIL_JIT_TRACE=off";;
      t15) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3 RUSTYNAIL_JIT_TRACE=off";;
      t15flush) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+flushall RUSTYNAIL_JIT_TRACE=off";;
      t15rand) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+randra=$seed RUSTYNAIL_JIT_TRACE=off";;
      t15verify) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+verify,+flushall RUSTYNAIL_JIT_TRACE=off";;
      t15verifyrand) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+verify,+randra=$seed RUSTYNAIL_JIT_TRACE=off";;
      t15shadow) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+shadow RUSTYNAIL_JIT_TRACE=off";;
      t15shadowrand) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+shadow,+randra=$seed RUSTYNAIL_JIT_TRACE=off";;
      t15storm1|t15storm2|t15storm7|t15storm61) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+exitstorm=${1#t15storm} RUSTYNAIL_JIT_TRACE=off";;
      t15storm) local ns=(1 2 7 61); echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+exitstorm=${ns[$((seed % 4))]},+randra=$seed RUSTYNAIL_JIT_TRACE=off";;
      t15stormshadow) local ns=(1 2 7 61); echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exithot=3,+exitstorm=${ns[$(((seed + 1) % 4))]},+shadow,+verify RUSTYNAIL_JIT_TRACE=off";;
      t15stormprof) local ns=(1 2 7 61); echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_T15_HOT=4 RUSTYNAIL_JIT_OPT=+force,+exithot=3,+exitstorm=${ns[$((seed % 4))]},+shadow RUSTYNAIL_JIT_TRACE=off";;
      t15prof) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_T15_HOT=4 RUSTYNAIL_JIT_OPT=+force,+exithot=3 RUSTYNAIL_JIT_TRACE=off";;
      t15profrand) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_T15_HOT=4 RUSTYNAIL_JIT_OPT=+force,+exithot=3,+randra=$seed RUSTYNAIL_JIT_TRACE=off";;
      t15on) echo "RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2 RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force RUSTYNAIL_JIT_TRACE=off";;
      on2) echo "RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2 RUSTYNAIL_JIT_TRACE=off";;
      on10) echo "RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=10 RUSTYNAIL_JIT_TRACE=off";;
      # F9: the J4 modes above pin RUSTYNAIL_JIT_TRACE=off (traces are on by default now); the modes below pin their own.
      # trdef / trdefon: the shipped default configuration (JIT=always / JIT=on THRESHOLD=2; TRACE, T15 and TRACE_HOT unset)
      trdef) echo "-u RUSTYNAIL_JIT_TRACE -u RUSTYNAIL_JIT_T15 -u RUSTYNAIL_JIT_TRACE_HOT -u RUSTYNAIL_JIT_TRACE_OPT -u RUSTYNAIL_JIT_TRACE_LIMITS RUSTYNAIL_JIT=always";;
      trdefon) echo "-u RUSTYNAIL_JIT_TRACE -u RUSTYNAIL_JIT_T15 -u RUSTYNAIL_JIT_TRACE_HOT -u RUSTYNAIL_JIT_TRACE_OPT -u RUSTYNAIL_JIT_TRACE_LIMITS RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2";;
      tr) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always";;
      tron) echo "RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2 RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=on RUSTYNAIL_JIT_TRACE_HOT=2,2";;
      trstack) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_STRESS=stackmove";;
      trstorm) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=+exitstorm=3";;
      trstorm[0-9]*) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=+exitstorm=${1#trstorm}";;
      tremu) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=+emurun";;
      trrand) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=+randra=$seed";;
      trverify) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=+verify";;
      trnoopt) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=-fold,-cse,-loop,-fwd,-abc";;
      trnosink) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=-sink";;
      trj4) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_T15_HOT=4 RUSTYNAIL_JIT_TRACE=always";;
      tronj4) echo "RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2 RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_T15_HOT=4 RUSTYNAIL_JIT_TRACE=on RUSTYNAIL_JIT_TRACE_HOT=2,2";;
      trreg) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_T15_HOT=4 RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_PREF=region";;
      trblack) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=on RUSTYNAIL_JIT_T15_HOT=4 RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=+allblack";;
      tr:*) echo "RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=${1#tr:}";;
      tron:*) echo "RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2 RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=on RUSTYNAIL_JIT_TRACE_HOT=2,2 RUSTYNAIL_JIT_TRACE_OPT=${1#tron:}";;
      ref) echo "RUSTYNAIL_JIT=off";;
      *) echo "RUSTYNAIL_JIT=$1 RUSTYNAIL_JIT_TRACE=off";;
    esac
  }
  # F6: the hook variants print hook counters that depend on the per-run hash seed (table iteration order changes the number of
  # line events: the JIT-off baseline alone varies run to run by up to ~1%), so they run with a fixed seed
  hseed() { [ "${hook:-none}" = none ] || echo "RUSTYNAIL_SEED=${RUSTYNAIL_SEED:-0}"; }
  # runmode <dir> <mode>: runs <dir>/p.lua, writes <dir>/out.<mode>, err.<mode>, rc.<mode>
  runmode() {
    local d=$1 m=$2 b=$BIN
    [ "$m" = ref ] && b=$JITDIFF_REF
    ( cd "$d" && { [ "${JITDIFF_NOULIMIT:-0}" = 1 ] || ulimit -v 6000000; } && env $(modeenv "$m") $(hseed) timeout "$(tmo_of "$m")" "$b" p.lua </dev/null >"out.$m" 2>"err.$m"; echo $? >"rc.$m"
      sed -E -i 's#^(.*/)?(rnlua|lua): #PROG: #; s/0x[0-9a-f]{6,}/0xADDR/g' "err.$m"
      sed -E -i 's/0x[0-9a-f]{6,}/0xADDR/g' "out.$m" )
  }
  # enters <dir> <mode>: run once more with RUSTYNAIL_JIT_VERBOSE=1 and record the compiled-code counters
  enters() {
    local d=$1 m=$2 b=$BIN
    [ "$m" = ref ] && return
    ( cd "$d" && { [ "${JITDIFF_NOULIMIT:-0}" = 1 ] || ulimit -v 6000000; } && env $(modeenv "$m") $(hseed) RUSTYNAIL_JIT_VERBOSE=1 timeout "$(tmo_of "$m")" "$b" p.lua </dev/null >/dev/null 2>"verb.$m" )
    local e p r
    e=$(sed -n 's/^\[jit\] compiled [0-9]* failed [0-9]* refused [0-9]* enters \([0-9]*\).*/\1/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
    p=$(sed -n 's/^\[jit\] t15: promoted \([0-9]*\) recompiled \([0-9]*\).*/\1/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
    r=$(sed -n 's/^\[jit\] t15: promoted \([0-9]*\) recompiled \([0-9]*\).*/\2/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
    local rs gu
    rs=$(sed -n 's/^\[jit\] t15 policy: .* respecialised \([0-9]*\) given_up \([0-9]*\).*/\1/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
    gu=$(sed -n 's/^\[jit\] t15 policy: .* respecialised \([0-9]*\) given_up \([0-9]*\).*/\2/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
    local sh
    sh=$(sed -n 's/^\[jit\] t15 shadow: checked \([0-9]*\) .*/\1/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
    echo "ENTERS $hook $m $e $p $r $rs $gu $sh" >>"$ROOT/results"
    case $m in tr*)
      local ti te tx
      # every state of the program prints its own summary: sum them
      ti=$(sed -n 's/^\[trace\] recorded [0-9]* installed \([0-9]*\).*/\1/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
      te=$(sed -n 's/^\[trace\] recorded [0-9]* installed [0-9]* entered \([0-9]*\).*/\1/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
      tx=$(sed -n 's/^\[trace\] exits \([0-9]*\).*/\1/p' "$d/verb.$m" | awk '{s+=$1} END {print s+0}')
      echo "TRACES $hook $m $seed $ti $te $tx" >>"$ROOT/results";;
    esac
  }
  # sinkon <mode>: can allocation sinking (J5, TR29) be active in this mode? Only the trace modes without `-sink`.
  sinkon() {
    case $1 in
      trnosink|*-sink*) return 1;;
      tr*) return 0;;
      *) return 1;;
    esac
  }
  same() { # same <dir> <modeA> <modeB>; lines tagged `GCOBS:` (GC-observable values, TR29.2) are ignored when either mode sinks
    # or is the reference lua (`ref`: its collector pacing and memory accounting differ by design from ours: the count deltas and
    # the number of finalizers run by a step/collect are the allocator's and the pacer's business, not program semantics)
    if sinkon "$2" || sinkon "$3" || [ "$2" = ref ] || [ "$3" = ref ]; then
      cmp -s <(grep -v '^GCOBS:' "$1/out.$2") <(grep -v '^GCOBS:' "$1/out.$3") && cmp -s "$1/err.$2" "$1/err.$3" && cmp -s "$1/rc.$2" "$1/rc.$3"
    else
      cmp -s "$1/out.$2" "$1/out.$3" && cmp -s "$1/err.$2" "$1/err.$3" && cmp -s "$1/rc.$2" "$1/rc.$3"
    fi
  }
  # differs <dir> <hook> <keep> <mode> -> 0 if baseline and mode differ
  differs() {
    local d=$1 hook=$2 keep=$3 m=$4
    local h=$hook; [ "$h" = none ] && h=""
    "$GENBIN" "$GEN" "$seed" "$h" "$keep" "$TSET" >"$d/p.lua" 2>"$d/gen.err" || return 1
    runmode "$d" "$base"; runmode "$d" "$m"
    ! same "$d" "$base" "$m"
  }

  w="$ROOT/s$seed"; mkdir -p "$w"
  status=ok
  for hook in $HOOKS; do
    d="$w/$hook"; mkdir -p "$d"
    h=$hook; [ "$h" = none ] && h=""
    if ! "$GENBIN" "$GEN" "$seed" "$h" "" "$TSET" >"$d/p.lua" 2>"$d/gen.err"; then
      echo "GENFAIL $seed $hook" >>"$ROOT/results"; continue
    fi
    runmode "$d" "$base"
    # a program that does not compile (or times out) is a generator problem
    if [ "$(cat "$d/rc.$base")" = 124 ]; then echo "TIMEOUT $seed $hook" >>"$ROOT/results"; fi
    if ! grep -q '^final' "$d/out.$base" && grep -q "syntax error\|expected near\|near '" "$d/err.$base" 2>/dev/null; then
      echo "GENSYNTAX $seed $hook" >>"$ROOT/results"
    fi
    for m in $MODES; do
      [ "$m" = "$base" ] && continue
      runmode "$d" "$m"
      [ "${JITDIFF_ENTERS:-0}" = 1 ] && enters "$d" "$m"
      # stackmove makes deep recursion quadratic: a timeout of a *stack* mode whose other runs finished is not a result
      if [ "$(cat "$d/rc.$m")" = 124 ] && [ "$(cat "$d/rc.$base")" != 124 ] && case $m in *stack*) true;; *) false;; esac; then
        echo "SLOWSKIP $seed $hook $m" >>"$ROOT/results"; continue
      fi
      if ! same "$d" "$base" "$m"; then
        fd="$FAILDIR/seed-$seed-$hook-$m"
        rm -rf "$fd"; mkdir -p "$fd"
        cp "$d"/p.lua "$d"/out.* "$d"/err.* "$d"/rc.* "$fd/"
        # minimise by dropping whole snippets (gen.lua arg 3 = list of snippets to keep)
        if [ "${MINIMIZE:-1}" != 0 ]; then
          n=$(sed -n 's/^-- snippets: //p' "$d/p.lua" | head -1)
          keep=""; for i in $(seq 1 "${n:-0}"); do keep="$keep,$i"; done; keep=${keep#,}
          for i in $(seq 1 "${n:-0}"); do
            try=$(echo ",$keep," | sed "s/,$i,/,/; s/^,//; s/,\$//")
            [ -z "$try" ] && continue
            md="$w/min"; rm -rf "$md"; mkdir -p "$md"
            if differs "$md" "$hook" "$try" "$m"; then keep=$try; fi
          done
          md="$w/min"; rm -rf "$md"; mkdir -p "$md"
          if differs "$md" "$hook" "$keep" "$m"; then
            cp "$md/p.lua" "$fd/min.lua"
            mkdir -p "$fd/min"; cp "$md"/out.* "$md"/err.* "$md"/rc.* "$fd/min/"
            echo "$keep" >"$fd/min.keep"
          fi
        fi
        echo "DIFF $seed $hook $m" >>"$ROOT/results"
        status=diff
      fi
    done
    rm -rf "$d"
  done
  echo "DONE $seed $status" >>"$ROOT/results"
  exit 0
fi

# ------------------------------------------------------------------ driver
if [ $# -lt 2 ]; then
  echo "usage: $0 <seed-from> <seed-to> [bin]" >&2; exit 2
fi
FROM=$1; TO=$2
BIN=${3:-$HERE/target/release/rnlua}
[ -x "$BIN" ] || { echo "no such binary: $BIN" >&2; exit 2; }
BIN=$(readlink -f "$BIN")
# trace modes need `rnlua --features trace` (build: cargo build --release -p rn-cli --features trace): check it
case " ${JITDIFF_MODES:-} " in *" tr"*)
  probe=$(mktemp "${TMPDIR:-/tmp}/jdprobe.XXXXXX")
  echo 'local s = 0 for i = 1, 5000 do s = s + i end print(s)' >"$probe"
  if ! RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_VERBOSE=1 "$BIN" "$probe" </dev/null 2>&1 | grep -q '^\[trace\] recorded'; then
    rm -f "$probe"; echo "$BIN was not built with the trace feature (cargo build --release -p rn-cli --features trace)" >&2; exit 2
  fi
  rm -f "$probe";;
esac
if [ -z "${JITDIFF_CAPPED:-}" ]; then
  export JITDIFF_CAPPED=1
  exec systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 "$0" "$FROM" "$TO" "$BIN"
fi
ROOT=$(mktemp -d "${TMPDIR:-/tmp}/jitdiff.XXXXXX")
mkdir -p "$FAILDIR"
: >"$ROOT/results"
JOBS=${JOBS:-6}
# One builder at a time is the rule, so 6 parallel runs fit the 16-CPU box; more than that oversubscribed it for hours
# when two builders overlapped. JITDIFF_ALLOW_MANY=1 lifts the cap.
if [ "${JITDIFF_ALLOW_MANY:-0}" != 1 ] && [ "$JOBS" -gt 6 ]; then echo "jit-diff: JOBS=$JOBS capped to 6 (JITDIFF_ALLOW_MANY=1 to lift)" >&2; JOBS=6; fi
trap '[ "${KEEP:-0}" = 1 ] && echo "kept $ROOT" || rm -rf "$ROOT"' EXIT
seq "$FROM" "$TO" >"$ROOT/seeds"
xargs -a "$ROOT/seeds" -P "$JOBS" -I{} nice -n 10 "$0" --one {} "$BIN" "$ROOT"
total=$(grep -c '^DONE' "$ROOT/results")
diffs=$(grep -c '^DIFF' "$ROOT/results")
# counter assertion: every (hook, mode) must have entered compiled code, T15 modes must have recompiled regions
noenters=0
if [ "${JITDIFF_ENTERS:-0}" = 1 ]; then
  while read -r hk m e p r rs gu sh; do
    echo "enters: hook=$hk mode=$m enters=$e t15_promoted=$p t15_recompiled=$r t15_respecialised=$rs t15_given_up=$gu shadow_checked=$sh"
    case $m in *shadow*) if [ "$sh" = 0 ]; then echo "NOSHADOW hook=$hk mode=$m: no block was ever shadow-verified"; noenters=1; fi;; esac
    if [ "$e" = 0 ]; then echo "NOENTERS hook=$hk mode=$m: compiled code was never entered"; noenters=1; fi
    case $m in t15*) if [ "$r" = 0 ]; then echo "NOREGIONS hook=$hk mode=$m: no region was ever generated"; noenters=1; fi;; esac
  done < <(grep '^ENTERS' "$ROOT/results" | awk '{k=$2" "$3; e[k]+=$4; p[k]+=$5; r[k]+=$6; rs[k]+=$7; gu[k]+=$8; sh[k]+=$9} END {for (k in e) print k, e[k], p[k], r[k], rs[k], gu[k], sh[k]}' | sort)
fi
# trace counters: per (hook, mode) the seeds that installed and entered a trace; NOTRACES (nothing in the whole range),
# FEWTRACES (< 25% of the seeds in tr / tron with hook=none)
if [ "${JITDIFF_ENTERS:-0}" = 1 ]; then
  while read -r hk m n k ti te tx; do
    pct=$(( n > 0 ? 100 * k / n : 0 ))
    echo "traces: hook=$hk mode=$m seeds=$n with_trace=$k ($pct%) installed=$ti entered=$te exits=$tx"
    if [ "$((te + tx))" = 0 ]; then echo "NOTRACES hook=$hk mode=$m: no trace was ever entered"; noenters=1; fi
    case "$hk $m" in "none tr"|"none tron") if [ "$pct" -lt 25 ]; then echo "FEWTRACES hook=$hk mode=$m: only $pct% of the seeds entered a trace (< 25%)"; noenters=1; fi;; esac
  done < <(grep '^TRACES' "$ROOT/results" | awk '{k=$2" "$3; n[k]++; if ($5 > 0 && ($6 > 0 || $7 > 0)) c[k]++; ti[k]+=$5; te[k]+=$6; tx[k]+=$7} END {for (k in n) print k, n[k], c[k]+0, ti[k], te[k], tx[k]}' | sort)
  if [ "${NOTRACES_LIST:-0}" -gt 0 ]; then
    grep '^TRACES' "$ROOT/results" | awk '$5 == 0 || ($6 == 0 && $7 == 0) {print "notraces: seed=" $4 " hook=" $2 " mode=" $3}' | sort -t= -k2 -n | head -"$NOTRACES_LIST"
  fi
fi
echo "jit-diff: seeds $FROM..$TO done=$total diffs=$diffs genfail=$(grep -c '^GENFAIL' "$ROOT/results") gensyntax=$(grep -c '^GENSYNTAX' "$ROOT/results") timeouts=$(grep -c "^TIMEOUT" "$ROOT/results") slowskips=$(grep -c "^SLOWSKIP" "$ROOT/results") hook=${JITDIFF_HOOK:-none}"
grep -E '^(DIFF|GENFAIL|GENSYNTAX|TIMEOUT)' "$ROOT/results" | sort -k2 -n | head -50
[ "$diffs" = 0 ] && [ "$total" = "$((TO - FROM + 1))" ] && [ "$noenters" = 0 ]
