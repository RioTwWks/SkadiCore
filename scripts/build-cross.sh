#!/usr/bin/env bash
# Кросс-сборка skadicore под Linux/musl, Windows (GNU) и macOS.
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${TARGET:?set TARGET (e.g. x86_64-pc-windows-gnu)}"

case "$TARGET" in
  x86_64-unknown-linux-musl | aarch64-unknown-linux-musl)
    exec env TARGET="$TARGET" ./scripts/build-musl.sh
    ;;
  x86_64-pc-windows-gnu)
    if ! command -v x86_64-w64-mingw32-gcc &>/dev/null; then
      echo "Install gcc-mingw-w64-x86-64 (Debian/Ubuntu: apt install gcc-mingw-w64-x86-64)" >&2
      exit 1
    fi
    rustup target add "$TARGET"
    echo "==> cargo build --release -p skadi-server --target $TARGET"
    cargo build --release -p skadi-server --target "$TARGET"
    BIN="target/$TARGET/release/skadicore.exe"
    file "$BIN"
    ;;
  x86_64-apple-darwin | aarch64-apple-darwin)
    rustup target add "$TARGET"
    echo "==> cargo build --release -p skadi-server --target $TARGET"
    cargo build --release -p skadi-server --target "$TARGET"
    BIN="target/$TARGET/release/skadicore"
    file "$BIN"
    ;;
  *)
    echo "Unsupported TARGET: $TARGET" >&2
    exit 1
    ;;
esac

echo "OK: $BIN"
