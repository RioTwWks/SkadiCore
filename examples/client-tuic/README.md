# Клиент: SOCKS5 → TUIC (внешний binary)

См. [индекс примеров](../README.md).

`skadicore client` генерирует JSON и запускает **`tuic-client`**,
который слушает локальный SOCKS5 (`client.listen`).

## Зависимости

| Компонент | Назначение |
|-----------|------------|
| `tuic-client` | TUIC client (`TUIC_CLIENT_BINARY` если не в PATH) |

Сервер: [`../tuic/`](../tuic/) (`tuic-server`).

## Запуск

Терминал 1 (сервер, после `./examples/tuic/generate-certs.sh`):

```bash
cargo run --bin skadicore -- --config examples/tuic/server.toml
```

Терминал 2:

```bash
cargo run --bin skadicore -- client --config examples/client-tuic/client.toml
curl --socks5-hostname 127.0.0.1:10808 https://example.com
```

Для self-signed оставьте `allow_insecure = true`. Если в сертификате
другое CN, задайте `server = "cn.example:8443"` и `ip = "127.0.0.1"`.
