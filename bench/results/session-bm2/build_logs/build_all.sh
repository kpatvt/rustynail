#!/bin/bash
B=$HOME/.cache/bm2
for h in 80f1d96e 79e7441c; do
  echo "$(date +%T) plain $h"; $B/build.sh $h plain > $B/log-plain-$h.txt 2>&1 || echo "FAIL plain $h"
done
echo "$(date +%T) nojit"; $B/build.sh 80f1d96e nojit > $B/log-nojit.txt 2>&1 || echo "FAIL nojit"
for h in 80f1d96e 79e7441c; do
  echo "$(date +%T) pgo $h"; $B/build.sh $h pgo > $B/log-pgo-$h.txt 2>&1 || echo "FAIL pgo $h"
done
echo "$(date +%T) ALL DONE"
