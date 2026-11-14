#!/bin/bash
# copies every benchmarked binary to an equal-length 3-char directory: bin/<name>/rnlua
cd <cache>
for pair in p55:dist-p55 g55:dist-g55 p54:dist-p54 g54:dist-g54 b9p:dist-b9p b9g:dist-b9g b8p:dist-b8p b8g:dist-b8g; do
  d=${pair%%:*}; s=${pair##*:}; mkdir -p bin/$d; cp $s/rnlua bin/$d/rnlua
done
mkdir -p bin/n55 bin/n54
cp tnj55/release/rnlua bin/n55/rnlua; cp tnj54/release/rnlua bin/n54/rnlua
strip -s bin/n55/rnlua bin/n54/rnlua
ls -l bin/*/rnlua
for d in p55 g55 p54 g54 n55 n54 b9p b9g b8p b8g; do echo $d $(bin/$d/rnlua --version </dev/null | head -n 1) $(bin/$d/rnlua --version </dev/null | grep JIT); done
