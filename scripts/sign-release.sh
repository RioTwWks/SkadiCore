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

sign_archive() {
  local archive="$1"
  local sig="${archive}.minisig"
  if [ -n "${MINISIGN_KEY_PASSPHRASE:-}" ]; then
    printf '%s\n' "$MINISIGN_KEY_PASSPHRASE" | minisign -S -s "$KEY_FILE" -m "$archive" -x "$sig"
  else
    if ! minisign -S -s "$KEY_FILE" -m "$archive" -x "$sig"; then
      echo "minisign signing failed." >&2
      echo "If the secret key is password-protected, set MINISIGN_KEY_PASSPHRASE (GitHub Actions secret)." >&2
      echo "For CI you can also regenerate without a password: minisign -G -W -p minisign.pub -s minisign.key" >&2
      exit 1
    fi
  fi
}

for archive in "${ARCHIVES[@]}"; do
  sign_archive "$archive"
  echo "Signed: $archive"
done
