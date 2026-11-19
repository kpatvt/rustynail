#!/bin/bash
# copies every benchmarked binary to an equal-length 3-char directory: bin/<name>/rnlua
cd $HOME/.cache/rnnb
for pair in p55:dist-p55 g55:dist-g55 p54:dist-p54 g54:dist-g54 b1p:dist-b1p b1g:dist-b1g; do
  d=${pair%%:*}; s=${pair##*:}; mkdir -p bin/$d; cp $s/rnlua bin/$d/rnlua
done
mkdir -p bin/n55 bin/n54
cp tnj55/release/rnlua bin/n55/rnlua; cp tnj54/release/rnlua bin/n54/rnlua
strip -s bin/n55/rnlua bin/n54/rnlua
ls -l bin/*/rnlua
for d in p55 g55 p54 g54 n55 n54 b1p b1g; do echo $d $(bin/$d/rnlua --version </dev/null | head -n 1) $(bin/$d/rnlua --version </dev/null | grep JIT); done
