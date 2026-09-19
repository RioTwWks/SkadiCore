# Клиент: SOCKS5 → VLESS + REALITY + XHTTP

См. [индекс примеров](../README.md).

Нативный **skadi-client** без Xray: локальный SOCKS5, outbound REALITY + XHTTP
(stream-one / stream-up / packet-up).

## Стек

```
curl --socks5 → skadicore client → REALITY TLS (ALPN http/1.1) → XHTTP → VLESS → skadicore server
```

Серверный пример: [`../reality-xhttp-vless/`](../reality-xhttp-vless/).

## 1. Ключи

```bash
cargo run --bin skadicore -- genkey reality
```

Подставьте `password` / `short_id` / `server_name` в `client.toml` и
`private_key` / `short_ids` / `server_names` в серверный TOML.

`[remote.xhttp].path` и `mode` должны совпадать с `[transport.xhttp]` на сервере.

## 2. Запуск

Терминал 1 (сервер):

```bash
cargo run --bin skadicore -- --config examples/reality-xhttp-vless/server.toml
```

Терминал 2 (клиент):

```bash
cargo run --bin skadicore -- client --config examples/client-reality-xhttp-vless/client.toml
curl --socks5-hostname 127.0.0.1:10808 https://example.com
```

Локально без root: на сервере `listen = "127.0.0.1:8443"`, в клиенте
`server = "127.0.0.1:8443"`.

## 3. Режимы XHTTP

| `mode` | Поведение |
|--------|-----------|
| `stream-one` (по умолчанию) | Один POST, chunked duplex |
| `stream-up` | GET downlink + POST uplink (два TCP) |
| `packet-up` | GET downlink + sequenced POST на каждый write |
| `auto` | На клиенте = `stream-one` |

## 4. Автотест

```bash
cargo test -p skadi-server --test client_socks5_vless_reality_xhttp_e2e
```
