# AmneziaWG VPN

См. [индекс примеров](../README.md).

**AmneziaWG** — обфусцированный WireGuard (UDP + junk packets + magic headers).
SkadiCore поднимает интерфейс через [`amneziawg-go`](https://github.com/amnezia-vpn/amneziawg-go)
и применяет конфиг через `awg setconf`.

Это **отдельный режим** от VLESS/REALITY: полноценный L3 VPN, не stream-прокси.

## Зависимости

| Компонент | Назначение |
|-----------|------------|
| `amneziawg-go` | Userspace WireGuard + AWG obfuscation |
| `awg` (amneziawg-tools) | `awg setconf` для настройки интерфейса |

Переменные окружения (если бинарники не в `PATH`):

- `AWG_GO_BINARY` — путь к `amneziawg-go`
- `AWG_TOOLS_BINARY` — путь к `awg`

## 1. Ключи

```bash
# Сервер
cargo run --bin skadicore -- genkey awg

# Клиент (отдельная пара)
cargo run --bin skadicore -- genkey awg
```

| Сервер `server.toml` | Клиент `client.conf` |
|----------------------|----------------------|
| `private_key` | `PublicKey` в `[Peer]` |
| `[[transport.awg.peers]].public_key` | `PrivateKey` в `[Interface]` |
| `h1`…`h4`, `jc`, `s*` | те же значения в `[Interface]` |

## 2. Запуск

```bash
sudo cargo run --bin skadicore -- --config examples/awg-vpn/server.toml
```

На сервере включите IP forwarding и NAT (пример для Linux):

```bash
sudo sysctl -w net.ipv4.ip_forward=1
sudo iptables -t nat -A POSTROUTING -s 10.8.0.0/24 -o eth0 -j MASQUERADE
```

Клиент: импортируйте `client.conf` в AmneziaVPN или `awg-quick up client.conf`.

## 3. Проверка конфига

```bash
cargo run --bin skadicore -- check-config --config examples/awg-vpn/server.toml
```

## 4. Тесты

```bash
cargo test -p skadi-server --test awg_config
cargo test -p skadi-transport --test awg_render
```
