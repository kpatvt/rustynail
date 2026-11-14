#!/bin/bash
cd <cache>/rnbm5
grep -h "training" logs/rel-g55.log logs/rel-g54.log logs/rel-b3g.log | head
for pair in p55:dist-p55 g55:dist-g55 p54:dist-p54 g54:dist-g54 b3p:dist-b3p b3g:dist-b3g; do
  d=${pair%%:*}; s=${pair##*:}; mkdir -p bin/$d; cp $s/rnlua bin/$d/rnlua
done
mkdir -p bin/n55 bin/n54
cp tnj55/release/rnlua bin/n55/rnlua; cp tnj54/release/rnlua bin/n54/rnlua
strip -s bin/n55/rnlua bin/n54/rnlua
ls -l bin/*/rnlua
for d in p55 g55 p54 g54 n55 n54 b3p b3g; do echo $d $(bin/$d/rnlua --version </dev/null | head -n 1); done
