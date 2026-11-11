#!/bin/bash
cd /path/to/cache/vcBuild
for b in dist-p55/rnlua dist-g55/rnlua dist-p54/rnlua dist-g54/rnlua tnj55/release/rnlua tnj54/release/rnlua tb3/release/lua-cli b3pgo/pgo/release/lua-cli; do
  echo "$b: $($b --version </dev/null | head -1)"
done
for d in dist-p55 dist-g55 dist-p54 dist-g54; do (cd $d && echo $d && sha256sum -c SHA256SUMS | tr '\n' ' '; echo); done
