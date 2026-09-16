#!/usr/bin/env bash
# Подпись артефактов релиза minisign (пропуск, если секрет не задан).
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -z "${MINISIGN_SECRET_KEY:-}" ]; then
  echo "MINISIGN_SECRET_KEY not set — skipping release signing"
  exit 0
fi

if ! command -v minisign >/dev/null 2>&1; then
  echo "minisign not found in PATH" >&2
  exit 1
fi

KEY_FILE=$(mktemp)
chmod 600 "$KEY_FILE"
printf '%s\n' "$MINISIGN_SECRET_KEY" > "$KEY_FILE"
trap 'rm -f "$KEY_FILE"' EXIT

shopt -s nullglob
ARCHIVES=(dist/skadicore-*.tar.gz dist/skadicore-*.zip)
if [ "${#ARCHIVES[@]}" -eq 0 ]; then
  echo "No release archives in dist/" >&2
  exit 1
fi

for archive in "${ARCHIVES[@]}"; do
  minisign -S -s "$KEY_FILE" -m "$archive" -x "${archive}.minisig"
  echo "Signed: $archive"
done
