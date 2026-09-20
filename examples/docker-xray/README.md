# Examples × Xray-core (Docker e2e)

Интеграционная проверка, что **опубликованные** `examples/*/server.toml` +
`client-xray.json` совместимы с реальным Xray-core.

## Что гоняется

| Пример | Клиент |
|--------|--------|
| `examples/reality-vless/` | VLESS + REALITY |
| `examples/vless-tls/` | VLESS + TLS (`allowInsecure`) |
| `examples/reality-xhttp-vless/` | VLESS + REALITY + XHTTP |

Скрипт поднимает локальный TCP echo, патчит порты/`dest`/`allow_private`,
стартует `skadicore`, затем **Xray в Docker** (`teddysun/xray:25.12.8`) и гоняет
SOCKS5 CONNECT → echo.

## Запуск

Из корня репозитория (нужны Docker + Rust toolchain):

```bash
./scripts/examples-xray-docker-e2e.sh
```

Без Docker (локально / отладка) — хостовый Xray:

```bash
SKIP_DOCKER=1 ./scripts/examples-xray-docker-e2e.sh
# или: XRAY_BINARY=/path/to/xray SKIP_DOCKER=1 ./scripts/examples-xray-docker-e2e.sh
```

Готовый бинарник сервера:

```bash
SKADICORE_BIN=./target/release/skadicore ./scripts/examples-xray-docker-e2e.sh
```

## CI

Job **`examples-xray-e2e`** в `.github/workflows/ci.yml`.
