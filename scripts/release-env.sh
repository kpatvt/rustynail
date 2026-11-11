# Sourced by the release/PGO/BOLT/fast build scripts (not run directly). Provides:
#
#   RN_REMAP    rustc flags that rewrite every build-machine path embedded in a binary (panic locations, debug info,
#               `file!()`): the checkout -> /rustynail, the cargo home -> /cargo, the rustup home -> /rustup,
#               the toolchain sysroot -> /rustc, the home directory -> /build-home. rustc applies the LAST matching
#               rule, so the most specific prefixes come last.
#   RN_CFLAGS   the same for the C sources compiled by build scripts (`cc` crate reads CFLAGS).
#   rn_strip F  strips a release artifact in place (executables: --strip-all, shared objects: --strip-unneeded,
#               static archives: --strip-debug). The dynamic symbol table of rnlua (the exported C API used by C
#               modules) is kept by --strip-all.
#   rn_privacy_check F...  fails when a file still embeds the home directory, the user name or the host name.
#
# Callers add their own target directory with `rn_remap_target <dir>` when it lies outside the checkout.
_RN_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
_RN_SYSROOT=$(rustc --print sysroot)
RN_REMAP="--remap-path-prefix=$HOME=/build-home"
RN_REMAP="$RN_REMAP --remap-path-prefix=${RUSTUP_HOME:-$HOME/.rustup}=/rustup"
RN_REMAP="$RN_REMAP --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
RN_REMAP="$RN_REMAP --remap-path-prefix=$_RN_ROOT=/rustynail"
RN_REMAP="$RN_REMAP --remap-path-prefix=$_RN_SYSROOT=/rustc"
RN_CFLAGS="-ffile-prefix-map=$HOME=/build-home -ffile-prefix-map=$_RN_ROOT=/rustynail"
rn_remap_target() { # rn_remap_target <abs dir>: a target dir outside the checkout
  RN_REMAP="$RN_REMAP --remap-path-prefix=$1=/target"
  RN_CFLAGS="$RN_CFLAGS -ffile-prefix-map=$1=/target"
}
rn_strip() {
  case "$1" in
    *.a) strip --strip-debug "$1" ;;
    *.so|*.so.*) strip --strip-unneeded "$1" ;;
    *) strip --strip-all "$1" ;;
  esac
}
rn_privacy_check() {
  local f n pat
  pat="/home/|$(id -un)|$(hostname 2>/dev/null || uname -n)"
  for f in "$@"; do
    n=$(strings -a "$f" | grep -c -E "$pat" || true)
    if [ "$n" != 0 ]; then
      echo "privacy check failed: $f embeds $n build-machine strings (home dir, user or host name)" >&2
      return 1
    fi
  done
}
