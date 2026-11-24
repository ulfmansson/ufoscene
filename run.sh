#!/usr/bin/env bash
# Build the demos to WebAssembly and serve them on http://127.0.0.1:8100
# (8100 is the port the Spotify redirect URI uses; override with PORT=<n> ./run.sh)
set -euo pipefail
cd "$(dirname "$0")"

# A stale Nix SDK path in the environment breaks the macOS linker for the server build.
if [[ -n "${DEVELOPER_DIR:-}" && ! -d "$DEVELOPER_DIR" ]]; then unset DEVELOPER_DIR SDKROOT; fi

if ! rustup target list --installed | grep -q '^wasm32-unknown-unknown$'; then
  rustup target add wasm32-unknown-unknown
fi

cargo build -p ufo-demo --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/ufo_demo.wasm web/ufo.wasm
echo "web/ufo.wasm: $(( $(wc -c < web/ufo.wasm) / 1024 )) KiB"

PORT="${PORT:-8100}" cargo run -q -p ufo-serve --release -- web
