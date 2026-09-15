#!/usr/bin/env bash
# Проверка minisign-подписей артефактов релиза.
set -euo pipefail

PUBKEY="${1:?usage: verify-release.sh <path-to-minisign.pub> [dist-dir]}"
DIST="${2:-dist}"

if ! command -v minisign >/dev/null 2>&1; then
  echo "minisign not found in PATH" >&2
  exit 1
fi

if [ ! -f "$PUBKEY" ]; then
  echo "Public key not found: $PUBKEY" >&2
  exit 1
fi

shopt -s nullglob
SIGS=("$DIST"/*.minisig)
if [ "${#SIGS[@]}" -eq 0 ]; then
  echo "No .minisig files in $DIST/" >&2
  exit 1
fi

for sig in "${SIGS[@]}"; do
  archive="${sig%.minisig}"
  minisign -V -p "$PUBKEY" -m "$archive" -x "$sig"
  echo "OK: $archive"
done
