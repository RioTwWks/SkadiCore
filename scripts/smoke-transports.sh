#!/usr/bin/env bash
# Smoke: примеры AWG / Hysteria2 / TUIC + опциональная проверка внешних бинарников.
#
#   ./scripts/smoke-transports.sh
#   SMOKE_START=1 ./scripts/smoke-transports.sh   # краткий старт процессов (если бинарники есть)
#   SMOKE_TRAFFIC=1 ./scripts/smoke-transports.sh # SOCKS5 echo e2e (нужны реальные бинарники)
#
# Переменные:
#   HYSTERIA_BIN / HYSTERIA2_BINARY, TUIC_SERVER_BIN / TUIC_*_BINARY
#   AWG_GO_BINARY, AWG_TOOLS_BINARY
#   SKADICORE — путь к бинарнику (по умолчанию cargo run --bin skadicore)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SMOKE_START="${SMOKE_START:-0}"
SMOKE_TRAFFIC="${SMOKE_TRAFFIC:-0}"
SKADICORE="${SKADICORE:-}"

check_config() {
  local cfg="$1"
  echo "check-config: $cfg"
  if [[ -n "$SKADICORE" ]]; then
    "$SKADICORE" check-config --config "$cfg"
  else
    cargo run --quiet --bin skadicore -- check-config --config "$cfg"
  fi
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

resolve_bin() {
  local env_name="$1"
  local default_name="$2"
  if [[ -n "${!env_name:-}" ]]; then
    echo "${!env_name}"
  elif have_cmd "$default_name"; then
    command -v "$default_name"
  else
    echo ""
  fi
}

echo "== Example configs =="
check_config examples/awg-vpn/server.toml
check_config examples/hysteria2/server.toml
check_config examples/tuic/server.toml

echo "== Optional binaries =="
# Managers/e2e предпочитают HYSTERIA2_BINARY / TUIC_*_BINARY; smoke допускает короткие алиасы.
hy2=$(resolve_bin HYSTERIA2_BINARY hysteria)
if [[ -z "$hy2" ]]; then hy2=$(resolve_bin HYSTERIA_BIN hysteria); fi
tuic=$(resolve_bin TUIC_SERVER_BINARY tuic-server)
if [[ -z "$tuic" ]]; then tuic=$(resolve_bin TUIC_SERVER_BIN tuic-server); fi
tuic_client=$(resolve_bin TUIC_CLIENT_BINARY tuic-client)
awg_go=$(resolve_bin AWG_GO_BINARY amneziawg-go)
awg_tools=$(resolve_bin AWG_TOOLS_BINARY awg)

for label in "hysteria:$hy2" "tuic-server:$tuic" "tuic-client:$tuic_client" "amneziawg-go:$awg_go" "awg:$awg_tools"; do
  name="${label%%:*}"
  path="${label#*:}"
  if [[ -z "$path" ]]; then
    echo "SKIP $name (not in PATH)"
  else
    echo "OK $name -> $path"
    case "$name" in
      hysteria) "$path" version 2>/dev/null | head -n1 || "$path" --version 2>/dev/null | head -n1 || true ;;
      tuic-server|tuic-client) "$path" --version 2>/dev/null | head -n1 || true ;;
      amneziawg-go) "$path" --version 2>/dev/null | head -n1 || true ;;
      awg) "$path" --version 2>/dev/null | head -n1 || "$path" help 2>/dev/null | head -n1 || true ;;
    esac
  fi
done

if [[ "$SMOKE_TRAFFIC" == "1" ]]; then
  echo "== Traffic e2e (real binaries) =="
  if [[ -z "$hy2" || -z "$tuic" || -z "$tuic_client" ]]; then
    echo "SMOKE_TRAFFIC=1 requires hysteria + tuic-server + tuic-client" >&2
    exit 1
  fi
  export HYSTERIA2_BINARY="${HYSTERIA2_BINARY:-$hy2}"
  export TUIC_SERVER_BINARY="${TUIC_SERVER_BINARY:-$tuic}"
  export TUIC_CLIENT_BINARY="${TUIC_CLIENT_BINARY:-$tuic_client}"
  cargo test -p skadi-server \
    --test hysteria2_traffic_e2e \
    --test tuic_traffic_e2e \
    -- --nocapture
fi

if [[ "$SMOKE_START" != "1" ]]; then
  if [[ "$SMOKE_TRAFFIC" == "1" ]]; then
    echo "smoke-transports: OK (traffic e2e)"
  else
    echo "smoke-transports: OK (set SMOKE_START=1 / SMOKE_TRAFFIC=1 for deeper checks)"
  fi
  exit 0
fi

echo "== Render-only smoke (cargo tests) =="
cargo test -p skadi-server --test hysteria2_config --test tuic_config --test awg_config -- --nocapture
cargo test -p skadi-transport --test awg_render 2>/dev/null || cargo test -p skadi-transport awg_render -- --nocapture

echo "smoke-transports: OK"
