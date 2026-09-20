#!/usr/bin/env bash
# Скачать pinned Hysteria2 / TUIC бинарники для traffic e2e.
#
#   ./scripts/install-sidecar-bins.sh
#   DEST=/tmp/sidecar-bins ./scripts/install-sidecar-bins.sh
#
# Версии: hysteria app/v2.10.0, Itsusinn tuic v1.7.2 (linux amd64/musl).

set -euo pipefail

DEST="${DEST:-${1:-/tmp/sidecar-bins}}"
TUIC_VERSION="${TUIC_VERSION:-v1.7.2}"

mkdir -p "$DEST"

hy2_url="https://github.com/apernet/hysteria/releases/download/app%2Fv2.10.0/hysteria-linux-amd64"
tuic_server_url="https://github.com/Itsusinn/tuic/releases/download/${TUIC_VERSION}/tuic-server-x86_64-linux-musl"
tuic_client_url="https://github.com/Itsusinn/tuic/releases/download/${TUIC_VERSION}/tuic-client-x86_64-linux-musl"

download() {
  local url="$1"
  local out="$2"
  if [[ -x "$out" ]]; then
    echo "OK (cached) $out"
    return 0
  fi
  echo "download $url -> $out"
  curl -fsSL -o "$out" "$url"
  chmod +x "$out"
}

download "$hy2_url" "$DEST/hysteria"
download "$tuic_server_url" "$DEST/tuic-server"
download "$tuic_client_url" "$DEST/tuic-client"

echo "HYSTERIA2_BINARY=$DEST/hysteria"
echo "TUIC_SERVER_BINARY=$DEST/tuic-server"
echo "TUIC_CLIENT_BINARY=$DEST/tuic-client"
echo "install-sidecar-bins: OK ($DEST)"
