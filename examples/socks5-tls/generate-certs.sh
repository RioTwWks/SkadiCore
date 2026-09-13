#!/usr/bin/env bash
# Self-signed TLS для локальной проверки SOCKS5 over TLS.
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
CERT_DIR="$DIR/certs"
mkdir -p "$CERT_DIR"

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$CERT_DIR/server.key" \
  -out "$CERT_DIR/server.pem" \
  -days 365 \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

echo "Wrote $CERT_DIR/server.pem and $CERT_DIR/server.key"
