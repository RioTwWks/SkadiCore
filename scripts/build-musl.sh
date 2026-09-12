#!/usr/bin/env bash
# Сборка статического skadicore под Linux/musl.
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${TARGET:-x86_64-unknown-linux-musl}"

case "$TARGET" in
  x86_64-unknown-linux-musl)
    if ! command -v musl-gcc &>/dev/null; then
      echo "Install musl-tools (Debian/Ubuntu: apt install musl-tools)" >&2
      exit 1
    fi
    ;;
  aarch64-unknown-linux-musl)
    if ! command -v aarch64-linux-musl-gcc &>/dev/null; then
      echo "Install aarch64-linux-musl-gcc (see https://musl.cc/ or docs/DEVELOPMENT.md)" >&2
      exit 1
    fi
    ;;
  *)
    echo "Unsupported TARGET: $TARGET" >&2
    exit 1
    ;;
esac

rustup target add "$TARGET"

echo "==> cargo build --release -p skadi-server --target $TARGET"
cargo build --release -p skadi-server --target "$TARGET"

BIN="target/$TARGET/release/skadicore"
file "$BIN"
ldd "$BIN" 2>&1 || true

echo "OK: $BIN"
