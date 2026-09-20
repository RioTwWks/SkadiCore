#!/usr/bin/env bash
# Скачать/собрать pinned sidecar-бинарники для traffic e2e.
#
#   ./scripts/install-sidecar-bins.sh
#   DEST=/tmp/sidecar-bins ./scripts/install-sidecar-bins.sh
#
# Версии:
#   hysteria app/v2.10.0
#   Itsusinn tuic v1.7.2 (linux amd64/musl)
#   amneziawg-tools v3.1.20260812 (ubuntu zip)
#   amneziawg-go v3.1.20260828 (go install; нужен Go ≥ 1.25)

set -euo pipefail

DEST="${DEST:-${1:-/tmp/sidecar-bins}}"
TUIC_VERSION="${TUIC_VERSION:-v1.7.2}"
AWG_TOOLS_VERSION="${AWG_TOOLS_VERSION:-v3.1.20260812}"
AWG_GO_VERSION="${AWG_GO_VERSION:-v3.1.20260828}"
INSTALL_AWG="${INSTALL_AWG:-1}"

mkdir -p "$DEST"

hy2_url="https://github.com/apernet/hysteria/releases/download/app%2Fv2.10.0/hysteria-linux-amd64"
tuic_server_url="https://github.com/Itsusinn/tuic/releases/download/${TUIC_VERSION}/tuic-server-x86_64-linux-musl"
tuic_client_url="https://github.com/Itsusinn/tuic/releases/download/${TUIC_VERSION}/tuic-client-x86_64-linux-musl"
awg_tools_url="https://github.com/amnezia-vpn/amneziawg-tools/releases/download/${AWG_TOOLS_VERSION}/ubuntu-22.04-amneziawg-tools.zip"

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

if [[ "$INSTALL_AWG" == "1" ]]; then
  if [[ ! -x "$DEST/awg" ]]; then
    tmp="$(mktemp -d)"
    echo "download $awg_tools_url"
    curl -fsSL -o "$tmp/awg-tools.zip" "$awg_tools_url"
    unzip -qo "$tmp/awg-tools.zip" -d "$tmp"
    install -m 0755 "$tmp"/ubuntu-22.04-amneziawg-tools/awg "$DEST/awg"
    rm -rf "$tmp"
  else
    echo "OK (cached) $DEST/awg"
  fi

  if [[ ! -x "$DEST/amneziawg-go" ]]; then
    if ! command -v go >/dev/null 2>&1; then
      echo "ERROR: go required to build amneziawg-go (INSTALL_AWG=1)" >&2
      exit 1
    fi
    echo "go install amneziawg-go@${AWG_GO_VERSION}"
    GOBIN="$DEST" go install "github.com/amnezia-vpn/amneziawg-go/v3@${AWG_GO_VERSION}"
  else
    echo "OK (cached) $DEST/amneziawg-go"
  fi
fi

echo "HYSTERIA2_BINARY=$DEST/hysteria"
echo "TUIC_SERVER_BINARY=$DEST/tuic-server"
echo "TUIC_CLIENT_BINARY=$DEST/tuic-client"
if [[ "$INSTALL_AWG" == "1" ]]; then
  echo "AWG_GO_BINARY=$DEST/amneziawg-go"
  echo "AWG_TOOLS_BINARY=$DEST/awg"
fi
echo "install-sidecar-bins: OK ($DEST)"
