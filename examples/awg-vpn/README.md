# AmneziaWG VPN

См. [индекс примеров](../README.md).

**AmneziaWG** — обфусцированный WireGuard (UDP + junk packets + magic headers).
SkadiCore поднимает интерфейс через [`amneziawg-go`](https://github.com/amnezia-vpn/amneziawg-go)
и применяет конфиг через `awg setconf`.

Это **отдельный режим** от VLESS/REALITY: полноценный L3 VPN, не stream-прокси.

Файлы в этой папке содержат **плейсхолдеры** (`<server-private-key>` и т.д.).
Перед запуском подставьте реальные ключи из `genkey awg`. `check-config` для примера
проверяет структуру TOML и обфускацию, но не валидирует плейсхолдеры как WireGuard-ключи.

## Зависимости

| Компонент | Назначение |
|-----------|------------|
| `amneziawg-go` | Userspace WireGuard + AWG obfuscation |
| `awg` (amneziawg-tools) | `awg setconf` для настройки интерфейса |

Переменные окружения (если бинарники не в `PATH`):

- `AWG_GO_BINARY` — путь к `amneziawg-go`
- `AWG_TOOLS_BINARY` — путь к `awg`

## 1. Ключи

Сгенерируйте **две независимые пары** (сервер и клиент):

```bash
# Сервер: сохраните вывод (private + public)
cargo run --bin skadicore -- genkey awg

# Клиент: вторая пара
cargo run --bin skadicore -- genkey awg
```

Публичный ключ сервера из приватного (если `genkey` выводит только private):

```bash
# В выводе genkey обычно есть оба; иначе используйте awg pubkey < private.key
```

### Куда что вставить

| Значение | `server.toml` | `client.toml` / `client.conf` |
|----------|---------------|-------------------------------|
| Приватный ключ **сервера** | `transport.awg.private_key` | — |
| Публичный ключ **сервера** | — (вычисляется из private) | `server_public_key` / `[Peer] PublicKey` |
| Приватный ключ **клиента** | — | `private_key` / `[Interface] PrivateKey` |
| Публичный ключ **клиента** | `[[transport.awg.peers]].public_key` | — |

Параметры `jc`, `jmin`, `jmax`, `s1`–`s4`, `h1`–`h4` должны **совпадать** на сервере и клиенте.

## 2. Запуск

После подстановки ключей:

```bash
sudo cargo run --bin skadicore -- --config examples/awg-vpn/server.toml
```

### NAT (автоматически)

Включите в `server.toml`:

```toml
[transport.awg.nat]
enabled = true
subnet = "10.8.0.0/24"
egress_interface = "eth0"  # опционально
```

SkadiCore применит `sysctl net.ipv4.ip_forward=1` и `iptables MASQUERADE`.

### Экспорт клиентского конфига

Удобнее, чем править `client.conf` вручную — сервер сам подставит obfuscation и peer:

```bash
cargo run --bin skadicore -- export-awg-client \
  --config examples/awg-vpn/server.toml \
  --peer 0 \
  --client-key "<client-private-key>" \
  --endpoint "vpn.example.com:51820" \
  -o client.conf
```

(`export-awg-client` требует **реальных** ключей в `server.toml`, не плейсхолдеров.)

### Клиент SkadiCore

```bash
sudo cargo run --bin skadicore -- client --config examples/awg-vpn/client.toml
```

Или импортируйте `client.conf` в AmneziaVPN / `awg-quick up client.conf`.

## 3. Проверка конфига

Структура примера (с плейсхолдерами):

```bash
cargo run --bin skadicore -- check-config --config examples/awg-vpn/server.toml
```

Полная проверка WireGuard-ключей — после замены плейсхолдеров на ключи из `genkey awg`.

## 4. Тесты

```bash
cargo test -p skadi-server --test awg_config
cargo test -p skadi-transport --test awg_render
```
