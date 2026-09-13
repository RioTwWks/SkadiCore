#!/usr/bin/env bash
# Упаковка skadicore в tar.gz (Unix) или zip (Windows) + SHA256 для релиза.
set -euo pipefail

VERSION="${1:?usage: package-release.sh <version> <target-triple>}"
TARGET="${2:?usage: package-release.sh <version> <target-triple>}"

cd "$(dirname "$0")/.."

DIST="dist"
rm -rf "$DIST"
mkdir -p "$DIST/staging"

if [[ "$TARGET" == *"windows"* ]]; then
  BIN="target/${TARGET}/release/skadicore.exe"
  STAGING_NAME="skadicore.exe"
  ARCHIVE="${DIST}/skadicore-${VERSION}-${TARGET}.zip"
else
  BIN="target/${TARGET}/release/skadicore"
  STAGING_NAME="skadicore"
  ARCHIVE="${DIST}/skadicore-${VERSION}-${TARGET}.tar.gz"
fi

if [ ! -f "$BIN" ]; then
  echo "Binary not found: $BIN (build first)" >&2
  exit 1
fi

cp "$BIN" "$DIST/staging/$STAGING_NAME"
chmod +x "$DIST/staging/$STAGING_NAME"

if [[ "$ARCHIVE" == *.zip ]]; then
  (cd "$DIST/staging" && zip -q "../$(basename "$ARCHIVE")" "$STAGING_NAME")
else
  tar -czf "$ARCHIVE" -C "$DIST/staging" "$STAGING_NAME"
fi
rm -rf "$DIST/staging"

(
  cd "$DIST"
  sha256sum "$(basename "$ARCHIVE")" > SHA256SUMS
)

echo "OK: $ARCHIVE"
cat "$DIST/SHA256SUMS"
