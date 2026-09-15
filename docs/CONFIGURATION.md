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

**Тип**: `String`
**Обязательно**: да
**Формат**: `IP:PORT`

Адрес и порт для входящих соединений.

```toml
[server]
listen = "0.0.0.0:443"
max_connections = 1000
```

### `max_connections`

**Тип**: `u32`  
**По умолчанию**: не задано (без лимита)

Максимум одновременных inbound-сессий (от `accept` до закрытия relay/TLS/REALITY).
При превышении лимита новое TCP-соединение **сразу закрывается** без ожидания в очереди.

Метрика: `skadicore_connections_rejected_total`.

Изменение через SIGHUP **не** поддерживается — нужен рестарт.

`max_connections = 0` — ошибка валидации.

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
  ├─ да  → dynamic Ed25519 cert → TLS 1.3 → VLESS/SOCKS5 → upstream
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

- `server.listen` — валидный `SocketAddr`.
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

## Что дальше

Когда конфиг разрастётся, в этот документ добавятся:

- Секция `[transport.tls]` — сертификаты, ALPN, cipher suites.
- Секция `[transport.reality]` — dest, serverNames, privateKey.
- Секция `[api].tls` — TLS для gRPC (опционально, v2).
- Расширенные метрики (latency histograms, transport errors).
- Секция `[log]` — уровень, формат, путь.

Документ должен отражать **текущее состояние кода**. Если реализация и документ расходятся — баг в документе.
