# skadicore client — VLESS + REALITY

Нативный клиент (`skadicore client`) без Xray-core: REALITY TLS через vendored rustls.

## Быстрый старт

1. На сервере: `examples/reality-vless/server.toml` (или свой конфиг с `[transport.reality]`).
2. `skadicore genkey reality` — скопировать `password` и `short_ids` в `client.toml`.
3. Запуск:

```bash
skadicore client --config examples/client-reality-vless/client.toml
```

4. Проверка (важно: **socks5h**, чтобы DNS шёл через прокси):

```bash
curl -v --socks5-hostname 127.0.0.1:10808 https://example.com/
```

## Поля

| Поле | Назначение |
|------|------------|
| `remote.server` | Адрес прокси (TCP) |
| `remote.reality.server_name` | SNI для REALITY (не путать с `remote.server`) |
| `remote.reality.password` | Публичный X25519 ключ сервера (base64) |
| `remote.reality.short_id` | Hex short id (1–8 байт) |

См. `docs/CONFIGURATION.md` и e2e `crates/skadi-server/tests/client_socks5_vless_reality_e2e.rs`.
