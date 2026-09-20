#!/usr/bin/env bash
# E2E: примеры VLESS (REALITY / TLS / REALITY+XHTTP) + Xray-core в Docker.
#
#   ./scripts/examples-xray-docker-e2e.sh
#
# Переменные:
#   SKADICORE_BIN   — путь к skadicore (иначе cargo build --release -p skadi-server)
#   XRAY_IMAGE      — образ Xray (по умолчанию teddysun/xray:latest)
#   XRAY_BINARY     — если Docker недоступен: хостовый xray (как в Rust e2e)
#   SKIP_DOCKER=1   — сразу fallback на XRAY_BINARY / авто-скачивание
#
# Требования: bash, python3, openssl, curl; Docker (CI) или XRAY_BINARY (локально).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

XRAY_IMAGE="${XRAY_IMAGE:-teddysun/xray:25.12.8}"
XRAY_VERSION="${XRAY_VERSION:-v25.12.8}"
XRAY_URL="${XRAY_URL:-https://github.com/XTLS/Xray-core/releases/download/${XRAY_VERSION}/Xray-linux-64.zip}"
SKIP_DOCKER="${SKIP_DOCKER:-0}"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/skadi-examples-xray.XXXXXX")"
HELPER_PIDS=()
SKADI_PID=""
XRAY_PID=""
XRAY_CID=""

cleanup() {
  local code=$?
  stop_xray || true
  if [[ -n "${SKADI_PID}" ]]; then
    kill "${SKADI_PID}" >/dev/null 2>&1 || true
    wait "${SKADI_PID}" 2>/dev/null || true
    SKADI_PID=""
  fi
  for pid in "${HELPER_PIDS[@]:-}"; do
    kill "${pid}" >/dev/null 2>&1 || true
  done
  rm -rf "${WORKDIR}"
  exit "${code}"
}
trap cleanup EXIT

