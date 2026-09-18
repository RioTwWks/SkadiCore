# Hysteria2

См. [индекс примеров](../README.md).

SkadiCore генерирует `config.yaml` и запускает внешний [`hysteria`](https://v2.hysteria.network) server binary.

## Зависимости

| Компонент | Назначение |
|-----------|------------|
| `hysteria` | Hysteria 2 server (`HYSTERIA2_BINARY` если не в PATH) |

## 1. Сертификаты

```bash
./examples/hysteria2/generate-certs.sh
```

## 2. Запуск

```bash
cargo run --bin skadicore -- --config examples/hysteria2/server.toml
```

## 3. Клиент

Используйте официальный Hysteria2 client или sing-box с тем же password и TLS.

Share link формат: `hy2://password@server:443?sni=example.com`

## 4. Проверка

```bash
cargo run --bin skadicore -- check-config --config examples/hysteria2/server.toml
```
