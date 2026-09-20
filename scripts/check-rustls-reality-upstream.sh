#!/usr/bin/env bash
# Сравнить pin third_party/rustls-reality/UPSTREAM.toml с upstream rustls.
# Exit 0 — актуально (или только informational drift на другой линии).
# Exit 10 — найден более новый релиз / security advisory (для CI → issue).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN_FILE="${ROOT}/third_party/rustls-reality/UPSTREAM.toml"

if [[ ! -f "${PIN_FILE}" ]]; then
  echo "error: missing ${PIN_FILE}" >&2
  exit 2
fi

pin_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "${PIN_FILE}" | head -1)"
pin_tag="$(sed -n 's/^tag = "\([^"]*\)"/\1/p' "${PIN_FILE}" | head -1)"
pin_line="$(sed -n 's/^line = "\([^"]*\)"/\1/p' "${PIN_FILE}" | head -1)"

if [[ -z "${pin_version}" || -z "${pin_tag}" ]]; then
  echo "error: could not parse version/tag from ${PIN_FILE}" >&2
  exit 2
fi

echo "pinned: version=${pin_version} tag=${pin_tag} line=${pin_line}"

need_gh=0
if ! command -v gh >/dev/null 2>&1; then
  need_gh=1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required" >&2
  exit 2
fi

fetch_releases() {
  if [[ "${need_gh}" -eq 0 ]]; then
    gh api "repos/rustls/rustls/releases?per_page=30" \
      --jq '[.[] | select(.draft==false and .prerelease==false) | {tag: .tag_name, name: .name, url: .html_url, body: .body}]'
  else
    curl -fsSL "https://api.github.com/repos/rustls/rustls/releases?per_page=30" \
      | jq '[.[] | select(.draft==false and .prerelease==false) | {tag: .tag_name, name: .name, url: .html_url, body: .body}]'
  fi
}

releases_json="$(fetch_releases)"
latest_tag="$(echo "${releases_json}" | jq -r '.[0].tag // empty')"
latest_url="$(echo "${releases_json}" | jq -r '.[0].url // empty')"

if [[ -z "${latest_tag}" ]]; then
  echo "error: failed to fetch rustls releases" >&2
  exit 2
fi

echo "latest upstream release: ${latest_tag} (${latest_url})"

# Теги rustls: v/0.23.45 → 0.23.45
normalize() {
  local t="$1"
  t="${t#v/}"
  t="${t#v}"
  echo "${t}"
}

latest_ver="$(normalize "${latest_tag}")"
pin_ver_norm="$(normalize "${pin_tag}")"

same_line_newer="$(
  echo "${releases_json}" | jq -r --arg line "${pin_line}" --arg pin "${pin_ver_norm}" '
    def ver: split(".") | map(tonumber? // 0);
    .[]
    | (.tag | sub("^v/"; "") | sub("^v"; "")) as $v
    | select($v | startswith($line + "."))
    | select(($v | ver) > ($pin | ver))
    | "\(.tag)\t\(.url)"
  '
)"

security_hits="$(
  echo "${releases_json}" | jq -r '
    .[]
    | select((.body // "") | test("GHSA-|security advisory|CVE-"; "i"))
    | "\(.tag)\t\(.url)"
  ' | head -5
)"

drift=0
report=()

if [[ "${latest_ver}" != "${pin_ver_norm}" ]]; then
  drift=1
  report+=("Upstream latest is **${latest_tag}**, vendored pin is **${pin_tag}** (${pin_version}).")
fi

if [[ -n "${same_line_newer}" ]]; then
  drift=1
  report+=("Newer releases on line ${pin_line}:")
  while IFS=$'\t' read -r t u; do
    [[ -z "${t}" ]] && continue
    report+=("- ${t}: ${u}")
  done <<< "${same_line_newer}"
fi

if [[ -n "${security_hits}" ]]; then
  # Advisory в новых релизах — всегда поднимаем флаг (даже если линия другая).
  drift=1
  report+=("Recent upstream releases mentioning security advisories:")
  while IFS=$'\t' read -r t u; do
    [[ -z "${t}" ]] && continue
    report+=("- ${t}: ${u}")
  done <<< "${security_hits}"
fi

cargo_version="$(
  sed -n 's/^version = "\([^"]*\)"/\1/p' \
    "${ROOT}/third_party/rustls-reality/rustls/Cargo.toml" | head -1
)"
if [[ -n "${cargo_version}" && "${cargo_version}" != "${pin_version}" ]]; then
  drift=1
  report+=("Mismatch: UPSTREAM.toml version=${pin_version} but rustls/Cargo.toml version=${cargo_version}.")
fi

mkdir -p "${ROOT}/target"
summary_file="${ROOT}/target/rustls-reality-upstream-report.md"
{
  echo "## rustls-reality upstream check"
  echo
  echo "- Pin: \`${pin_tag}\` (version ${pin_version}, line ${pin_line})"
  echo "- Latest rustls release: \`${latest_tag}\`"
  echo "- Tree: \`third_party/rustls-reality/\`"
  echo
  if [[ "${drift}" -eq 0 ]]; then
    echo "Status: **up to date** relative to monitoring rules (no newer same-line release; no recent advisory mentions in release notes sampled)."
  else
    echo "Status: **action recommended**"
    echo
    for line in "${report[@]}"; do
      echo "${line}"
    done
    echo
    echo "### Next steps"
    echo
    echo "1. Read \`third_party/rustls-reality/UPSTREAM.md\` rebase checklist."
    echo "2. Update \`UPSTREAM.toml\` and re-vendor carefully (preserve \`skadi.preserve\`)."
    echo "3. Run REALITY e2e tests listed in UPSTREAM.md."
  fi
} > "${summary_file}"

cat "${summary_file}"

if [[ "${drift}" -ne 0 ]]; then
  exit 10
fi
exit 0
