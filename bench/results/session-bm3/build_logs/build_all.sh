#!/bin/bash
# bm3: n = 1e6ec4ca (current main), p = 80f1d96e (previous subject, rebuilt)
B=$HOME/.cache/bm3
for h in 1e6ec4ca 80f1d96e; do
  echo "$(date +%T) plain $h"; $B/build.sh $h plain > $B/log-plain-$h.txt 2>&1 || echo "FAIL plain $h"
done
echo "$(date +%T) nojit"; $B/build.sh 1e6ec4ca nojit > $B/log-nojit.txt 2>&1 || echo "FAIL nojit"
for h in 1e6ec4ca 80f1d96e; do
  echo "$(date +%T) pgo $h"; $B/build.sh $h pgo > $B/log-pgo-$h.txt 2>&1 || echo "FAIL pgo $h"
done
echo "$(date +%T) ALL DONE"
