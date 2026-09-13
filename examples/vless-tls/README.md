# VLESS over TLS

Сервер SkadiCore с TLS inbound и VLESS. Подходит для отладки и как шаблон
для собственных сертификатов (Let's Encrypt, внутренний CA).

Для продакшена предпочтительнее [REALITY](../reality-vless/) — не нужен
публичный сертификат на вашем IP.

## 1. Сертификаты

Из корня репозитория:

```bash
chmod +x examples/vless-tls/generate-certs.sh
./examples/vless-tls/generate-certs.sh
```

Для продакшена замените `certs/server.pem` / `certs/server.key` на реальные PEM.

## 2. Запуск сервера

```bash
cargo run --bin skadicore -- --config examples/vless-tls/server.toml
```

Порт `8443` на loopback — без root. Для `443` измените `server.listen` и `port` в клиенте.

## 3. Smoke test (TLS handshake)

```bash
openssl s_client -connect 127.0.0.1:8443 -servername localhost
```

## 4. Клиент Xray-core

```bash
xray run -c examples/vless-tls/client-xray.json
curl --socks5 127.0.0.1:10808 https://example.com
```

`allowInsecure: true` — только для self-signed. В продакшене используйте
валидный сертификат и уберите этот флаг.

## 5. Nekoray / v2rayNG

- Протокол: VLESS
- Адрес: `127.0.0.1`, порт: `8443`
- UUID: `b831381d-6324-4d53-ad4f-8cda48b30811`
- TLS: включён, SNI `localhost`
- Для self-signed: разрешить insecure / skip cert verify

Не включайте `flow=xtls-rprx-vision` — SkadiCore отклоняет этот flow.

## 6. Автотест

```bash
cargo test -p skadi-server --test tls_vless_e2e
```
