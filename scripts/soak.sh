#!/usr/bin/env bash
# Длительный soak-тест SkadiCore: нагрузка + снятие RSS / FD / latency.
#
# Режимы нагрузки (SOAK_LOAD):
#   native  — soak_load через VLESS (по умолчанию)
#   iperf3  — upstream iperf3 -s, soak_load через VLESS
#   wrk     — HTTP + SOCKS5 + proxychains wrk (требует proxychains4, wrk)
#
# Быстрый прогон (~2 мин):
#   ./scripts/soak.sh --quick
#
# Полный 24-часовой:
#   SOAK_DURATION=24h ./scripts/soak.sh
#
# Переменные окружения:
#   SOAK_DURATION       — 24h, 30m, 3600s (по умолчанию 24h)
#   SOAK_INTERVAL       — интервал снятия метрик, сек (60)
#   SOAK_LOAD           — native | iperf3 | wrk
#   SOAK_LOG_DIR        — каталог логов (/tmp/skadicore-soak-<ts>)
#   SOAK_CONCURRENCY    — параллельность soak_load (32)
#   MAX_RSS_GROWTH_MB   — порог роста RSS (256)
#   MAX_FD_GROWTH       — порог роста FD (500)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SOAK_DURATION="${SOAK_DURATION:-24h}"
SOAK_INTERVAL="${SOAK_INTERVAL:-60}"
SOAK_LOAD="${SOAK_LOAD:-native}"
SOAK_CONCURRENCY="${SOAK_CONCURRENCY:-32}"
MAX_RSS_GROWTH_MB="${MAX_RSS_GROWTH_MB:-256}"
MAX_FD_GROWTH="${MAX_FD_GROWTH:-500}"
QUICK=false

PROXY_PORT=10800
METRICS_PORT=19090
UPSTREAM_PORT=5201
HTTP_PORT=8080
TEST_UUID="b831381d-6324-4d53-ad4f-8cda48b30811"

SKADI_PID=""
UPSTREAM_PID=""
LOAD_PID=""
SAMPLER_PID=""
HTTP_PID=""

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
  echo ""
  echo "Options:"
  echo "  --quick          Короткий прогон (~2 мин, SOAK_LOAD=native)"
  echo "  --load MODE      native | iperf3 | wrk"
  echo "  --duration DUR   24h, 30m, 300s"
  echo "  --log-dir PATH   Каталог для логов и samples.csv"
  echo "  -h, --help"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick)
      QUICK=true
      SOAK_DURATION="120s"
      SOAK_INTERVAL=10
      shift
      ;;
    --load)
      SOAK_LOAD="$2"
      shift 2
      ;;
    --duration)
      SOAK_DURATION="$2"
      shift 2
      ;;
    --log-dir)
      SOAK_LOG_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

parse_duration() {
  local raw="$1"
  if [[ "$raw" =~ ^[0-9]+$ ]]; then
    echo "$raw"
    return
  fi
  if [[ "$raw" =~ ^([0-9]+)h$ ]]; then
    echo $(( BASH_REMATCH[1] * 3600 ))
    return
  fi
  if [[ "$raw" =~ ^([0-9]+)m$ ]]; then
    echo $(( BASH_REMATCH[1] * 60 ))
    return
  fi
  if [[ "$raw" =~ ^([0-9]+)s$ ]]; then
    echo "${BASH_REMATCH[1]}"
    return
  fi
  echo "Invalid SOAK_DURATION: $raw" >&2
  exit 1
}

SOAK_LOG_DIR="${SOAK_LOG_DIR:-/tmp/skadicore-soak-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$SOAK_LOG_DIR"

DURATION_SEC="$(parse_duration "$SOAK_DURATION")"
CONFIG_PATH="$SOAK_LOG_DIR/server.toml"
SAMPLES_CSV="$SOAK_LOG_DIR/samples.csv"
SKADI_LOG="$SOAK_LOG_DIR/skadicore.log"
LOAD_LOG="$SOAK_LOG_DIR/load.log"

