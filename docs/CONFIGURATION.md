# Конфигурация SkadiCore

Документ описывает формат TOML-конфига: все поля, значения по
умолчанию, ограничения и типичные ошибки.

**Важно**: конфиг читается один раз при старте. Горячая
перезагрузка (`SIGHUP`) пока не реализована — для изменения
конфига нужен перезапуск процесса.

---

## Содержание

- [Общая структура](#общая-структура)
- [Секция `[server]`](#секция-server)
- [Секция `[transport.tls]`](#секция-transporttls)
- [Секция `[protocol.socks5]`](#секция-protocolsocks5)
- [Секция `[protocol.vless]`](#секция-protocolvless)
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
[transport.tls]       # опционально
[protocol.socks5]     # опционально
[protocol.vless]      # опционально
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
```

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
TCP accept → TLS handshake → протокол (SOCKS5/VLESS) → upstream TCP
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
- Секция `[api]` — gRPC endpoint, токен, TLS.
- Секция `[metrics]` — Prometheus endpoint, интервал.
- Секция `[log]` — уровень, формат, путь.

Документ должен отражать **текущее состояние кода**. Если реализация и документ расходятся — баг в документе.
