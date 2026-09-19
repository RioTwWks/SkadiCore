# Разработка SkadiCore

Документ для тех, кто пишет код SkadiCore. Если вы просто
хотите использовать ядро — начните с `README.md`.

---

## Содержание

- [Окружение](#окружение)
- [Сборка](#сборка)
- [Тестирование](#тестирование)
- [Fuzz-тестирование](#fuzz-тестирование)
- [Стиль кода](#стиль-кода)
- [Работа с парсерами](#работа-с-парсерами)
- [Добавление нового протокола](#добавление-нового-протокола)
- [Добавление нового транспорта](#добавление-нового-транспорта)
- [Отладка](#отладка)
- [Полезные команды](#полезные-команды)

---

## Окружение

### Требования

- **Rust stable** — версия зафиксирована в `rust-toolchain.toml`.
  Установите через [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **nightly Rust** — для fuzz-тестов:
  ```bash
  rustup toolchain install nightly
  ```
- **cargo-fuzz** — для fuzzing:
  ```bash
  cargo install cargo-fuzz
  ```
- **cargo-audit** — для проверки CVE в зависимостях:
  ```bash
  cargo install cargo-audit
  ```
- **cargo-deny** — для проверки лицензий и дубликатов:
  ```bash
  cargo install cargo-deny
  ```

### Опционально

- **socat** — для ручной отладки протоколов.
- **tcpdump** — для захвата трафика.
- **curl** — для проверки SOCKS5.

### Проверка окружения

```bash
rustc --version       # должен совпасть с rust-toolchain.toml
cargo --version
cargo fuzz --version  # если установлен
```

---

## Сборка

### Debug

```bash
cargo build
```

Быстро, но медленный рантайм. Подходит для разработки.

### Release

```bash
cargo build --release
```

Профиль настроен в корневом `Cargo.toml`:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = true
panic = "abort"
```

- `lto = "thin"` — межмодульная оптимизация. Даёт 5–15%
  прироста, сборка медленнее на 20–30%.
- `codegen-units = 1` — один модуль компиляции. Максимальная
  оптимизация, но медленная сборка.
- `strip = true` — убирает символы из бинарника.
- `panic = "abort"` — не разворачивает стек при панике.
  Уменьшает размер, ускоряет. **Но**: `catch_unwind` не
  работает.

### Кросс-компиляция (musl)

Статический Linux-бинарник без зависимости от glibc. Линкер
настроен в `.cargo/config.toml` (`musl-gcc` / `aarch64-linux-musl-gcc`).

**Быстрый путь (x86_64):**

```bash
sudo apt install musl-tools   # Debian/Ubuntu
./scripts/build-musl.sh
```

**Вручную:**

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release -p skadi-server --target x86_64-unknown-linux-musl
file target/x86_64-unknown-linux-musl/release/skadicore
# ожидается: statically linked / static-pie
```

**ARM64 (aarch64):** рекомендуется `cargo-zigbuild` + Zig (как в CI):

```bash
pip install cargo-zigbuild   # ставит ziglang
TARGET=aarch64-unknown-linux-musl ./scripts/build-musl.sh
```

Альтернатива — кросс-компилятор `aarch64-linux-musl-gcc` (musl.cc, локально).
На устройстве aarch64 можно собирать нативно без кросс-компилятора.

CI проверяет `x86_64-unknown-linux-musl` (musl-tools) и
`aarch64-unknown-linux-musl` (cargo-zigbuild) на каждом PR (job `musl`).

### Docker (musl, `FROM scratch`)

Версия Rust зафиксирована в `rust-toolchain.toml` (та же используется в Dockerfile).

```bash
docker build -t skadicore:local .

docker run --rm -p 443:443 \
  -v "$(pwd)/config:/config:ro" \
  -v "$(pwd)/certs:/certs:ro" \
  skadicore:local --config /config/skadi.toml
```

Образ содержит только статический бинарник `/skadicore` — без shell и libc.
Конфигурация и TLS-сертификаты монтируются с хоста.

Проверка сборки образа — job `docker` в CI.

### Кросс-компиляция (Windows / macOS)

**Windows (x86_64, GNU ABI):**

```bash
sudo apt install gcc-mingw-w64-x86-64   # Debian/Ubuntu
TARGET=x86_64-pc-windows-gnu ./scripts/build-cross.sh
file target/x86_64-pc-windows-gnu/release/skadicore.exe
```

Линкер `x86_64-w64-mingw32-gcc` задан в `.cargo/config.toml`.
На Windows можно собирать нативно: `cargo build --release -p skadi-server`.

**macOS (aarch64 / x86_64):** только на macOS (или в CI на `macos-latest`):

```bash
TARGET=aarch64-apple-darwin ./scripts/build-cross.sh
TARGET=x86_64-apple-darwin ./scripts/build-cross.sh   # cross с Apple Silicon
```

CI job `cross-platform` собирает Windows GNU (Ubuntu + mingw) и оба macOS target
на `macos-latest` с smoke `check-config`.

### GitHub Releases

Пошаговый чеклист: [`docs/RELEASING.md`](RELEASING.md).

Релизные статические бинарники публикуются workflow `.github/workflows/release.yml`
при push тега `v*` (версия в теге должна совпадать с `Cargo.toml`).

```bash
# 1. Обновить version в Cargo.toml и CHANGELOG.md
# 2. Закоммитить, создать тег и запушить
git tag v0.1.0
git push origin v0.1.0
```

Артефакты релиза:

- `skadicore-<version>-x86_64-unknown-linux-musl.tar.gz`
- `skadicore-<version>-aarch64-unknown-linux-musl.tar.gz`
- `skadicore-<version>-x86_64-pc-windows-gnu.zip`
- `skadicore-<version>-aarch64-apple-darwin.tar.gz`
- `skadicore-<version>-x86_64-apple-darwin.tar.gz`
- `SHA256SUMS`
- `*.minisig` — подписи minisign (если в репозитории задан секрет `MINISIGN_SECRET_KEY`)

Локальная упаковка после сборки:

```bash
./scripts/build-musl.sh
./scripts/package-release.sh 0.1.0 x86_64-unknown-linux-musl
```

Подпись релиза (опционально, требуется [minisign](https://github.com/jedisct1/minisign)):

```bash
# Сгенерировать ключи (один раз): minisign -G -p minisign.pub -s minisign.key
export MINISIGN_SECRET_KEY="$(cat minisign.key)"
./scripts/sign-release.sh
```

Проверка подписи на стороне пользователя:

```bash
./scripts/verify-release.sh minisign.pub dist
```

Ручной запуск workflow: Actions → Release → Run workflow (указать существующий тег).

---

## Тестирование

### Юнит-тесты

```bash
# Всё
cargo test --workspace

# Только парсеры (быстро)
cargo test -p skadi-protocol

# Конкретный тест
cargo test -p skadi-protocol parse_greeting_ok
```

### Интеграционные тесты

Живут в `crates/skadi-server/tests/` и поднимают реальный сервер
через `skadi_server::run_server()`:

| Файл | Что проверяет |
|------|----------------|
| `tls_socks5_e2e.rs` | SOCKS5 CONNECT + relay поверх TLS |
| `tls_vless_e2e.rs` | VLESS TCP + relay; отказ при неверном UUID |
| `tls_sni_e2e.rs` | SNI-роутинг и fallback на default cert |

```bash
cargo test -p skadi-server --test tls_vless_e2e
cargo test -p skadi-server   # все три
```

Тесты используют `rcgen` для self-signed сертификатов и
`tempfile` для PEM на диске. Crypto provider: `rustls::crypto::ring`.

### Ручная проверка (smoke test)

Транспорты AWG / Hysteria2 / TUIC (примеры + опциональные бинарники):

```bash
./scripts/smoke-transports.sh
SMOKE_START=1 ./scripts/smoke-transports.sh
```

REALITY active probe (тайминги TLS handshake vs `dest`, опционально JA3):

```bash
./scripts/probe-test.sh --dest www.example.com:443 --sni www.example.com
./scripts/probe-test.sh --dest www.example.com:443 --sni www.example.com --skadi 127.0.0.1:443
```

SOCKS5 без TLS:

```bash
cargo run --bin skadicore -- --config config/skadi.toml
curl --socks5-hostname 127.0.0.1:1080 https://example.com
```

TLS inbound (нужны `cert.pem` / `key.pem`, `transport.tls.enabled = true`):

```bash
openssl s_client -connect localhost:443 -servername localhost
```

TLS outbound (проверка upstream через прокси с `[outbound.tls]`):

```bash
# Автотест: VLESS → TLS echo upstream
cargo test -p skadi-server --test tls_outbound_e2e

# Ручная проверка TLS-сервера upstream (до включения outbound.tls)
openssl s_client -connect upstream.example.com:443 -servername upstream.example.com
```

VLESS over TLS: клиент v2rayNG / Nekoray с TLS + UUID из конфига.
Автотест собирает запрос через `build_tcp_request()` из `skadi-protocol`.

VLESS Mux (TCP/UDP over CMD_MUX):

```bash
cargo test -p skadi-server --test vless_mux_e2e
cargo test -p skadi-server --test vless_mux_udp_e2e
cargo test -p skadi-server --test vless_xudp_e2e
```

VLESS over REALITY (рекомендуется):

```bash
# Автотест с реальным Xray-core (совместим с v2rayNG / Nekoray)
cargo test -p skadi-server --test reality_vless_xray_e2e

# Ручная проверка
cargo run --bin skadicore -- genkey reality
cargo run --bin skadicore -- --config examples/reality-vless/server.toml
xray run -c examples/reality-vless/client-xray.json
curl --socks5-hostname 127.0.0.1:10808 https://example.com
```

Нужны `curl` и `unzip` для авто-скачивания Xray, либо `XRAY_BINARY=/path/to/xray`.
См. `examples/reality-vless/README.md` для Nekoray и v2rayNG share link.
Полный список примеров: `examples/README.md` (VLESS TLS, SOCKS5 TLS).

**REALITY TLS client (rustls):** после REALITY auth session `auth_key` проверяйте leaf
через `rustls::reality::RealityServerCertVerifier` (или `skadi_transport::reality::verify_server_cert_hmac`).
Не используйте «accept all» verifier на пользовательском трафике; исключение — однократный
`fetch_impersonate_cert_from_dest` для админского prefetch шаблона с публичного `dest`.

### Покрытие

```bash
cargo install cargo-tarpaulin  # Linux
cargo tarpaulin --workspace --out Html
```

Цель: **≥ 70%** для парсеров, **≥ 50%** в целом.

### Линтеры

```bash
# Форматирование
cargo fmt --check

# Автоисправление форматирования
cargo fmt

# Clippy с предупреждениями как ошибками
cargo clippy --workspace --all-targets -- -D warnings
```

CI запускает всё это на каждый PR. Локально — перед коммитом.

### Аудит зависимостей

```bash
# CVE в зависимостях
cargo audit

# Лицензии, дубликаты, запрещённые крейты
cargo deny check
```

---

## Fuzz-тестирование

Fuzz-тесты живут в отдельном workspace:
`crates/skadi-protocol/fuzz/`. Они не входят в основной
workspace, потому что требуют nightly Rust.

### Запуск

```bash
cd crates/skadi-protocol

# Конкретный таргет
cargo +nightly fuzz run parse_greeting

# С таймаутом (остановить через 60 секунд)
cargo +nightly fuzz run parse_greeting -- -max_total_time=60

# С несколькими воркерами (использовать все ядра)
cargo +nightly fuzz run parse_greeting -- -workers=4
```

### Что делает fuzzer

Он подаёт случайные байты на вход парсеру и следит за:
- **Паниками** — самая частая проблема.
- **Утечками памяти** — через AddressSanitizer.
- **Бесконечными циклами** — через таймаут.
- **OOM** — через лимит памяти.

### Если fuzzer нашёл падение

Артефакт сохраняется в `fuzz/artifacts/<target>/crash-<hash>`.

Воспроизвести:

```bash
cargo +nightly fuzz run parse_greeting \
  fuzz/artifacts/parse_greeting/crash-<hash>
```

Минимальный входной файл — в `fuzz/artifacts/.../minimized-*`.

### Добавление нового таргета

1. Создать `fuzz/fuzz_targets/<name>.rs`:
   ```rust
   #![no_main]
   use libfuzzer_sys::fuzz_target;
   use skadi_protocol::vless::parse::parse_request;

   fuzz_target!(|data: &[u8]| {
       let _ = parse_request(data);
   });
   ```
2. Добавить в `fuzz/Cargo.toml`:
   ```toml
   [[bin]]
   name = "<name>"
   path = "fuzz_targets/<name>.rs"
   test = false
   doc = false
   ```
3. Запустить:
   ```bash
   cargo +nightly fuzz run <name>
   ```

### Сколько гонять

- **Локально перед коммитом**: 60 секунд на таргет.
- **В CI**: 5 минут на таргет (по расписанию, не на каждый PR).
- **Перед релизом**: ночь на каждый таргет.

Fuzzer находит новые пути бесконечно. Через 5 минут обычно
покрыты все очевидные ветки. Через час — редкие комбинации.

---

## Качество и безопасность зависимостей

### cargo deny

Проверка лицензий, advisories и дубликатов:

```bash
cargo install cargo-deny --locked
cargo deny check
```

Конфигурация: `deny.toml` в корне репозитория. Запускается в CI (job `cargo deny`).

Политика supply-chain:
- **advisories** — yanked crates и unsound advisories → ошибка; unmaintained — для direct deps workspace
- **bans** — запрещены `openssl`, `openssl-sys`, `native-tls`, `tokio-native-tls`, `hyper-tls`
- **sources** — только `crates.io`; git-источники запрещены (vendored `rustls` — path dep, не git)

### Покрытие тестами (tarpaulin)

```bash
sudo apt-get install -y libssl-dev pkg-config
cargo install cargo-tarpaulin --locked
cargo tarpaulin \
  -p skadi-core -p skadi-transport -p skadi-protocol -p skadi-api -p skadi-server \
  --exclude-files 'third_party/*' \
  --fail-under 70
```

Порог **70%** enforced в CI (job `Coverage (tarpaulin)`). Vendored `third_party/` исключается из отчёта.

---

## Нагрузочное тестирование

### Автотесты (Rust)

В CI прогоняется `connection_load_e2e::concurrent_vless_connections` — 64
параллельных VLESS-сессии через plain TCP.

Ручной тяжёлый прогон (512 соединений):

```bash
cargo test -p skadi-server --test connection_load_e2e massive -- --ignored
```

Для прогона ближе к 10k соединений увеличьте константу `CONNECTIONS` в тесте
`massive_concurrent_vless_connections` и `max_connections` в конфиге прокси.

### Soak-тесты (утечки памяти / зависшие сессии)

#### Rust E2E (CI)

`connection_soak_e2e::sequential_connections_no_leak` — 120 последовательных
VLESS connect/relay/close. Проверяется:

- `skadicore_active_connections` возвращается к `0` (метрики)
- рост VmRSS процесса (Linux `/proc/self/status`) не превышает порога

```bash
cargo test -p skadi-server --test connection_soak_e2e

# Длинный прогон (2000 итераций):
cargo test -p skadi-server --test connection_soak_e2e long -- --ignored
```

#### 24-часовой soak (`scripts/soak.sh`)

Скрипт поднимает SkadiCore + upstream, генерирует нагрузку и пишет CSV с метриками
(RSS, FD, `skadicore_active_connections`) каждые `SOAK_INTERVAL` секунд.

Зависимости: `socat` (режим `native`), опционально `iperf3`, `wrk`, `proxychains4`.

```bash
# Быстрая проверка (~2 мин, CI smoke):
./scripts/soak.sh --quick

# Полный прогон 24 часа (нагрузка soak_load через VLESS):
SOAK_DURATION=24h ./scripts/soak.sh

# iperf3 upstream + VLESS relay:
SOAK_LOAD=iperf3 SOAK_DURATION=1h ./scripts/soak.sh

# HTTP через SOCKS5 + wrk (proxychains4):
SOAK_LOAD=wrk SOAK_DURATION=30m ./scripts/soak.sh
```

Переменные окружения:

| Переменная | По умолчанию | Описание |
|------------|--------------|----------|
| `SOAK_DURATION` | `24h` | `24h`, `30m`, `3600s` |
| `SOAK_INTERVAL` | `60` | Интервал снятия метрик (сек) |
| `SOAK_LOAD` | `native` | `native` \| `iperf3` \| `wrk` |
| `SOAK_CONCURRENCY` | `32` | Параллельность нагрузки |
| `MAX_RSS_GROWTH_MB` | `256` | Порог роста RSS |
| `MAX_FD_GROWTH` | `500` | Порог роста FD |

Артефакты: `$SOAK_LOG_DIR/samples.csv`, `skadicore.log`, `load.log`.

Генератор нагрузки VLESS: `cargo run --release --bin soak_load -- --help`.

Пример конфига для ручного запуска: `examples/soak/server.toml`.

Рекомендуется задавать `[server]` лимиты перед нагрузкой:

```toml
max_connections = 10000
idle_timeout_secs = 300
max_session_lifetime_secs = 3600
```

---

## Стиль кода

### Форматирование

`rustfmt.toml`:

```toml
edition = "2021"
max_width = 100
use_field_init_shorthand = true
```

Перед коммитом:
```bash
cargo fmt
```

### Соглашения

- **Имена крейтов**: `skadi-<назначение>`.
- **Имена модулей**: `snake_case`, во множественном числе для
  коллекций (`users`, `sessions`).
- **Имена типов**: `CamelCase`. Суффиксы: `Config`, `Handler`,
  `Transport`, `Error`, `Request`, `Response`.
- **Функции**: `snake_case`, глагол в начале (`parse_`,
  `build_`, `handle_`, `connect_`).

### Обработка ошибок

- **Библиотечные крейты** (`skadi-core`,
  `skadi-protocol`, `skadi-transport`) — `thiserror` для
  типизированных ошибок.
- **Бинарник** (`skadi-server`) — `anyhow` для контекста.

```rust
// Библиотека
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("not enough data: need {need}, have {have}")]
    Incomplete { need: usize, have: usize },
}

// Бинарник
let config = Config::load(&path)
    .with_context(|| format!("failed to load {:?}", path))?;
```

### Логирование

- **Библиотеки** используют `tracing` и не инициализируют
  subscriber.
- **Бинарник** инициализирует subscriber в `main`.
- **Уровни**:
  - `error!` — то, что требует вмешательства.
  - `warn!` — подозрительное, но не критичное.
  - `info!` — значимые события (сессия открыта, закрыта).
  - `debug!` — детали протокола (endpoint, команда).
  - `trace!` — байты, состояния парсера (только для отладки).

### Что НЕ логировать

- Пароли.
- Полные UUID (можно логировать в hex, но не как строку
  пользователя).
- Содержимое трафика.
- Реальные IP пользователей на уровне `info`.

### `unsafe`

Использовать **крайне редко**. Если нужен — обязателен
комментарий `// SAFETY:` с обоснованием:

```rust
// SAFETY: указатель получен из Box::into_raw и не используется
// после этого вызова. Владение возвращается через Box::from_raw.
let ptr = Box::into_raw(value);
```

### `unwrap()` и `expect()`

- **Запрещены** на горячем пути и в обработке недоверенных
  данных.
- **Допустимы** в тестах и в `main` для инициализации, где
  паника — правильное поведение.

---

## Работа с парсерами

Парсеры — критическая часть проекта. Правила:

### 1. Чистые функции

Парсер **не делает I/O**. Он принимает `&[u8]` и возвращает
`Result<(T, usize), ParseError>`:

```rust
pub fn parse_request(input: &[u8]) -> Result<(VlessRequest, usize), ParseError>
```

Второй элемент кортежа — количество потреблённых байт.

### 2. Явные ошибки

Все ошибки — в `enum ParseError` через `thiserror`. Никаких
`anyhow` в парсерах.

```rust
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("not enough data: need {need}, have {have}")]
    Incomplete { need: usize, have: usize },

    #[error("unsupported version: 0x{0:02x}")]
    BadVersion(u8),
}
```

### 3. Лимиты на всё

Любое поле с переменной длиной имеет константу-лимит:

```rust
pub const MAX_ADDONS: usize = 512;
pub const MAX_DOMAIN: usize = 255;
pub const MAX_METHODS: usize = 16;
```

Проверка лимита идёт **до** аллокации.

### 4. Юнит-тесты

Каждый парсер имеет тесты на:
- Успешный разбор минимального валидного входа.
- Успешный разбор с опциональными полями.
- Обрезанный вход (Incomplete).
- Неверная версия / тип.
- Пустое поле.
- Превышение лимита.

### 5. Fuzz-таргет

Для каждого парсера — таргет в `fuzz/fuzz_targets/`.

### 6. Никаких паник

`unwrap()`, `expect()`, `panic!()` — запрещены. Индексация
массива — только после проверки длины. Арифметика — с
`checked_add`, `checked_mul`, если есть риск переполнения.

---

## Добавление нового протокола

Рассмотрим на примере Trojan.

### 1. Создать структуру

```
crates/skadi-protocol/src/trojan/
├── mod.rs          # публичный API, реэкспорты
├── parse.rs        # чистые парсеры
├── handler.rs      # I/O-обвязка
└── config.rs       # конфигурация
```

### 2. `config.rs`

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TrojanConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub users: Vec<TrojanUser>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrojanUser {
    pub password: String,
    pub email: Option<String>,
}

impl TrojanConfig {
    pub fn authenticate(&self, candidate: &str) -> Option<&TrojanUser> {
        use subtle::ConstantTimeEq;
        for user in &self.users {
            if user.password.as_bytes().ct_eq(candidate.as_bytes()).into() {
                return Some(user);
            }
        }
        None
    }
}
```

### 3. `parse.rs`

Чистые функции разбора заголовка:

```rust
use thiserror::Error;
use skadi_core::Endpoint;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("not enough data")]
    Incomplete,
    // ...
}

pub struct TrojanRequest {
    pub password_hash: [u8; 56],   // SHA-224 от пароля
    pub command: u8,
    pub target: Endpoint,
}

pub fn parse_request(input: &[u8]) -> Result<(TrojanRequest, usize), ParseError> {
    // ...
}
```

**Тесты** — по той же схеме, что для SOCKS5 и VLESS.

### 4. `handler.rs`

I/O-обвязка:

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct TrojanHandler;

impl TrojanHandler {
    pub async fn handshake(
        client: &mut TcpStream,
        config: &TrojanConfig,
    ) -> Result<Endpoint> {
        // Читаем заголовок, парсим, аутентифицируем.
        // ...
    }
}
```

### 5. Fuzz-таргет

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use skadi_protocol::trojan::parse::parse_request;

fuzz_target!(|data: &[u8]| {
    let _ = parse_request(data);
});
```

Добавить в `fuzz/Cargo.toml`.

### 6. Подключить в `skadi-server`

В `config.rs`:
```rust
#[derive(Debug, Deserialize, Default)]
pub struct ProtocolConfig {
    #[serde(default)]
    pub socks5: Socks5Config,
    #[serde(default)]
    pub vless: VlessConfig,
    #[serde(default)]
    pub trojan: TrojanConfig,  // новое
}
```

В `handle_client`:
```rust
let target = if config.trojan.enabled {
    TrojanHandler::handshake(&mut client, &config.trojan).await?
} else if config.vless.enabled {
    // ...
};
```

### 7. Обновить документацию

- `docs/PROTOCOLS.md` — добавить секцию.
- `docs/CONFIGURATION.md` — добавить секцию `[protocol.trojan]`.
- `CHANGELOG.md` — запись в `[Unreleased]`.

---

## Добавление нового транспорта

Транспорт — это то, что **несёт** байты. Пример: XHTTP.

### 1. Создать модуль

```
crates/skadi-transport/src/xhttp.rs
```

### 2. Реализовать

```rust
use anyhow::Result;
use skadi_core::Endpoint;
use tokio::io::{AsyncRead, AsyncWrite};

pub struct XhttpTransport {
    base: TcpTransport,
    path: String,
    host: String,
}

impl XhttpTransport {
    pub async fn connect(
        &self,
        endpoint: &Endpoint,
    ) -> Result<impl AsyncRead + AsyncWrite + Unpin> {
        // Установить TCP-соединение через base.
        // Обернуть в HTTP/2-сессию.
        // Вернуть стрим.
        todo!()
    }
}
```

### 3. Правила

- Транспорт **не знает про протоколы**. XHTTP может нести
  VLESS, Trojan или что угодно.
- Транспорт возвращает `impl AsyncRead + AsyncWrite`. Это
  позволяет оборачивать его в другие транспорты.
- Ошибки — типизированные, через `thiserror`.

### 4. Обобщить handler

Если handler принимает `&mut TcpStream` — заменить на generic:

```rust
pub async fn handshake<S>(
    stream: &mut S,
    config: &VlessConfig,
) -> Result<Endpoint>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // ...
}
```

Это позволит использовать handler с любым транспортом.

---

## Отладка

### Логи

```bash
# Debug-уровень
RUST_LOG=debug ./target/release/skadicore --config config/skadi.toml

# Только конкретный модуль
RUST_LOG=skadi_protocol=trace ./target/release/skadicore
```

### Захват трафика

```bash
# Локально
sudo tcpdump -i lo -X 'tcp port 1080'

# С сохранением в файл для Wireshark
sudo tcpdump -i eth0 -w capture.pcap 'tcp port 1080'
```

### Ручная проверка протокола

```bash
# Слушать на порту и печатать hex
socat -v TCP-LISTEN:1080,reuseaddr,fork -

# Отправить байты вручную
echo -ne '\x05\x01\x00' | nc 127.0.0.1 1080 | xxd
```

### Отладка в VS Code

`.vscode/launch.json`:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug skadicore",
      "cargo": {
        "args": ["build", "--bin=skadicore", "--package=skadi-server"]
      },
      "args": ["--config", "config/skadi.toml", "--log-level", "debug"],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

---

## Полезные команды

```bash
# Сборка и запуск
cargo run --bin skadicore -- --config config/skadi.toml

# Проверка без сборки бинарника
cargo check --workspace

# Тесты с выводом println!
cargo test -- --nocapture

# Конкретный тест
cargo test -p skadi-protocol parse_request_domain_ok

# Miri (парсеры, memory safety)
./scripts/miri-test.sh
# или вручную:
cargo +nightly miri test -p skadi-protocol

# Бенчмарки (criterion)
cargo bench -p skadi-protocol
cargo bench -p skadi-transport
# smoke в CI (1 итерация):
cargo bench -p skadi-protocol -- --test

# Размер бинарника
ls -lh target/release/skadicore

# Зависимости
cargo tree

# Устаревшие зависимости
cargo outdated

# Размер каждой зависимости в бинарнике
cargo bloat --release --crates
```

---

## Что дальше

- **Miri** — `./scripts/miri-test.sh`; CI job `miri`.
- **Бенчмарки** (`criterion`) — `crates/skadi-protocol/benches/`, `crates/skadi-transport/benches/relay.rs`.
- **REALITY** — отдельный раздел про `rustls-reality`.
- **gRPC API** — `crates/skadi-api/proto/skadi.proto`, реализация в `skadi-server/src/api/`.
  Тест: `cargo test -p skadi-server --test grpc_api_e2e`.
- **Observability** — `skadi-server/src/observability/`, `[metrics]` в конфиге.
  Тест: `cargo test -p skadi-server --test metrics_e2e`.
- **CLI** — `skadicore check-config`, SIGHUP reload `[protocol.*]`:
  ```bash
  cargo run --bin skadicore -- check-config --config config/skadi.toml
  kill -HUP $(pidof skadicore)   # перечитать пользователей из TOML
  ```
- **Метрики** — куда вставлять инкременты в `handle_connection`.

CI описан в `.github/workflows/ci.yml`. Интеграционные тесты — выше.