cleanup() {
  local code=$?
  set +e
  [[ -n "$LOAD_PID" ]] && kill "$LOAD_PID" 2>/dev/null || true
  [[ -n "$SAMPLER_PID" ]] && kill "$SAMPLER_PID" 2>/dev/null || true
  [[ -n "$SKADI_PID" ]] && kill "$SKADI_PID" 2>/dev/null || true
  [[ -n "$UPSTREAM_PID" ]] && kill "$UPSTREAM_PID" 2>/dev/null || true
  [[ -n "$HTTP_PID" ]] && kill "$HTTP_PID" 2>/dev/null || true
  wait 2>/dev/null || true
  if [[ $code -ne 0 ]]; then
    echo "soak.sh failed (exit $code). Logs: $SOAK_LOG_DIR" >&2
  fi
  exit "$code"
}
trap cleanup EXIT INT TERM

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Required command not found: $1" >&2
    exit 1
  }
}

write_config() {
  local socks5_enabled="false"
  local vless_enabled="true"
  if [[ "$SOAK_LOAD" == "wrk" ]]; then
    socks5_enabled="true"
    vless_enabled="false"
  fi

  cat >"$CONFIG_PATH" <<EOF
[server]
listen = "127.0.0.1:${PROXY_PORT}"
max_connections = 10000
idle_timeout_secs = 300
max_session_lifetime_secs = 3600

[protocol.socks5]
enabled = ${socks5_enabled}

[protocol.vless]
enabled = ${vless_enabled}

[[protocol.vless.users]]
id = "${TEST_UUID}"
email = "soak@local"

[metrics]
enabled = true
listen = "127.0.0.1:${METRICS_PORT}"

[outbound]
allow_private = true
EOF
}

wait_for_port() {
  local host="$1"
  local port="$2"
  local retries="${3:-50}"
  for _ in $(seq 1 "$retries"); do
    if (echo >/dev/tcp/"$host"/"$port") 2>/dev/null; then
      return 0
    fi
    sleep 0.2
  done
  echo "Timeout waiting for $host:$port" >&2
  return 1
}

start_upstream() {
  case "$SOAK_LOAD" in
    iperf3)
      need_cmd iperf3
      iperf3 -s -p "$UPSTREAM_PORT" >"$SOAK_LOG_DIR/iperf3.log" 2>&1 &
      UPSTREAM_PID=$!
      ;;
    wrk)
      need_cmd python3
      python3 -m http.server "$HTTP_PORT" --bind 127.0.0.1 \
        >/dev/null 2>"$SOAK_LOG_DIR/http.log" &
      HTTP_PID=$!
      UPSTREAM_PORT=$HTTP_PORT
      ;;
    *)
      need_cmd socat
      socat TCP-LISTEN:"$UPSTREAM_PORT",fork,reuseaddr EXEC:'cat' \
        >/dev/null 2>"$SOAK_LOG_DIR/upstream.log" &
      UPSTREAM_PID=$!
      ;;
  esac
  wait_for_port 127.0.0.1 "$UPSTREAM_PORT"
}

start_skadicore() {
  echo "==> Building skadi-server (release)"
  cargo build --release -p skadi-server

  echo "==> Starting skadicore"
  RUST_LOG="${RUST_LOG:-info}" \
    "$ROOT/target/release/skadicore" --config "$CONFIG_PATH" \
    >"$SKADI_LOG" 2>&1 &
  SKADI_PID=$!
  wait_for_port 127.0.0.1 "$PROXY_PORT"
  wait_for_port 127.0.0.1 "$METRICS_PORT"
}

sample_once() {
  local ts rss fds active
  ts="$(date -Iseconds)"
  rss="$(awk '/^VmRSS:/{print $2}' /proc/"$SKADI_PID"/status 2>/dev/null || echo "")"
  fds="$(ls "/proc/$SKADI_PID/fd" 2>/dev/null | wc -l | tr -d ' ')"
  active="$(curl -sf "http://127.0.0.1:${METRICS_PORT}/metrics" 2>/dev/null \
    | awk '/^skadicore_active_connections /{print $2; exit}' || echo "")"
  echo "${ts},${rss},${fds},${active}" >>"$SAMPLES_CSV"
}

start_sampler() {
  echo "timestamp,rss_kb,fd_count,active_connections" >"$SAMPLES_CSV"
  (
    while kill -0 "$SKADI_PID" 2>/dev/null; do
      sample_once
      sleep "$SOAK_INTERVAL"
    done
  ) &
  SAMPLER_PID=$!
}

