# VLESS + REALITY + XHTTP (stream-one)

См. [индекс примеров](../README.md).

Комбинация **REALITY** (TLS-маскировка) и **XHTTP** (HTTP-транспорт поверх TLS).
Рекомендуется при деградации чистого TCP: трафик выглядит как обычный HTTPS
с POST/стримингом, а не как длинный TLS-поток.

## Стек

```
Клиент (Xray) → REALITY TLS → XHTTP upgrade → VLESS → upstream
```

- `flow` / `xtls-rprx-vision` **не** используйте — SkadiCore не поддерживает.
- Для REALITY+XHTTP Xray обычно выбирает `stream-one` (один HTTP POST на сессию).

## 1. Ключи

```bash
cargo run --bin skadicore -- genkey reality
```

| Сервер | Клиент |
|--------|--------|
| `private_key` | `password` (RawURL base64 из `genkey`) |
| `server_names` | `serverName` |
| `short_ids` | `shortId` |
| `[transport.xhttp].path` | `xhttpSettings.path` (должны совпадать) |

## 2. Запуск

```bash
cargo run --bin skadicore -- --config examples/reality-xhttp-vless/server.toml
xray run -c examples/reality-xhttp-vless/client-xray.json
curl --socks5-hostname 127.0.0.1:10808 https://example.com
```

Локально без root: смените `listen` на `127.0.0.1:8443` и `port` в клиенте.

## 3. Share link (v2rayNG)

```
vless://b831381d-6324-4d53-ad4f-8cda48b30811@127.0.0.1:443?encryption=none&security=reality&sni=www.microsoft.com&fp=chrome&pbk=EyxEK-AQ-9V-cmAzKKp25x_MwVA6riGTJ9FNnJmT9HI&sid=0123456789abcdef&spx=%2F&type=xhttp&path=%2Fxhttp&mode=stream-one#SkadiCore-REALITY-XHTTP
```

## 4. Автотест

```bash
cargo test -p skadi-server --test reality_xhttp_vless_e2e
```
