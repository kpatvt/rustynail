#!/bin/bash
# wait for run_rest.sh (JS) to end, then the 15-run reruns with FORCE_STALE=1 (luajit upgraded 2026-11-23; the rerun compares subject vs 5a064d7 only)
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-a70b4d7021ef0234a
for i in $(seq 400); do kill -0 3340 2>/dev/null || break; sleep 15; done
FORCE_STALE=1 bash bench/results/session-l/run_rerun.sh