start_load() {
  echo "==> Load mode: $SOAK_LOAD (duration ${DURATION_SEC}s, concurrency $SOAK_CONCURRENCY)"
  case "$SOAK_LOAD" in
    native|iperf3)
      cargo build --release -p skadi-server --bin soak_load
      "$ROOT/target/release/soak_load" \
        --proxy "127.0.0.1:${PROXY_PORT}" \
        --uuid "$TEST_UUID" \
        --target-host 127.0.0.1 \
        --target-port "$UPSTREAM_PORT" \
        --concurrency "$SOAK_CONCURRENCY" \
        --payload-bytes 65536 \
        --duration-secs "$DURATION_SEC" \
        --report-interval-secs "$SOAK_INTERVAL" \
        >"$LOAD_LOG" 2>&1 &
      LOAD_PID=$!
      ;;
    wrk)
      need_cmd wrk
      need_cmd proxychains4
      cat >"$SOAK_LOG_DIR/proxychains.conf" <<EOF
strict_chain
proxy_dns
[ProxyList]
socks5 127.0.0.1 ${PROXY_PORT}
EOF
      (
        end=$((SECONDS + DURATION_SEC))
        while (( SECONDS < end )); do
          proxychains4 -f "$SOAK_LOG_DIR/proxychains.conf" -q \
            wrk -t2 -c"$SOAK_CONCURRENCY" -d"${SOAK_INTERVAL}s" \
            "http://127.0.0.1:${HTTP_PORT}/" \
            >>"$LOAD_LOG" 2>&1 || true
        done
      ) &
      LOAD_PID=$!
      ;;
    *)
      echo "Unknown SOAK_LOAD: $SOAK_LOAD" >&2
      exit 1
      ;;
  esac
}

check_thresholds() {
  [[ -f "$SAMPLES_CSV" ]] || return 0
  mapfile -t rows < <(tail -n +2 "$SAMPLES_CSV" | grep -v '^$' || true)
  [[ ${#rows[@]} -ge 2 ]] || {
    echo "WARN: not enough samples in $SAMPLES_CSV" >&2
    return 0
  }

  local first_rss last_rss first_fd last_fd max_active
  first_rss="$(echo "${rows[0]}" | cut -d, -f2)"
  last_rss="$(echo "${rows[-1]}" | cut -d, -f2)"
  first_fd="$(echo "${rows[0]}" | cut -d, -f3)"
  last_fd="$(echo "${rows[-1]}" | cut -d, -f3)"
  max_active="$(awk -F, 'NR>1 && $4!=""{print $4}' "$SAMPLES_CSV" | sort -n | tail -1)"

  if [[ -n "$first_rss" && -n "$last_rss" ]]; then
    local growth_mb=$(( (last_rss - first_rss) / 1024 ))
    echo "RSS: ${first_rss} KiB -> ${last_rss} KiB (delta ${growth_mb} MiB)"
    if (( growth_mb > MAX_RSS_GROWTH_MB )); then
      echo "FAIL: RSS growth ${growth_mb} MiB exceeds ${MAX_RSS_GROWTH_MB} MiB" >&2
      return 1
    fi
  fi

  if [[ -n "$first_fd" && -n "$last_fd" ]]; then
    local fd_growth=$((last_fd - first_fd))
    echo "FDs: ${first_fd} -> ${last_fd} (delta ${fd_growth})"
    if (( fd_growth > MAX_FD_GROWTH )); then
      echo "FAIL: FD growth ${fd_growth} exceeds ${MAX_FD_GROWTH}" >&2
      return 1
    fi
  fi

  if [[ -n "$max_active" ]]; then
    local slack=$((SOAK_CONCURRENCY + 16))
    echo "Max active_connections: ${max_active} (slack ${slack})"
    awk -v m="$max_active" -v s="$slack" 'BEGIN{exit !(m+0 > s+0)}' && {
      echo "FAIL: active_connections leak suspected (max $max_active)" >&2
      return 1
    }
  fi

  echo "Threshold checks passed."
}

echo "==> Soak test log dir: $SOAK_LOG_DIR"
write_config
start_upstream
start_skadicore
start_sampler
start_load

if [[ -n "$LOAD_PID" ]]; then
  wait "$LOAD_PID" || {
    echo "Load process failed. See $LOAD_LOG" >&2
    tail -30 "$LOAD_LOG" >&2 || true
    exit 1
  }
else
  sleep "$DURATION_SEC"
fi

check_thresholds
echo "Soak test completed successfully. Artifacts: $SOAK_LOG_DIR"
