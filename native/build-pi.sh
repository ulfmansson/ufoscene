#!/usr/bin/env bash
# Build ufo-native for the Raspberry Pi on this Mac (or any machine with rustup): static musl
# binaries linked with Rust's own rust-lld, so no cross C toolchain, Docker or Pi is needed.
#
#   /Users/ulf.mansson/IdeaProjects/ufoscene/native/build-pi.sh [arm64|armhf|all]
#
#   arm64  Raspberry Pi 3, 4, 400, 5, 500, Zero 2 W running 64-bit Raspberry Pi OS
#   armhf  Raspberry Pi 2, and any Pi running 32-bit Raspberry Pi OS
#
# Out: target/pi-native/ufo-native-arm64 and ufo-native-armhf. Needs the two Rust targets once:
#   rustup target add aarch64-unknown-linux-musl armv7-unknown-linux-musleabihf
# (UFO_SYSROOT=<dir> uses a sysroot with those targets in it instead, for a rustup that cannot
# install them.)
set -euo pipefail
cd "$(dirname "$0")/.."
# a Nix shell's DEVELOPER_DIR/SDKROOT break Apple's tool shims
if [[ -n "${DEVELOPER_DIR:-}" && ! -d "$DEVELOPER_DIR" ]]; then unset DEVELOPER_DIR SDKROOT; fi
export PATH="$HOME/.cargo/bin:$PATH"

which=${1:-all}
case "$which" in arm64|armhf|all) ;; *) echo "usage: $0 [arm64|armhf|all]" >&2; exit 2 ;; esac

host=$(rustc -vV | sed -n 's/^host: //p')
lld="$(rustc --print sysroot)/lib/rustlib/$host/bin/rust-lld"
[[ -x "$lld" ]] || { echo "no rust-lld at $lld" >&2; exit 1; }

mkdir -p target/pi-native
build() {
  local arch=$1 triple=$2
  local flags="-C linker=$lld -C linker-flavor=ld.lld -C target-feature=+crt-static -C strip=symbols"
  if [[ -n "${UFO_SYSROOT:-}" ]]; then
    flags="$flags --sysroot $UFO_SYSROOT"
  elif ! rustup target list --installed | grep -qx "$triple"; then
    rustup target add "$triple"
  fi
  # cargo's note that it drops ufo-demo's cdylib (the .wasm crate type) is left out: a static
  # target has no cdylibs, and ufo-native links the rlib
  local log=target/pi-native/.build-$arch.log
  if ! RUSTFLAGS="$flags" cargo build -q -p ufo-native --release --target "$triple" 2>"$log"; then
    cat "$log" >&2; rm -f "$log"; exit 1
  fi
  grep -v -e 'unsupported crate type .cdylib.' -e '^$' "$log" >&2 || true
  rm -f "$log"
  cp "target/$triple/release/ufo-native" "target/pi-native/ufo-native-$arch"
  echo "target/pi-native/ufo-native-$arch: $(( $(wc -c < "target/pi-native/ufo-native-$arch") / 1024 )) KiB"
}
[[ $which == armhf ]] || build arm64 aarch64-unknown-linux-musl
[[ $which == arm64 ]] || build armhf armv7-unknown-linux-musleabihf
# with what the Pi needs to try it: copy the folder over and see try-native.sh
install -m 644 native/pi/ufoscene-native.service target/pi-native/
install -m 755 native/pi/try-native.sh target/pi-native/
echo "To a Pi: scp -r $(pwd)/target/pi-native <user>@ufoscene.local:  then on the Pi: sudo ./pi-native/try-native.sh bench"
