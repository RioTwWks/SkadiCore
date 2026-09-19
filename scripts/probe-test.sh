#!/usr/bin/env bash
# Сравнение TLS-зонда REALITY/SkadiCore с реальным dest (тайминги, опционально JA3).
#
# Использование:
#   ./scripts/probe-test.sh --dest www.example.com:443 --sni www.example.com
#   ./scripts/probe-test.sh --dest www.example.com:443 --sni www.example.com \
#       --skadi 127.0.0.1:443 --tolerance 10
#
# Переменные:
#   JA3_BIN   — путь к `ja3` (salesforce/ja3) для JA3-строки
#   PROBE_RUNS — число замеров handshake (по умолчанию 5)
#
# Требования: openssl, python3 (для медианы), опционально ja3.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DEST=""
SNI=""
SKADI=""
TOLERANCE_PCT=10
PROBE_RUNS="${PROBE_RUNS:-5}"

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dest) DEST="$2"; shift 2 ;;
    --sni) SNI="$2"; shift 2 ;;
    --skadi) SKADI="$2"; shift 2 ;;
    --tolerance) TOLERANCE_PCT="$2"; shift 2 ;;
    -h|--help) usage 0 ;;
    *) echo "unknown arg: $1" >&2; usage 1 ;;
  esac
done

if [[ -z "$DEST" || -z "$SNI" ]]; then
  echo "error: --dest and --sni are required" >&2
  usage 1
fi

if ! command -v openssl >/dev/null 2>&1; then
  echo "error: openssl not found" >&2
  exit 1
fi

handshake_ms() {
  local hostport="$1"
  local sni="$2"
  local start end
  start=$(date +%s%N)
  echo | timeout 15 openssl s_client -connect "$hostport" -servername "$sni" -brief 2>/dev/null \
    | head -n 5 >/dev/null || true
  end=$(date +%s%N)
  python3 - <<PY
start = $start
end = $end
print(f"{(end - start) / 1_000_000:.2f}")
PY
}

median_ms() {
  local hostport="$1"
  local sni="$2"
  local i
  local tmp
  tmp=$(mktemp)
  for ((i = 0; i < PROBE_RUNS; i++)); do
    handshake_ms "$hostport" "$sni" >>"$tmp"
  done
  python3 - <<PY
import statistics
from pathlib import Path
samples = [float(x) for x in Path("$tmp").read_text().split() if x.strip()]
print(f"{statistics.median(samples):.2f}")
PY
  rm -f "$tmp"
}

tls_summary() {
  local hostport="$1"
  local sni="$2"
  echo | timeout 15 openssl s_client -connect "$hostport" -servername "$sni" -brief 2>/dev/null \
    | grep -E '^(CONNECTION|Protocol|Cipher|Verification)' || true
}

ja3_probe() {
  local host="$1"
  local port="$2"
  if [[ -n "${JA3_BIN:-}" && -x "$JA3_BIN" ]]; then
    "$JA3_BIN" -host "$host" -port "$port" 2>/dev/null || true
  elif command -v ja3 >/dev/null 2>&1; then
    ja3 -host "$host" -port "$port" 2>/dev/null || true
  else
    echo "(ja3 binary not found; set JA3_BIN or install salesforce/ja3)"
  fi
}

compare_timing() {
  local dest_ms="$1"
  local skadi_ms="$2"
  python3 - <<PY
dest = float("$dest_ms")
skadi = float("$skadi_ms")
tol = float("$TOLERANCE_PCT") / 100.0
if dest <= 0:
    raise SystemExit("invalid dest timing")
ratio = skadi / dest
low = 1.0 - tol
high = 1.0 + tol
ok = low <= ratio <= high
print(f"dest_median_ms={dest:.2f} skadi_median_ms={skadi:.2f} ratio={ratio:.3f} tolerance=±{int($TOLERANCE_PCT)}%")
raise SystemExit(0 if ok else 2)
PY
}

echo "== TLS summary (dest: $DEST, SNI: $SNI) =="
tls_summary "$DEST" "$SNI"
echo

dest_host="${DEST%%:*}"
dest_port="${DEST##*:}"
echo "== JA3 (dest) =="
ja3_probe "$dest_host" "$dest_port"
echo

echo "== Handshake timing (median of $PROBE_RUNS runs) =="
dest_median=$(median_ms "$DEST" "$SNI")
echo "dest: ${dest_median} ms"

if [[ -n "$SKADI" ]]; then
  echo "== TLS summary (skadi: $SKADI) =="
  tls_summary "$SKADI" "$SNI"
  echo
  skadi_host="${SKADI%%:*}"
  skadi_port="${SKADI##*:}"
  echo "== JA3 (skadi) =="
  ja3_probe "$skadi_host" "$skadi_port"
  echo
  skadi_median=$(median_ms "$SKADI" "$SNI")
  echo "skadi: ${skadi_median} ms"
  compare_timing "$dest_median" "$skadi_median"
else
  echo "(skip skadi compare; pass --skadi host:port to compare handshake timing)"
fi

echo "probe-test: OK"
