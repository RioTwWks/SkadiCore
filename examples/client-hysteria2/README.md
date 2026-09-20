# Клиент: SOCKS5 → Hysteria2 (внешний binary)

См. [индекс примеров](../README.md).

`skadicore client` генерирует YAML и запускает **`hysteria client`**,
который слушает локальный SOCKS5 (`client.listen`).

## Зависимости

| Компонент | Назначение |
|-----------|------------|
| `hysteria` | Hysteria 2 client (`HYSTERIA2_BINARY` если не в PATH) |

Сервер: [`../hysteria2/`](../hysteria2/).

## Запуск

Терминал 1 (сервер, после `./examples/hysteria2/generate-certs.sh`):

```bash
cargo run --bin skadicore -- --config examples/hysteria2/server.toml
```

Терминал 2:

```bash
cargo run --bin skadicore -- client --config examples/client-hysteria2/client.toml
curl --socks5-hostname 127.0.0.1:10808 https://example.com
```

Для self-signed оставьте `insecure = true` или укажите `ca_file`.

## Share URI

Официальный формат: `hy2://password@host:443?sni=…&insecure=1`
