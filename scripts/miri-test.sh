#!/usr/bin/env bash
# Miri smoke: чистые парсеры и sync unit-тесты без сети/TLS.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! rustup component list --installed | grep -q '^miri'; then
  echo "Installing Miri (nightly)..."
  rustup toolchain install nightly -c miri
fi

export MIRIFLAGS="${MIRIFLAGS:--Zmiri-disable-isolation}"

echo "==> cargo miri setup"
cargo +nightly miri setup

echo "==> skadi-protocol"
cargo +nightly miri test -p skadi-protocol

echo "==> skadi-server (connection_gate)"
cargo +nightly miri test -p skadi-server --lib connection_gate

echo "Miri OK"