log() { printf '==> %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

have() { command -v "$1" >/dev/null 2>&1; }

free_port() {
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

start_tcp_echo() {
  local port="$1"
  python3 - "$port" <<'PY' &
import socket, sys, threading
port = int(sys.argv[1])
srv = socket.socket()
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", port))
srv.listen(64)

def handle(c):
    try:
        while True:
            data = c.recv(65536)
            if not data:
                break
            c.sendall(data)
    finally:
        c.close()

while True:
    c, _ = srv.accept()
    threading.Thread(target=handle, args=(c,), daemon=True).start()
PY
  HELPER_PIDS+=($!)
}

start_tcp_sink() {
  local port="$1"
  python3 - "$port" <<'PY' &
import socket, sys
port = int(sys.argv[1])
srv = socket.socket()
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", port))
srv.listen(16)
while True:
    c, _ = srv.accept()
    c.close()
PY
  HELPER_PIDS+=($!)
}

socks5_echo_check() {
  local socks_port="$1" echo_port="$2" payload="$3"
  python3 - "$socks_port" "$echo_port" "$payload" <<'PY'
import socket, struct, sys
socks_port = int(sys.argv[1])
echo_port = int(sys.argv[2])
payload = sys.argv[3].encode()

s = socket.create_connection(("127.0.0.1", socks_port), timeout=5)
s.sendall(b"\x05\x01\x00")
assert s.recv(2) == b"\x05\x00", "socks auth"
req = b"\x05\x01\x00\x01" + socket.inet_aton("127.0.0.1") + struct.pack("!H", echo_port)
s.sendall(req)
resp = s.recv(10)
assert resp[0:2] == b"\x05\x00", f"socks connect failed: {resp!r}"
s.sendall(payload)
got = b""
while len(got) < len(payload):
    chunk = s.recv(len(payload) - len(got))
    assert chunk, "eof before echo"
    got += chunk
assert got == payload, (got, payload)
s.close()
print("ok", payload.decode())
PY
}

resolve_skadicore() {
  if [[ -n "${SKADICORE_BIN:-}" ]]; then
    [[ -x "${SKADICORE_BIN}" ]] || die "SKADICORE_BIN not executable: ${SKADICORE_BIN}"
    echo "${SKADICORE_BIN}"
    return
  fi
  log "building skadicore (release)"
  cargo build --release -p skadi-server --quiet
  echo "${ROOT}/target/release/skadicore"
}

resolve_xray_host() {
  if [[ -n "${XRAY_BINARY:-}" && -x "${XRAY_BINARY}" ]]; then
    echo "${XRAY_BINARY}"
    return
  fi
  local cache="${HOME}/.cache/skadicore/xray"
  local bin="${cache}/xray"
  if [[ -x "${bin}" ]]; then
    echo "${bin}"
    return
  fi
  have curl || die "curl required to download xray"
  have unzip || die "unzip required to download xray"
  mkdir -p "${cache}"
  log "downloading Xray ${XRAY_VERSION}"
  curl -fsSL -o "${cache}/xray.zip" "${XRAY_URL}"
  unzip -qo "${cache}/xray.zip" -d "${cache}"
  [[ -x "${bin}" ]] || die "xray binary missing after download"
  echo "${bin}"
}

docker_available() {
  [[ "${SKIP_DOCKER}" == "1" ]] && return 1
  have docker || return 1
  docker info >/dev/null 2>&1
}

start_skadi() {
  local cfg="$1"
  local skadi="$2"
  "${skadi}" --config "${cfg}" &
  SKADI_PID=$!
  sleep 0.6
  kill -0 "${SKADI_PID}" 2>/dev/null || die "skadicore exited immediately"
}

stop_skadi() {
  if [[ -n "${SKADI_PID}" ]]; then
    kill "${SKADI_PID}" >/dev/null 2>&1 || true
    wait "${SKADI_PID}" 2>/dev/null || true
    SKADI_PID=""
  fi
}

start_xray() {
  local config_path="$1"
  if docker_available; then
    log "starting Xray in Docker (${XRAY_IMAGE})"
    docker pull -q "${XRAY_IMAGE}" >/dev/null
    XRAY_CID="$(docker run -d --network host \
      -v "${config_path}:/etc/xray/config.json:ro" \
      "${XRAY_IMAGE}")"
    sleep 1.2
    docker inspect -f '{{.State.Running}}' "${XRAY_CID}" | grep -q true \
      || die "xray container not running; logs: $(docker logs "${XRAY_CID}" 2>&1 | tail -40)"
  else
    log "Docker unavailable — using host Xray binary"
    local xray
    xray="$(resolve_xray_host)"
    "${xray}" run -c "${config_path}" >/dev/null 2>&1 &
    XRAY_PID=$!
    sleep 1.2
    kill -0 "${XRAY_PID}" 2>/dev/null || die "host xray exited immediately"
  fi
}

stop_xray() {
  if [[ -n "${XRAY_CID}" ]]; then
    docker rm -f "${XRAY_CID}" >/dev/null 2>&1 || true
    XRAY_CID=""
  fi
  if [[ -n "${XRAY_PID}" ]]; then
    kill "${XRAY_PID}" >/dev/null 2>&1 || true
    wait "${XRAY_PID}" 2>/dev/null || true
    XRAY_PID=""
  fi
}

patch_reality_client() {
  local src="$1" dst="$2" proxy_port="$3" socks_port="$4"
  python3 - "${src}" "${dst}" "${proxy_port}" "${socks_port}" <<'PY'
import json, sys
src, dst, proxy_port, socks_port = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
cfg = json.load(open(src))
cfg["inbounds"][0]["port"] = socks_port
cfg["outbounds"][0]["settings"]["vnext"][0]["port"] = proxy_port
cfg["outbounds"][0]["streamSettings"]["realitySettings"]["serverName"] = "reality.test"
json.dump(cfg, open(dst, "w"), indent=2)
PY
}

patch_tls_client() {
  local src="$1" dst="$2" proxy_port="$3" socks_port="$4"
  python3 - "${src}" "${dst}" "${proxy_port}" "${socks_port}" <<'PY'
import json, sys
src, dst, proxy_port, socks_port = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
cfg = json.load(open(src))
cfg["inbounds"][0]["port"] = socks_port
cfg["outbounds"][0]["settings"]["vnext"][0]["port"] = proxy_port
json.dump(cfg, open(dst, "w"), indent=2)
PY
}

# ─── scenario: reality-vless ───────────────────────────────────────────
run_reality_vless() {
  local skadi="$1"
  local proxy_port echo_port dest_port socks_port
  proxy_port="$(free_port)"
  echo_port="$(free_port)"
  dest_port="$(free_port)"
  socks_port="$(free_port)"

  log "scenario reality-vless (proxy=${proxy_port} socks=${socks_port})"
  start_tcp_echo "${echo_port}"
  start_tcp_sink "${dest_port}"

  local server_cfg="${WORKDIR}/reality-vless-server.toml"
  local client_cfg="${WORKDIR}/reality-vless-client.json"

  sed \
    -e "s/^listen = .*/listen = \"127.0.0.1:${proxy_port}\"/" \
    -e "s|^dest = .*|dest = \"127.0.0.1:${dest_port}\"|" \
    -e "s/^server_names = .*/server_names = [\"reality.test\"]/" \
    "${ROOT}/examples/reality-vless/server.toml" > "${server_cfg}"
  cat >> "${server_cfg}" <<EOF

[outbound]
allow_private = true
EOF

  patch_reality_client \
    "${ROOT}/examples/reality-vless/client-xray.json" \
    "${client_cfg}" "${proxy_port}" "${socks_port}"

  start_skadi "${server_cfg}" "${skadi}"
  start_xray "${client_cfg}"
  socks5_echo_check "${socks_port}" "${echo_port}" "hello-reality-vless-example"
  stop_xray
  stop_skadi
}

# ─── scenario: vless-tls ───────────────────────────────────────────────
run_vless_tls() {
  local skadi="$1"
  local proxy_port echo_port socks_port
  proxy_port="$(free_port)"
  echo_port="$(free_port)"
  socks_port="$(free_port)"

  log "scenario vless-tls (proxy=${proxy_port} socks=${socks_port})"
  start_tcp_echo "${echo_port}"

  local cert_dir="${WORKDIR}/vless-tls-certs"
  mkdir -p "${cert_dir}"
  openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "${cert_dir}/server.key" \
    -out "${cert_dir}/server.pem" \
    -days 1 \
    -subj "/CN=localhost" \
    -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" >/dev/null 2>&1

  local server_cfg="${WORKDIR}/vless-tls-server.toml"
  local client_cfg="${WORKDIR}/vless-tls-client.json"

  sed \
    -e "s/^listen = .*/listen = \"127.0.0.1:${proxy_port}\"/" \
    -e "s|^cert = .*|cert = \"${cert_dir}/server.pem\"|" \
    -e "s|^key = .*|key = \"${cert_dir}/server.key\"|" \
    "${ROOT}/examples/vless-tls/server.toml" > "${server_cfg}"
  cat >> "${server_cfg}" <<EOF

[outbound]
allow_private = true
EOF

  patch_tls_client \
    "${ROOT}/examples/vless-tls/client-xray.json" \
    "${client_cfg}" "${proxy_port}" "${socks_port}"

  start_skadi "${server_cfg}" "${skadi}"
  start_xray "${client_cfg}"
  socks5_echo_check "${socks_port}" "${echo_port}" "hello-vless-tls-example"
  stop_xray
  stop_skadi
}

# ─── scenario: reality-xhttp-vless ─────────────────────────────────────
run_reality_xhttp() {
  local skadi="$1"
  local proxy_port echo_port dest_port socks_port
  proxy_port="$(free_port)"
  echo_port="$(free_port)"
  dest_port="$(free_port)"
  socks_port="$(free_port)"

  log "scenario reality-xhttp-vless (proxy=${proxy_port} socks=${socks_port})"
  start_tcp_echo "${echo_port}"
  start_tcp_sink "${dest_port}"

  local server_cfg="${WORKDIR}/reality-xhttp-server.toml"
  local client_cfg="${WORKDIR}/reality-xhttp-client.json"

  sed \
    -e "s/^listen = .*/listen = \"127.0.0.1:${proxy_port}\"/" \
    -e "s|^dest = .*|dest = \"127.0.0.1:${dest_port}\"|" \
    -e "s/^server_names = .*/server_names = [\"reality.test\"]/" \
    "${ROOT}/examples/reality-xhttp-vless/server.toml" > "${server_cfg}"
  cat >> "${server_cfg}" <<EOF

[outbound]
allow_private = true
EOF

  patch_reality_client \
    "${ROOT}/examples/reality-xhttp-vless/client-xray.json" \
    "${client_cfg}" "${proxy_port}" "${socks_port}"

  start_skadi "${server_cfg}" "${skadi}"
  start_xray "${client_cfg}"
  socks5_echo_check "${socks_port}" "${echo_port}" "hello-reality-xhttp-example"
  stop_xray
  stop_skadi
}

main() {
  have python3 || die "python3 required"
  have openssl || die "openssl required"
  local skadi
  skadi="$(resolve_skadicore)"

  if docker_available; then
    log "using Docker for Xray-core"
  else
    log "Docker not available — host Xray fallback (CI must have Docker)"
  fi

  run_reality_vless "${skadi}"
  run_vless_tls "${skadi}"
  run_reality_xhttp "${skadi}"
  log "all example × Xray scenarios passed"
}

main "$@"
