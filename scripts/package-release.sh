#!/usr/bin/env bash
# Упаковка skadicore в tar.gz + SHA256 для релиза.
set -euo pipefail

VERSION="${1:?usage: package-release.sh <version> <target-triple>}"
TARGET="${2:?usage: package-release.sh <version> <target-triple>}"

cd "$(dirname "$0")/.."

BIN="target/${TARGET}/release/skadicore"
if [ ! -f "$BIN" ]; then
  echo "Binary not found: $BIN (build first)" >&2
  exit 1
fi

DIST="dist"
ARCHIVE="${DIST}/skadicore-${VERSION}-${TARGET}.tar.gz"
rm -rf "$DIST"
mkdir -p "$DIST/staging"

cp "$BIN" "$DIST/staging/skadicore"
chmod +x "$DIST/staging/skadicore"

tar -czf "$ARCHIVE" -C "$DIST/staging" skadicore
rm -rf "$DIST/staging"

(
  cd "$DIST"
  sha256sum "$(basename "$ARCHIVE")" > SHA256SUMS
)

echo "OK: $ARCHIVE"
cat "$DIST/SHA256SUMS"
