# VLESS + REALITY — пример для Xray / Nekoray / v2rayNG

См. также [индекс примеров](../README.md) (VLESS TLS, SOCKS5 TLS).

Сервер SkadiCore с REALITY inbound и VLESS. Клиент — любой Xray-совместимый
клиент (Nekoray, v2rayNG, Xray-core).

## 1. Ключи

```bash
cargo run --bin skadicore -- genkey reality
```

Скопируйте `private_key` и `short_ids` в `server.toml`, `password` — в клиент.

| Сервер (TOML) | Клиент (JSON / GUI) |
|---------------|---------------------|
| `private_key` (standard base64, 32 байта) | `password` (RawURL base64 pubkey) |
| `server_names` | `serverName` |
| `short_ids` | `shortId` |
| `dest` | — (только сервер) |

**Важно:** в Xray 25+ поле клиента называется `password`, не `publicKey`.
Значение — **не** standard base64 публичного ключа, а вывод `skadicore genkey`
или `xray x25519 -i <privateKey>`.

Не включайте `flow=xtls-rprx-vision` — SkadiCore его не поддерживает.

## 2. Запуск сервера

```bash
cargo run --bin skadicore -- --config examples/reality-vless/server.toml
```

Для локального теста без root слушайте другой порт, например `127.0.0.1:8443`,
и поправьте `port` в клиенте.

## 3. Клиент Xray-core

```bash
xray run -c examples/reality-vless/client-xray.json
curl --socks5 127.0.0.1:10808 https://example.com
```

## 4. Nekoray

1. Импорт → из буфера / JSON → `client-xray.json`, или вручную:
   - Протокол: VLESS
   - Адрес / порт сервера
   - UUID: `b831381d-6324-4d53-ad4f-8cda48b30811`
   - TLS: REALITY
   - SNI: `www.microsoft.com`
   - Public Key / Password: `EyxEK-AQ-9V-cmAzKKp25x_MwVA6riGTJ9FNnJmT9HI`
   - Short ID: `0123456789abcdef`
   - Fingerprint: `chrome`

## 5. v2rayNG (share link)

```
vless://b831381d-6324-4d53-ad4f-8cda48b30811@127.0.0.1:443?encryption=none&security=reality&sni=www.microsoft.com&fp=chrome&pbk=EyxEK-AQ-9V-cmAzKKp25x_MwVA6riGTJ9FNnJmT9HI&sid=0123456789abcdef&spx=%2F&type=tcp#SkadiCore-REALITY
```

Замените адрес, UUID и ключи на свои. Параметр `pbk` — то же значение, что
`password` в JSON (RawURL base64 pubkey).

## 6. Автотест

E2E с реальным бинарником Xray:

```bash
cargo test -p skadi-server --test reality_vless_xray_e2e
```

При первом запуске скачивается Xray v25.12.8 (нужны `curl` и `unzip`), либо
укажите `XRAY_BINARY=/path/to/xray`.
