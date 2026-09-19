# Релизы SkadiCore

Этот документ для **тех, кто выпускает версии** (maintainer), и для **тех, кто скачивает бинарники** с GitHub Releases.

Связанные файлы:

| Файл | Назначение |
|------|------------|
| `CHANGELOG.md` | Что изменилось (формат [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/)) |
| `Cargo.toml` | Номер версии в коде (`[workspace.package] version`) |
| `.github/workflows/release.yml` | Сборка и публикация при теге `v*` |
| `scripts/package-release.sh` | Упаковка бинарника в `.tar.gz` / `.zip` |
| `scripts/sign-release.sh` | Подпись артефактов (если настроен секрет) |
| `scripts/verify-release.sh` | Проверка подписей у пользователя |

Версионирование — [Semantic Versioning](https://semver.org/lang/ru/): `MAJOR.MINOR.PATCH` (тег `v0.1.1` → версия `0.1.1`).

---

## Как устроен релиз (кратко)

```text
main + CHANGELOG + версия в Cargo.toml
        ↓
   git tag v0.1.1 && git push origin v0.1.1
        ↓
   GitHub Actions: workflow «Release»
        ↓
   GitHub Releases: архивы + SHA256SUMS (+ опционально .minisig)
```

**Важно:** тег `vX.Y.Z` должен **совпадать** с `version` в корневом `Cargo.toml`. Иначе workflow упадёт на шаге проверки.

Сборка идёт на GitHub (Linux musl x86_64/aarch64, Windows GNU, macOS). Локально релиз собирать не обязательно, если вас устраивает CI.

---

## Maintainer: выпуск новой версии

### 1. Накопить изменения в `CHANGELOG.md`

Пока разработка идёт, всё пишется в секцию **`[Unreleased]`**.

Перед релизом:

1. Переименовать `[Unreleased]` в `[X.Y.Z] - ГГГГ-ММ-ДД` (дата релиза).
2. Открыть новую пустую секцию `[Unreleased]` вверху файла.
3. Обновить ссылки внизу `CHANGELOG.md` (`compare/v...`).

### 2. Версия в `Cargo.toml`

В корневом `Cargo.toml`:

```toml
[workspace.package]
version = "0.1.1"
```

Все крейты берут версию через `version.workspace = true` — менять нужно **только здесь**.

### 3. Документация и примеры

При изменении конфига или CLI обновите `docs/CONFIGURATION.md`, `README.md`, каталоги `examples/`.

### 4. Зелёный CI на `main`

Убедитесь, что последний коммит на `main` прошёл workflow **CI** (тесты, clippy, musl и т.д.).

### 5. Тег (после merge в `main`)

Тег ставят на **уже смерженный** `main`, отдельным шагом:

```bash
git checkout main
git pull origin main
git tag -a v0.1.1 -m "SkadiCore 0.1.1"
git push origin v0.1.1
```

Push тега **запускает** workflow Release. Через несколько минут на странице [Releases](https://github.com/RioTwWks/SkadiCore/releases) появятся файлы.

### 6. Если сборку нужно повторить (тот же тег)

Не обязательно удалять тег. В GitHub:

**Actions → Release → Run workflow** → в поле tag указать, например, `v0.1.1`.

Workflow пересоберёт артефакты и обновит Release (если job завершился успешно).

Переместить тег на другой коммит (редко):

```bash
git tag -fa v0.1.1 -m "SkadiCore 0.1.1"
git push origin v0.1.1 --force
```

---

## Что попадает на GitHub Release

Для версии `0.1.1` типичные имена:

| Файл | Содержимое |
|------|------------|
| `skadicore-0.1.1-x86_64-unknown-linux-musl.tar.gz` | Статический Linux x86_64 |
| `skadicore-0.1.1-aarch64-unknown-linux-musl.tar.gz` | Статический Linux ARM64 |
| `skadicore-0.1.1-x86_64-pc-windows-gnu.zip` | Windows |
| `skadicore-0.1.1-aarch64-apple-darwin.tar.gz` | macOS Apple Silicon |
| `skadicore-0.1.1-x86_64-apple-darwin.tar.gz` | macOS Intel |
| `SHA256SUMS` | Контрольные суммы всех архивов |

Внутри архива один исполняемый файл: `skadicore` (или `skadicore.exe`).

---

## Подпись релизов: что такое minisign и зачем

### Зачем это нужно

- **`SHA256SUMS`** — проверяет, что файл **не побился** при скачивании (битый диск, прокси).
- **Подпись minisign** — дополнительно показывает, что архив **выпустил владелец ключа**, а не подменённый файл на зеркале.

Подпись **не обязательна**: без настройки секрета релизы всё равно публикуются, просто не будет файлов `*.minisig`.

[minisign](https://github.com/jedisct1/minisign) — маленькая утилита (как упрощённый PGP для файлов). В проекте используется пара ключей:

| Файл | Что это | Где хранить |
|------|---------|-------------|
| **`minisign.pub`** | Публичный ключ | Можно и **нужно** положить в репозиторий (например `release/minisign.pub`), чтобы пользователи проверяли подписи |
| **`minisign.key`** | Секретный ключ | **Только** у maintainer: локально + секрет GitHub Actions. **Никогда** не коммитить |

Аналогия: `.pub` — «печать, которой все могут сверить документ», `.key` — «сама печать, которой подписывают».

### Команда `minisign -G -p minisign.pub -s minisign.key`

Выполняется **один раз** на машине maintainer (после установки minisign):

```bash
minisign -G -p minisign.pub -s minisign.key
```

- **`-G`** — *generate*: создать новую пару ключей.
- **`-p minisign.pub`** — куда записать публичный ключ.
- **`-s minisign.key`** — куда записать секретный ключ.

Утилита спросит **парольную фразу** (passphrase) для секретного ключа — запомните её; без неё ключ на другом компьютере не использовать.

Установка minisign (примеры):

```bash
# Debian/Ubuntu
sudo apt install minisign

# macOS
brew install minisign

# или бинарник с https://github.com/jedisct1/minisign/releases
```

### Что сделать после генерации ключей

1. **Закоммитить только публичный ключ**, например:

   ```bash
   mkdir -p release
   mv minisign.pub release/minisign.pub
   git add release/minisign.pub
   git commit -m "chore: add release signing public key"
   ```

   Пользователи будут проверять: `./scripts/verify-release.sh release/minisign.pub dist`.

2. **Секретный ключ — в GitHub Actions** (не в git):

   - Откройте репозиторий → **Settings** → **Secrets and variables** → **Actions**.
   - **New repository secret**
   - Имя: **`MINISIGN_SECRET_KEY`**
   - Значение: **полное содержимое файла** `minisign.key` (текст, как в файле на диске).

   CI при релизе кладёт это во временный файл и вызывает `scripts/sign-release.sh`, который для каждого архива создаёт `имя.tar.gz.minisig`.

3. **`minisign.key` локально** — в безопасном месте (менеджер паролей, зашифрованный бэкап). Если ключ утерян, старые подписи проверяются старым `.pub`, новые релизы подписываются **новой** парой (и нужен новый `.pub` в репо).

### Если секрет не настроен

Workflow пишет в лог что-то вроде `MINISIGN_SECRET_KEY not configured — skipping signatures` и публикует только архивы и `SHA256SUMS`. Это нормально для первых релизов.

---

## Пользователь: скачал релиз, как проверить

### 1. Контрольная сумма (минимум)

Скачайте архив для своей платформы и файл `SHA256SUMS` с той же страницы Release:

```bash
sha256sum -c SHA256SUMS
```

Должно быть `OK` для вашего файла.

### 2. Подпись minisign (если на Release есть `.minisig`)

Скачайте рядом `skadicore-….tar.gz.minisig` (или `.zip.minisig`).

Публичный ключ — из репозитория (когда maintainer добавит `release/minisign.pub`) или со страницы релиза, если он туда выложен.

```bash
mkdir -p dist
mv ~/Downloads/skadicore-0.1.0-*.tar.gz dist/
mv ~/Downloads/skadicore-0.1.0-*.tar.gz.minisig dist/

./scripts/verify-release.sh release/minisign.pub dist
```

Скрипт для каждой подписи вызывает `minisign -V` и печатает `OK: …`.

### 3. Распаковка и запуск

```bash
tar -xzf dist/skadicore-0.1.0-x86_64-unknown-linux-musl.tar.gz
./skadicore --version
./skadicore check-config --config examples/reality-vless/server.toml
```

---

## Локальная сборка релиза (опционально)

Для отладки упаковки, без публикации:

```bash
./scripts/build-musl.sh   # или scripts/build-cross.sh для других target
./scripts/package-release.sh 0.1.1 x86_64-unknown-linux-musl
ls dist/
```

Подпись локально (если есть `minisign.key`):

```bash
export MINISIGN_SECRET_KEY="$(cat minisign.key)"
./scripts/sign-release.sh
```

---

## После релиза

- Новые фичи и фиксы — снова только в **`[Unreleased]`** в `CHANGELOG.md`.
- Патчи безопасности: новый тег `v0.1.x` с тем же процессом; политика — `SECURITY.md`.

---

## Частые проблемы

| Симптом | Что проверить |
|---------|----------------|
| Release упал на «Tag version != Cargo.toml» | Версия в теге и в `Cargo.toml` должны совпадать |
| aarch64 musl: `can't find crate for core` | В workflow должен быть шаг `rustup target add` (см. `release.yml`) |
| Нет `.minisig` на Release | Секрет `MINISIGN_SECRET_KEY` не задан или пустой |
| `verify-release.sh`: No .minisig files | Для этого релиза подпись не делалась — достаточно `SHA256SUMS` |
| `minisign -V` failed | Скачан не тот архив, битый файл или не тот `.pub` |

Если что-то в документе устарело, правьте этот файл в том же PR, что и изменения в `release.yml`.
