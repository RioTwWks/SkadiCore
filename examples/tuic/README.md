# TUIC

См. [индекс примеров](../README.md).

SkadiCore генерирует `config.toml` и запускает внешний [`tuic-server`](https://github.com/Itsusinn/tuic).

## Зависимости

| Компонент | Назначение |
|-----------|------------|
| `tuic-server` | TUIC QUIC proxy (`TUIC_SERVER_BINARY` если не в PATH) |

## 1. Сертификаты

```bash
./examples/tuic/generate-certs.sh
```

## 2. Запуск

```bash
cargo run --bin skadicore -- --config examples/tuic/server.toml
```

## 3. Клиент

Нативный sidecar: [`../client-tuic/`](../client-tuic/) (`skadicore client` + `tuic-client`).

Либо внешний tuic-client / sing-box / Nekoray с uuid/password из конфига.

## 4. Проверка

```bash
cargo run --bin skadicore -- check-config --config examples/tuic/server.toml
```
