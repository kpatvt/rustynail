#!/bin/bash
B=$HOME/.cache/bench1110
for h in 4ce16b7a 53133e8d 8965e73e c276c376 cb1af288; do
  echo "$(date +%T) plain $h"; $B/build.sh $h plain > $B/log-plain-$h.txt 2>&1 || echo "FAIL plain $h"
done
echo "$(date +%T) nojit"; $B/build.sh 4ce16b7a nojit > $B/log-nojit.txt 2>&1 || echo "FAIL nojit"
for h in 4ce16b7a 53133e8d 8965e73e c276c376 cb1af288; do
  echo "$(date +%T) pgo $h"; $B/build.sh $h pgo > $B/log-pgo-$h.txt 2>&1 || echo "FAIL pgo $h"
done
echo "$(date +%T) ALL DONE"
