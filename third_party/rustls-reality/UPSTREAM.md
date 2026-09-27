# Vendored `rustls-reality` — upstream sync

Skadi держит **path-зависимость** на форк `rustls` с хуками REALITY
в `third_party/rustls-reality/` (см. корневой `Cargo.toml`).

## Почему не git submodule / crates.io

- В дереве есть **Skadi-патчи** (`reality/`, ClientHello seal, HMAC verifier,
  hybrid PQ hooks), которых нет в stock `rustls`.
- Политика `deny.toml`: только `crates.io` + path; git-источники запрещены.

## Pin

Машиночитаемый pin: [`UPSTREAM.toml`](./UPSTREAM.toml).

| Поле | Смысл |
|------|--------|
| `rustls.version` / `tag` | Базовая версия upstream `rustls`, от которой взято дерево |
| `rustls.line` | Мажорная линия (`0.23`); следующий крупный шаг — `0.24+` / `1.x` |
| `skadi.preserve` | Пути, которые нужно перенести вручную при vendor refresh |

Текущая линия **0.23** (pin `v/0.23.45`). Skadi-патчи: `reality/` + хуки в
`client/` и `server/` (session_id seal, ServerHello inject, ED25519).
Workspace: `tokio-rustls` 0.26 с `ring` (не default `aws-lc-rs`).

## Автоматика

1. **Weekly Action** [`.github/workflows/sync-rustls-reality.yml`](../../.github/workflows/sync-rustls-reality.yml)
   — `scripts/check-rustls-reality-upstream.sh`: сравнивает pin с latest
   release upstream и открывает issue при drift / security advisory.
2. **Dependabot** [`.github/dependabot.yml`](../../.github/dependabot.yml) —
   `cargo` (Cargo.lock) и `github-actions`.
3. **Renovate** [`renovate.json`](../../renovate.json) — regex manager на
   `UPSTREAM.toml` (`datasource=github-releases`, `rustls/rustls`).

## Ручной rebase (чеклист)

1. Обновить `UPSTREAM.toml` (`version` / `tag`).
2. Скопировать upstream `rustls/` в `third_party/rustls-reality/rustls/`,
   **не затирая** пути из `skadi.preserve`.
3. Перенести серверные хуки (`reality_config`, `inject_auth`, session_id)
   в `server/` по `git log -- third_party/rustls-reality`.
4. `cargo test -p skadi-transport -p skadi-server --test reality_fallback_e2e
   --test reality_native_tls_e2e --test client_socks5_vless_reality_e2e`.
5. Запись в `CHANGELOG.md`.
