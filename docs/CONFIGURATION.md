# Конфигурация SkadiCore

Документ описывает формат TOML-конфига: все поля, значения по
умолчанию, ограничения и типичные ошибки.

**Важно**: при старте конфиг читается с диска. Секцию `[protocol.*]` (пользователи
и флаги `enabled`) можно обновить без рестарта:

- `kill -HUP <pid>` — перечитать TOML и применить `[protocol.*]`
- gRPC API (`[api]`) — точечное добавление/удаление пользователей

`server.listen`, `[transport.tls]`, `[transport.reality]` и `[outbound.tls]`
при SIGHUP **не** меняются — для них нужен перезапуск процесса.

Проверка конфига без запуска: `skadicore check-config --config path/to.toml`.

---

## Содержание

- [Общая структура](#общая-структура)
- [Секция `[server]`](#секция-server)
- [Секция `[transport.tls]`](#секция-transporttls)
- [Секция `[outbound.tls]`](#секция-outboundtls)
- [Секция `[transport.reality]`](#секция-transportreality)
- [Секция `[protocol.socks5]`](#секция-protocolsocks5)
- [Секция `[protocol.vless]`](#секция-protocolvless)
- [Секция `[api]`](#секция-api)
- [Секция `[metrics]`](#секция-metrics)
- [Валидация](#валидация)
- [Примеры](#примеры)
- [Типичные ошибки](#типичные-ошибки)

---

## Общая структура

Конфиг — это TOML-файл. Путь задаётся флагом `--config`:

```bash
skadicore --config /etc/skadicore/config.toml
```

По умолчанию — `config/skadi.toml` в рабочей директории.

Верхнеуровневые секции:

```toml
[server]              # обязательно
[transport.tls]       # опционально (не вместе с reality)
[outbound.tls]        # опционально (TLS к upstream)
[transport.reality]   # опционально (рекомендуется для VLESS)
[protocol.socks5]     # опционально
[protocol.vless]      # опционально
[api]                 # опционально (gRPC hot reload пользователей)
[metrics]             # опционально (Prometheus + /healthz)
```

Хотя бы один протокол должен быть включён. Если все выключены —
ядро не запустится с ошибкой `no protocol enabled`.

---

## Секция `[server]`

### `listen`

**Тип**: `String` или массив `String`
**Обязательно**: да
**Формат**: `IP:PORT` (один адрес) или `["IP:PORT", ...]` (несколько сокетов)

Адрес(а) и порт для входящих TCP-соединений. Несколько значений полезны для
dual-stack (IPv4 + IPv6 на одном порту).

```toml
[server]
listen = "0.0.0.0:443"
max_connections = 1000
```

Dual-stack (рекомендуется при публичном `0.0.0.0`):

```toml
[server]
listen = ["0.0.0.0:443", "[::]:443"]
```

При старте, если указан только `0.0.0.0:PORT` без `[::]:PORT`, в лог пишется
предупреждение — IPv6-клиенты могут быть недоступны.

### `max_connections`

**Тип**: `u32`  
**По умолчанию**: не задано (без лимита)

Максимум одновременных inbound-сессий (от `accept` до закрытия relay/TLS/REALITY).
При превышении лимита новое TCP-соединение **сразу закрывается** без ожидания в очереди.

Метрика: `skadicore_connections_rejected_total`.

Изменение через SIGHUP **не** поддерживается — нужен рестарт.

`max_connections = 0` — ошибка валидации.

### `[server.auth_rate_limit]`

Per-IP ограничение неудачных аутентификаций (SOCKS5/VLESS).

| Поле | Тип | По умолчанию | Описание |
|------|-----|--------------|----------|
| `enabled` | bool | `true` | Включить rate limiting |
| `max_failures` | u32? | `10` | Неудачных попыток до бана (`0` — выключить) |
| `window_secs` | u64 | `600` | Окно подсчёта неудач (сек) |
| `ban_base_secs` | u64 | `60` | Базовая длительность бана (сек), растёт экспоненциально |
| `ban_max_secs` | u64 | `3600` | Максимальная длительность бана (сек) |

Метрики: `skadicore_auth_failures_total`, `skadicore_auth_blocked_total`.

**Допустимые значения**:

- `"0.0.0.0:443"` — все IPv4-интерфейсы.
- `"127.0.0.1:1080"` — только локально (для отладки).
- `"[::]:443"` — все IPv6-интерфейсы.
- `"[::1]:1080"` — IPv6 loopback.

**Ограничения**:

- Порт должен быть свободен. Если занят — ядро завершится с
  ошибкой `failed to bind`.
- Порты < 1024 требуют прав root на Linux. Используйте
  `setcap` или systemd-юнит с `CAP_NET_BIND_SERVICE`.

**Не поддерживается**:

- Unix-сокеты (пока).
- Диапазоны портов.
- Несколько адресов в одном поле.

### `[server.timeouts]` (опционально)

Таймауты TCP-прокси после установки сессии.

```toml
[server.timeouts]
connect_timeout_secs = 10
idle_timeout_secs = 300
```

| Поле | Тип | По умолчанию | Описание |
|------|-----|--------------|----------|
| `connect_timeout_secs` | `u64` | `10` | Таймаут исходящего TCP `connect` к целевому хосту |
| `idle_timeout_secs` | `u64` | не задано | Закрыть relay-сессию, если нет трафика в обе стороны |
| `max_session_lifetime_secs` | `u64` | не задано | Закрыть relay-сессию по абсолютному времени (даже при активном трафике) |

**Поведение `idle_timeout_secs`:**

- Считается от последнего переданного байта в любую сторону.
- При срабатывании сессия закрывается без ошибки в логах (`closed (idle timeout)`).
- Не применяется к фазе handshake (SOCKS5/VLESS/TLS) — только к relay.
- Изменение через SIGHUP **не** поддерживается (нужен рестарт).

`idle_timeout_secs = 0` или отрицательное значение — ошибка валидации.

**Поведение `max_session_lifetime_secs`:**

- Отсчёт начинается с начала relay (после handshake).
- Не сбрасывается при передаче данных (в отличие от idle).
- При срабатывании: `closed (max session lifetime)` в логах.

---

## Секция `[transport.tls]`

TLS-обёртка для **входящих** соединений. Рекомендуется для VLESS
в любых условиях, кроме локальной отладки.

### `enabled`

**Тип**: `bool`
**По умолчанию**: `false`

```toml
[transport.tls]
enabled = true
cert = "certs/server.pem"
key = "certs/server.key"
```

### `cert`

**Тип**: `String`
**Обязательно**: если `enabled = true`

Путь к PEM-файлу с цепочкой сертификатов сервера.

### `key`

**Тип**: `String`
**Обязательно**: если `enabled = true`

Путь к PEM-файлу с приватным ключом (PKCS#8, RSA или EC).

### `alpn`

**Тип**: массив строк
**По умолчанию**: `[]`

ALPN-протоколы для TLS handshake:

```toml
alpn = ["h2", "http/1.1"]
```

### `kex_mode`

**Тип**: `string`  
**По умолчанию**: `"classic"`

Режим key exchange для TLS 1.3:

| Значение | Описание |
|----------|----------|
| `classic` | X25519 / ECDHE (по умолчанию) |
| `hybrid_pq` | X25519MLKEM768 — гибрид X25519 + ML-KEM-768 ([RFC 10024](https://www.rfc-editor.org/rfc/rfc10024.html)) |

```toml
[transport.tls]
enabled = true
kex_mode = "hybrid_pq"
cert = "certs/server.pem"
key = "certs/server.key"
```

Клиент и сервер должны использовать один и тот же режим. **REALITY** остаётся
на классическом X25519 (несовместим с `hybrid_pq`).

### `[[transport.tls.certificates]]`

**Тип**: массив таблиц
**По умолчанию**: `[]`

SNI-роутинг: разные сертификаты для разных имён на одном порту.

```toml
[transport.tls]
enabled = true
cert = "certs/default.pem"    # fallback при неизвестном SNI
key = "certs/default.key"
alpn = ["h2", "http/1.1"]

[[transport.tls.certificates]]
server_names = ["example.com", "www.example.com"]
cert = "certs/example.pem"
key = "certs/example-key.pem"

[[transport.tls.certificates]]
server_names = ["cdn.example.net"]
cert = "certs/cdn.pem"
key = "certs/cdn-key.pem"
```

#### `server_names`

**Тип**: массив строк
**Обязательно**: да

DNS-имена (без IP). Сравнение без учёта регистра.
Дубликаты между записями запрещены.

#### `cert` / `key`

Пути к PEM для данной группы имён.

**Режимы работы**:

| Конфиг | Поведение |
|--------|-----------|
| Только `cert` + `key` | Один сертификат для всех (как раньше) |
| Только `[[certificates]]` | Только SNI-имена из таблицы; без SNI — отказ |
| Оба | SNI-таблица + `cert`/`key` как fallback |

**Ограничения**:

- Только TLS 1.3 (настроено в коде).
- Проверка клиентского сертификата отключена (`with_no_client_auth`).
- При `enabled = true` все PEM-файлы проверяются при старте.

**Порядок обработки**:

```
TCP accept → TLS handshake → протокол (SOCKS5/VLESS) → upstream TCP/TLS
```

---

## Секция `[outbound]`

### `allow_private`

**Тип**: `bool`  
**По умолчанию**: `false`

Запрещает relay к loopback, private, link-local и ULA адресам (SSRF-защита).
При `false` подключения к `127.0.0.1`, `10.x`, `192.168.x` и т.п. блокируются.

```toml
[outbound]
allow_private = false
```

## Секция `[outbound.tls]`

TLS-обёртка для **исходящих** TCP-соединений к upstream. По умолчанию
прокси подключается к целевому хосту по обычному TCP. При
`outbound.tls.enabled = true` после TCP connect выполняется TLS handshake
с проверкой сертификата сервера.

UDP outbound TLS не поддерживает — только TCP relay.

### `enabled`

**Тип**: `bool`  
**По умолчанию**: `false`

```toml
[outbound.tls]
enabled = true
```

### `ca_file`

**Тип**: `String`  
**По умолчанию**: не задан (системное хранилище CA через `rustls-native-certs`)

Путь к PEM с доверенными CA для проверки сертификата upstream.
Используйте для self-signed или корпоративных CA.

```toml
[outbound.tls]
enabled = true
ca_file = "/etc/skadicore/upstream-ca.pem"
```

### `cert` / `key`

**Тип**: `String`  
**По умолчанию**: не заданы

Опциональный клиентский сертификат (mTLS). Оба поля должны быть заданы
вместе.

### `kex_mode`

**Тип**: `string`  
**По умолчанию**: `"classic"`

`classic` или `hybrid_pq` (X25519MLKEM768). Должен совпадать с режимом upstream TLS.

```toml
[outbound.tls]
enabled = true
cert = "/etc/skadicore/client.pem"
key = "/etc/skadicore/client-key.pem"
```

### Поведение

| Цель VLESS/SOCKS5 | SNI при TLS outbound |
|-------------------|----------------------|
| Домен | Имя домена из запроса |
| IPv4/IPv6 | IP-адрес (SAN должен содержать IP) |

**Порядок обработки** (при `outbound.tls.enabled = true`):

```
протокол → TCP connect → TLS handshake (verify) → relay
```

**Ограничения**:

- Только TLS 1.3.
- При `enabled = true` PEM-файлы проверяются при старте (`check-config`).
- Hot reload через SIGHUP **не** меняет `outbound.tls` — нужен перезапуск.

---

## Секция `[transport.xhttp]`

XHTTP (SplitHTTP) — HTTP-транспорт поверх TLS, REALITY или plain TCP.
В текущей версии поддерживается **stream-one**, **stream-up**
(GET downlink + POST uplink: `/xhttp/{sessionId}`) и **packet-up**
(sequenced POST: `/xhttp/{sessionId}/{seq}` + GET downlink).

```toml
[transport.tls]
enabled = true
cert = "certs/default.pem"
key = "certs/default.key"

[transport.xhttp]
enabled = true
path = "/xhttp"
mode = "stream-one"   # auto | stream-one | stream-up | packet-up
# host = "example.com"   # опционально: проверка Host
# x_padding_bytes = [100, 1000]
```

| Поле | Тип | Описание |
|------|-----|----------|
| `enabled` | `bool` | Включить XHTTP upgrade после inbound TLS/REALITY |
| `path` | `string` | URL prefix (по умолчанию `/xhttp`) |
| `mode` | `string` | `auto`, `stream-one`, `stream-up`, `packet-up` (все три режима на сервере в `auto`) |
| `host` | `string?` | Ожидаемый HTTP Host |
| `no_sse_header` | `bool` | Не отправлять `Content-Type: text/event-stream` |
| `x_padding_bytes` | `[u32; 2]?` | Диапазон длины `X-Padding` в ответе |

Packet-up: каждый POST содержит полный payload с monotonic `seq` (начиная с 0);
сервер собирает пакеты в порядке seq перед передачей в VLESS/SOCKS5.

---

## Секция `[transport.reality]`

REALITY — TLS-маскировка с fallback на реальный сайт при невалидном
клиенте. Рекомендуется вместо обычного `[transport.tls]` для VLESS.

**Нельзя** включать `transport.tls` и `transport.reality` одновременно.

### Генерация ключей

```bash
skadicore genkey reality
```

### Поля

```toml
[transport.reality]
enabled = true
dest = "www.microsoft.com:443"
server_names = ["www.microsoft.com"]
private_key = "BASE64_X25519_32_BYTES"
short_ids = ["0123456789abcdef"]
```

| Поле | Тип | Описание |
|------|-----|----------|
| `enabled` | `bool` | Включить REALITY inbound |
| `dest` | `string` | Fallback `host:port` (реальный сайт) |
| `server_names` | `string[]` | Разрешённые SNI (Xray: `serverNames`) |
| `private_key` | `string` | X25519 private key, standard base64 (32 байта) |
| `short_ids` | `string[]` | Short ID в hex (1..8 байт каждый) |
| `impersonate_cert` | `string?` | Путь к PEM/DER leaf-сертификата `dest` для ImpersonateCert (rkn-fix) |
| `fetch_impersonate_cert` | `bool` | Получить leaf-сертификат с `dest` при старте (по умолчанию `true`) |
| `kex_mode` | `string` | `classic` (по умолчанию) или `hybrid_pq` (X25519MLKEM768, RFC 10024) |

### `kex_mode` (REALITY)

| Значение | Описание |
|----------|----------|
| `classic` | Классический X25519 (совместимость с Xray / v2rayNG) |
| `hybrid_pq` | Гибрид X25519 + ML-KEM-768 — защита от store-now-decrypt-later |

При `hybrid_pq` клиент **должен** поддерживать X25519MLKEM768 (стандартные Xray-клиенты
пока используют только `classic`). REALITY-аутентификация (X25519 short_id) не меняется —
PQ-KEX применяется только к TLS 1.3 key exchange после успешного verify.

```toml
[transport.reality]
kex_mode = "hybrid_pq"
```

### REALITY-rkn-fix (анти-DPI)

Каждое соединение получает свежий Ed25519-сертификат с реалистичными X.509
полями (случайный serial, CN = SNI клиента, валидность 30–89 дней назад +
1–2 года). Механизм HMAC-SHA512 REALITY не меняется.

При `fetch_impersonate_cert = true` (или `impersonate_cert = "/path/to/leaf.der"`)
метаданные сертификата клонируются с leaf-сертификата `dest` — пассивный DPI
не видит статический отпечаток `SerialNumber=0` / пустой Subject.

Если fetch не удался, сервер продолжает работу с randomized per-connection certs.

**Криптографические ограничения:** REALITY-аутентификация использует X25519.
Для защиты TLS-сессии от store-now-decrypt-later включите `kex_mode = "hybrid_pq"`
(на сервере и PQ-совместимом клиенте). См. `docs/RISKS.md` §1.5 и `SECURITY.md`.

### Клиент (Xray / Nekoray / v2rayNG)

| Поле клиента | Значение |
|--------------|----------|
| `password` | RawURL base64 публичного X25519 ключа — вывод `skadicore genkey reality` |
| `serverName` | Один из `server_names` |
| `shortId` | Один из `short_ids` (hex) |
| `fingerprint` | `chrome` (рекомендуется) |

В Xray 25+ поле называется `password` (раньше `publicKey`). Это **не**
standard base64 — используйте значение из `genkey`, не кодируйте pubkey вручную.

Не задавайте `flow=xtls-rprx-vision` — SkadiCore не поддерживает.

Пример: `examples/reality-vless/`.

**Порядок обработки**:

```
TCP accept → sniff ClientHello → REALITY verify?
  ├─ да  → per-connection Ed25519 cert (rkn-fix) → TLS 1.3 → VLESS/SOCKS5 → upstream
  └─ нет → transparent proxy к dest (зонд видит реальный сайт)
```

---

## Секция `[protocol.socks5]`

### `enabled`

**Тип**: `bool`
**По умолчанию**: `false`

Включает SOCKS5-обработчик.

```toml
[protocol.socks5]
enabled = true
```

### `auth`

**Тип**: `String`
**По умолчанию**: `"no-auth"`
**Допустимые значения**: `"no-auth"`, `"user-pass"`

Метод аутентификации.

- `"no-auth"` — любой клиент может подключиться без пароля.
  **Не используйте на публичном IP**.
- `"user-pass"` — требуется логин и пароль из списка `users`.

### `users`

**Тип**: массив таблиц
**Обязательно**: если `auth = "user-pass"`

Список пользователей.

```toml
[[protocol.socks5.users]]
username = "alice"
password = "change-me"

[[protocol.socks5.users]]
username = "bob"
password = "another-secret"
```

**Ограничения**:

- `username` и `password` — валидный UTF-8, непустые.
- Максимальная длина — 255 байт каждое (ограничение RFC 1929).
- Дубликаты `username` не проверяются на этапе валидации.
  Если два пользователя с одинаковым логином — победит первый
  в списке. Это известная недоработка, будет исправлено.

**Безопасность**:

- Пароли хранятся в открытом виде. Это осознанное решение:
  SOCKS5 передаёт пароль открытым текстом (если не обёрнут в
  TLS). Хеширование не даёт выигрыша, потому что атакующий
  видит пароль в трафике.
- При `auth = "no-auth"` секция `users` игнорируется.

---

## Секция `[protocol.vless]`

### `enabled`

**Тип**: `bool`
**По умолчанию**: `false`

Включает VLESS-обработчик.

```toml
[protocol.vless]
enabled = true
```

### `users`

**Тип**: массив таблиц
**Обязательно**: если `enabled = true`

Список пользователей VLESS.

```toml
[[protocol.vless.users]]
id = "b831381d-6324-4d53-ad4f-8cda48b30811"
email = "alice@example.com"

[[protocol.vless.users]]
id = "9c3f7b12-4567-4d89-9abc-def012345678"
email = "bob@example.com"
```

### `users[].id`

**Тип**: `String`
**Обязательно**: да
**Формат**: UUID версии 4 (канонический)

Идентификатор пользователя. Используется для аутентификации.

**Допустимые форматы**:

- С дефисами: `"b831381d-6324-4d53-ad4f-8cda48b30811"`.
- Без дефисов: `"b831381d63244d53ad4f8cda48b30811"`.
- В верхнем регистре: `"B831381D-6324-4D53-AD4F-8CDA48B30811"`.

Все три формы эквивалентны. Регистр не важен.

**Ограничения**:

- Ровно 32 hex-цифры (без учёта дефисов).
- Только символы `0-9`, `a-f`, `A-F`.
- Дубликаты `id` не проверяются. Победит первый в списке.

**Генерация UUID**:

```bash
# Linux/macOS
uuidgen

# Или через Python
python3 -c "import uuid; print(uuid.uuid4())"

# Или через openssl
openssl rand -hex 16 | sed 's/\(..\)/\1/g' | \
  sed 's/^\(........\)\(....\)\(....\)\(....\)\(............\)$/\1-\2-\3-\4-\5/'
```

### `users[].email`

**Тип**: `String`
**Обязательно**: нет

Метка пользователя. Используется только в логах и метриках.
Не влияет на аутентификацию.

```toml
email = "alice@example.com"
```

Если не указано — в логах будет пустая строка.

### `users[].flow`

**Тип**: `String`
**Обязательно**: нет

Flow-режим XTLS. **Не поддерживается в текущей версии.**

Поле читается, но игнорируется. Если клиент требует
`xtls-rprx-vision`, соединение не установится.

Зарезервировано для будущей реализации.

---

## Секция `[api]`

gRPC management API для hot reload пользователей без перезапуска сервера.

```toml
[api]
enabled = true
listen = "127.0.0.1:10085"
token = "change-me-to-a-long-random-secret"
rate_limit_per_sec = 30   # опционально; 0 или отсутствие = без лимита

[api.tls]
enabled = true
cert = "certs/api.pem"
key = "certs/api-key.pem"
```

| Поле | Тип | Описание |
|------|-----|----------|
| `enabled` | `bool` | Включить gRPC API |
| `listen` | `string` | Адрес **только loopback** (`127.0.0.1` или `::1`) |
| `token` | `string` | Bearer-токен; обязателен при `enabled = true` |
| `rate_limit_per_sec` | `u32` | Макс. RPC/сек (глобально). Превышение → gRPC `RESOURCE_EXHAUSTED` |
| `[api.tls]` | table | TLS 1.3 для gRPC (опционально) |
| `api.tls.enabled` | `bool` | Включить TLS |
| `api.tls.cert` / `key` | `string` | PEM-файлы; обязательны при `api.tls.enabled` |

### Методы (`skadi.api.v1.SkadiApi`)

| RPC | Описание |
|-----|----------|
| `AddVlessUser` | Добавить VLESS-пользователя (UUID) |
| `RemoveVlessUser` | Удалить по UUID |
| `ListVlessUsers` | Список VLESS-пользователей |
| `AddSocks5User` | Добавить SOCKS5 user/pass |
| `RemoveSocks5User` | Удалить по username |
| `ListSocks5Users` | Список SOCKS5-пользователей |
| `GetStats` | Счётчики пользователей (`vless_users`, `socks5_users`) |

Аутентификация: заголовок `authorization: Bearer <token>`.

`api.token` хранится как секрет: не попадает в `Debug`/`Display` конфига и
маскируется в structured-логах (`Bearer [REDACTED]`). Метрики Prometheus
не содержат токен.

Пример с [grpcurl](https://github.com/fullstorydev/grpcurl):

```bash
grpcurl -plaintext \
  -H "authorization: Bearer change-me-to-a-long-random-secret" \
  -d '{"user":{"id":"b831381d-6324-4d53-ad4f-8cda48b30811","email":"alice@example.com"}}' \
  127.0.0.1:10085 skadi.api.v1.SkadiApi/AddVlessUser
```

С TLS (`[api.tls]`):

```bash
grpcurl \
  -cacert certs/api.pem \
  -H "authorization: Bearer change-me-to-a-long-random-secret" \
  127.0.0.1:10085 skadi.api.v1.SkadiApi/ListVlessUsers
```

Автотест: `cargo test -p skadi-server --test grpc_api_e2e`.

---

## Секция `[metrics]`

Prometheus-метрики и health-check. HTTP-сервер на loopback.

```toml
[metrics]
enabled = true
listen = "127.0.0.1:9090"
```

| Поле | Тип | Описание |
|------|-----|----------|
| `enabled` | `bool` | Включить HTTP observability |
| `listen` | `string` | Адрес **только loopback** |

### Endpoints

| Путь | Описание |
|------|----------|
| `GET /metrics` | Prometheus text format |
| `GET /healthz` | `200 ok` — процесс жив |

### Метрики (без PII)

| Имя | Тип | Описание |
|-----|-----|----------|
| `skadicore_active_connections` | gauge | Активные relay-сессии |
| `skadicore_connections_total` | counter | `protocol`, `event` (opened/closed/failed) |
| `skadicore_transfer_bytes_total` | counter | `direction` (up/down) |

```bash
curl http://127.0.0.1:9090/healthz
curl http://127.0.0.1:9090/metrics
```

CLI: `--log-format=json|pretty` (по умолчанию `json`).

Автотест: `cargo test -p skadi-server --test metrics_e2e`.

---

## Валидация

Валидация происходит при загрузке конфига. При ошибке ядро
завершается с ненулевым кодом и понятным сообщением.

### Что проверяется

- `server.listen` — валидный `SocketAddr` (строка или непустой массив, без дубликатов).
- Хотя бы один протокол включён.
- `protocol.socks5.auth` — одно из допустимых значений.
- `protocol.vless.users[].id` — валидный UUID.
- `protocol.socks5.users[].username` — непустой.
- `protocol.socks5.users[].password` — непустой.

### Что НЕ проверяется (пока)

- Дубликаты `username` в SOCKS5.
- Дубликаты `id` в VLESS.
- Существование порта (проверяется при `bind`).
- Права на чтение файла (проверяются при открытии).

---

## Примеры

### Минимальный конфиг (SOCKS5 без пароля)

```toml
[server]
listen = "127.0.0.1:1080"

[protocol.socks5]
enabled = true
auth = "no-auth"
```

Подходит для локальной отладки. Не выставляйте на публичный
интерфейс.

### SOCKS5 с паролем

```toml
[server]
listen = "0.0.0.0:1080"

[protocol.socks5]
enabled = true
auth = "user-pass"

[[protocol.socks5.users]]
username = "alice"
password = "correct-horse-battery-staple"
```

### VLESS (для отладки без TLS)

```toml
[server]
listen = "0.0.0.0:8080"

[protocol.vless]
enabled = true

[[protocol.vless.users]]
id = "b831381d-6324-4d53-ad4f-8cda48b30811"
email = "alice"
```

⚠️ **Не используйте в реальных условиях.** VLESS без TLS
передаёт UUID и содержимое соединений открытым текстом.

### Оба протокола на одном порту

```toml
[server]
listen = "0.0.0.0:1080"

[protocol.socks5]
enabled = true
auth = "user-pass"

[[protocol.socks5.users]]
username = "alice"
password = "secret"

[protocol.vless]
enabled = true

[[protocol.vless.users]]
id = "b831381d-6324-4d53-ad4f-8cda48b30811"
```

**Внимание**: в текущем MVP `handle_client` поддерживает
только один активный протокол за раз. Если включены оба —
используется VLESS. Мультиплексирование по порту (определение
протокола по первым байтам) появится позже.

---

## Типичные ошибки

### `failed to load config from "config/skadi.toml"`

Файл не найден или не читается.

**Проверьте**:
- Путь к файлу (флаг `--config`).
- Права на чтение (`ls -la config/skadi.toml`).
- Рабочую директорию (`pwd`).

### `invalid TOML in "config/skadi.toml": expected ...`

Синтаксическая ошибка в TOML.

**Проверьте**:
- Кавычки вокруг строк.
- Отсутствие запятых в конце строк.
- Правильные секции `[server]` vs `[[protocol.socks5.users]]`
  (двойные скобки — для массива таблиц).

### `missing field 'listen'`

В секции `[server]` нет обязательного поля.

### `no protocol enabled`

Все протоколы выключены или не указаны.

**Проверьте**: `enabled = true` хотя бы у одного.

### `SOCKS5 auth is user-pass but no users defined`

`auth = "user-pass"`, но список `users` пуст.

**Исправьте**: добавьте хотя бы одного пользователя или
переключитесь на `auth = "no-auth"`.

### `failed to bind 0.0.0.0:443: permission denied`

Порт < 1024 требует прав root.

**Решения**:
- Запустить от root (не рекомендуется).
- Использовать `setcap`:
  ```bash
  sudo setcap 'cap_net_bind_service=+ep' ./target/release/skadicore
  ```
- Использовать порт > 1024 и пробросить через iptables.

### `failed to bind 0.0.0.0:443: address already in use`

Порт занят другим процессом.

**Проверьте**:
```bash
sudo ss -tlnp | grep :443
```

### `authentication failed` в логах при подключении

Пользователь прислал неверный UUID (VLESS) или логин/пароль
(SOCKS5).

**Проверьте**:
- UUID в клиенте совпадает с `id` в конфиге.
- Логин и пароль совпадают символ в символ.
- В клиенте не выбран метод аутентификации `no-auth`, если
  сервер требует `user-pass`.

---

## Клиентский режим (`skadicore client`)

Отдельный TOML-файл для локального inbound (SOCKS5 и/или TUN), который
туннелирует трафик через удалённый VLESS+TLS сервер.

```bash
skadicore client --config examples/client-vless-tls/client.toml
```

Нужен хотя бы один inbound: `client.listen` (SOCKS5) или `client.tun.enabled = true`.

### Секция `[client]`

| Поле | Тип | Описание |
|------|-----|----------|
| `listen` | string? | Адрес локального SOCKS5, например `127.0.0.1:1080` |

Приложения должны использовать **удалённый DNS** (`socks5h`, `curl --socks5-hostname`).
Обычный `socks5`/`--socks5` резолвит имена локально — DNS-запросы уходят провайдеру.
SkadiCore при старте клиента выводит предупреждение об этом риске.

### Секция `[client.tun]` (Linux)

| Поле | Тип | Описание |
|------|-----|----------|
| `enabled` | bool | Создать TUN-интерфейс (по умолчанию `false`) |
| `name` | string | Имя интерфейса, например `skadi0` |
| `address` | string | IPv4 клиента в туннеле, например `10.0.0.2` |
| `gateway` | string | IPv4 шлюза в туннеле, например `10.0.0.1` |
| `netmask` | string | Маска сети, например `255.255.255.0` |
| `mtu` | u16 | MTU интерфейса (по умолчанию `1400`) |
| `pmtud` | string | `static` (по умолчанию), `probe` или `off` — см. ниже |
| `mtu_overhead` | u16 | Запас на VLESS+TLS при `pmtud = "probe"` (по умолчанию `100`) |

**PMTUD:**

- **Phase 1** — выбор `effective_mtu` при старте:
  - **`static`** — использовать `mtu` как есть (по умолчанию `1400`).
  - **`probe`** — измерить path MTU до прокси (Linux `IP_MTU` на UDP connect)
    и выставить `effective_mtu = min(mtu, path_mtu - mtu_overhead)`.
  - **`off`** — как `static`, без дополнительной логики.
- **Phase 2 (runtime)** — для UDP через TUN:
  - oversized датаграммы не уходят в VLESS; приложению отправляется ICMP
    *Fragmentation Needed* с текущим MTU;
  - входящие ICMP type 3 code 4 понижают runtime MTU;
  - ICMP echo (ping) включён в userspace netstack (`enable_icmp`).
  - TCP MSS задаётся smoltcp по MTU интерфейса; динамическое изменение MTU TUN
    device после старта не поддерживается. IPv6 ICMP PTB — не реализован.

При `mtu >= 1500` и `pmtud != "probe"` клиент предупреждает о риске фрагментации.

#### `[client.tun.routing]`

| Поле | Тип | Описание |
|------|-----|----------|
| `auto` | bool | Автоматически настроить `ip rule` / `ip route` (по умолчанию `false`; требует root/CAP_NET_ADMIN) |
| `table` | u32 | Номер таблицы маршрутизации (по умолчанию `100`, диапазон `1..=252`) |
| `bypass` | string[] | Дополнительные IPv4, которые не должны идти в TUN (помимо IP прокси-сервера) |

При `auto = true` клиент:
1. Добавляет bypass-маршруты для IP прокси и `bypass` через основной шлюз.
2. Устанавливает `default dev <tun.name> table <table>`.
3. Добавляет `ip rule from <tun.address> table <table>`.

При остановке правила и таблица откатываются.

#### `[client.tun.dns]`

| Поле | Тип | Описание |
|------|-----|----------|
| `hijack` | bool | Перенаправлять UDP/53 через upstream DNS в VLESS (по умолчанию `true`) |
| `mode` | string | `udp`, `doh` или `dot` (см. ниже) |
| `server` | string? | Upstream: `udp` — IPv4/`host:53`; `doh` — `https://…/dns-query`; `dot` — `tls://host` или `host:853` |
| `block_system_dot` | bool | Блокировать TCP/853 (системный DoT), по умолчанию `true` |
| `block_system_doh` | bool | Блокировать TCP/443 к известным DoH IP и SNI, по умолчанию `true` |

TUN использует userspace netstack (`netstack-smoltcp`): TCP/UDP из
интерфейса уходят в VLESS. DNS-hijack перехватывает UDP на порт 53
на уровне netstack.

- **`mode = "udp"`** — DNS-пакеты форвардятся на `server` через VLESS UDP.
- **`mode = "doh"`** — DNS-запрос отправляется как RFC 8484 POST на DoH-сервер
  через VLESS TCP + TLS (шифрование до Cloudflare/Google, не plain UDP).
- **`mode = "dot"`** — DNS-over-TLS (RFC 7858) через VLESS TCP + TLS на порт 853.

При `hijack = true` по умолчанию блокируются системные обходы:
- **`block_system_dot`** — TCP/853 (DoT) закрывается без relay;
- **`block_system_doh`** — TCP/443 к известным DoH IP (1.1.1.1, 8.8.8.8, …) и
  SNI (`dns.google`, `cloudflare-dns.com`, `dns.quad9.net`, …). Для неизвестных
  IP читается TLS ClientHello; ECH/SNI-less соединения не блокируются.

Приложения должны откатиться на UDP/53, который перехватывается `hijack`.
Произвольный HTTPS на 443 **не** блокируется. При `hijack = false` клиент
предупреждает об утечке DNS при старте.

Пример DoH:

```toml
[client.tun.dns]
hijack = true
mode = "doh"
server = "https://cloudflare-dns.com/dns-query"
```

Пример DoT:

```toml
[client.tun.dns]
hijack = true
mode = "dot"
server = "tls://one.one.one.one"
block_system_dot = true
block_system_doh = true
```

Пример SOCKS5 + TUN: `examples/client-vless-tls-tun/client.toml`.

### Секция `[remote]`

| Поле | Тип | Описание |
|------|-----|----------|
| `server` | string | Адрес удалённого прокси `host:port` |
| `uuid` | string | UUID пользователя VLESS на сервере |

### Секция `[remote.tls]`

| Поле | Тип | Описание |
|------|-----|----------|
| `enabled` | bool | TLS к удалённому прокси (по умолчанию `false`) |
| `ca_file` | string? | PEM с доверенным CA; если не задан — системные корни |
| `server_name` | string? | SNI для TLS; по умолчанию hostname из `server` |
| `kex_mode` | string | `classic` (по умолчанию) или `hybrid_pq` (X25519MLKEM768) |

Пример: `examples/client-vless-tls/client.toml`.

---

## Что дальше

Когда конфиг разрастётся, в этот документ добавятся:

- Секция `[transport.tls]` — сертификаты, ALPN, cipher suites.
- Секция `[transport.reality]` — dest, serverNames, privateKey.
- Секция `[api].tls` — TLS для gRPC (опционально, v2).
- Расширенные метрики (latency histograms, transport errors).
- Секция `[log]` — уровень, формат, путь.

Документ должен отражать **текущее состояние кода**. Если реализация и документ расходятся — баг в документе.
