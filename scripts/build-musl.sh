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
    if command -v cargo-zigbuild &>/dev/null; then
      USE_ZIGBUILD=1
    elif command -v aarch64-linux-musl-gcc &>/dev/null; then
      USE_ZIGBUILD=0
    else
      echo "Install cargo-zigbuild (pip install cargo-zigbuild) or aarch64-linux-musl-gcc" >&2
      echo "See docs/DEVELOPMENT.md" >&2
      exit 1
    fi
    ;;
  *)
    echo "Unsupported TARGET: $TARGET" >&2
    exit 1
    ;;
esac

rustup target add "$TARGET"

if [ "${USE_ZIGBUILD:-0}" = "1" ]; then
  echo "==> cargo zigbuild --release -p skadi-server --target $TARGET"
  cargo zigbuild --release -p skadi-server --target "$TARGET"
else
  echo "==> cargo build --release -p skadi-server --target $TARGET"
  cargo build --release -p skadi-server --target "$TARGET"
fi

BIN="target/$TARGET/release/skadicore"
file "$BIN"
ldd "$BIN" 2>&1 || true

echo "OK: $BIN"
