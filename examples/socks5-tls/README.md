# SOCKS5 over TLS

SOCKS5 с аутентификацией user-pass поверх TLS inbound. Удобно для
проверки TLS-слоя и как шаблон для корпоративного SOCKS.

> `curl --socks5` не умеет TLS-wrapped SOCKS5 напрямую. Для полного
> прокси-пути используйте клиент с TLS-туннелем или автотест ниже.

## 1. Сертификаты

```bash
chmod +x examples/socks5-tls/generate-certs.sh
./examples/socks5-tls/generate-certs.sh
```

## 2. Запуск сервера

```bash
cargo run --bin skadicore -- --config examples/socks5-tls/server.toml
```

## 3. Smoke test (TLS)

```bash
openssl s_client -connect 127.0.0.1:8443 -servername localhost
```

После установки TLS-сессии клиент должен отправить SOCKS5 greeting
(первый байт `0x05`). Ручная проверка через `openssl s_client` — только
для проверки сертификата и ALPN.

## 4. Автотест

```bash
cargo test -p skadi-server --test tls_socks5_e2e
```

## 5. Учётные данные

| Поле | Значение (пример) |
|------|-------------------|
| username | `alice` |
| password | `change-me-in-production` |

Смените пароль перед любым использованием вне localhost.
