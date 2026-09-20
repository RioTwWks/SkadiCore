# Обход троттлинга ТСПУ (вне SkadiCore)

SkadiCore — **userspace TCP/UDP-прокси**. Он не делает DPI-фрагментацию
на уровне пакетов (`fake` / `multisplit` / `hostfakesplit` и т.п.): для этого
нужны raw sockets, NFQUEUE или `SO_ORIGINAL_DST`, чего в ядре прокси нет и
**не планируется** (см. `.cursor/priorities.md`).

Если провайдер/ТСПУ **троттлит** уже установленные TLS/REALITY/XHTTP-сессии,
маскировки одного протокола может не хватить. Ниже — клиентские утилиты
*рядом* со SkadiCore, а не вместо него.

## Рекомендуемый порядок

1. **Сменить транспорт в SkadiCore** (часто достаточно):
   - REALITY + **XHTTP** (`examples/reality-xhttp-vless/`, `client-reality-xhttp-vless/`)
   - **Hysteria2 / TUIC** (QUIC/UDP) — другой профиль трафика
   - **AmneziaWG** — обфусцированный WireGuard
2. Если TCP всё ещё режут — поставить **локальный DPI-обход** перед браузером
   или перед `skadicore client` (цепочка ниже).
3. Не смешивать NFQUEUE-обход с TUN SkadiCore на одном хосте без понимания
   маршрутизации — легко получить петли и утечки.

## Клиентские утилиты

| Утилита | Роль | Заметки |
|---------|------|---------|
| [zapret](https://github.com/bol-van/zapret) | nfqws / tpws, fooling, split | Linux; часто вместе с iptables/nftables |
| [ByeDPI](https://github.com/hufrea/byedpi) | userspace desync (Windows/Linux) | Удобен как локальный SOCKS/HTTP перед приложением |
| [SpoofDPI](https://github.com/xvzc/SpoofDPI) | простой TLS ClientHello split | Лёгкий SOCKS; меньше опций, чем zapret |

SkadiCore **не вендорит** и не запускает эти бинарники. Держите их как
отдельный системный сервис или контейнер.

### Типичные схемы

**A. Приложение → ByeDPI/SpoofDPI (SOCKS) → skadicore client (SOCKS) → сервер**

```text
browser --socks5h--> byedpi:1080 --socks5h--> skadicore client:10808 --> VLESS+REALITY
```

Имеет смысл, если режут именно «первый» TLS к прокси. DNS — только через
`socks5h` / `--socks5-hostname` на каждом hop.

**B. skadicore client (TUN) + zapret на хосте**

TUN перехватывает трафик интерфейса; zapret/nfqws трогает пакеты до/после.
Настраивайте bypass для IP самого прокси и DNS (DoH/DoT в TUN — см.
`docs/CONFIGURATION.md` §`[client.tun.dns]`). Без bypass возможны петли.

**C. Только смена транспорта SkadiCore**

Предпочтительно, если троттлинг завязан на TCP fingerprint / длинные
потоки: XHTTP или QUIC (Hy2/TUIC) часто снимают проблему без NFQUEUE.

## Чего не делать в ядре SkadiCore

- Не добавлять NFQUEUE / `SO_ORIGINAL_DST` «для sonicdpi» в MVP.
- Не эмулировать `fake`+`fooling` в userspace copy_bidirectional — это не
  работает на уже зашифрованном TLS без перехвата на netfilter.
- Не отключать REALITY/TLS verify ради «обхода».

## IPv6

ТСПУ часто **паритетен** по IPv6. Слушайте dual-stack на сервере
(`listen = ["0.0.0.0:443", "[::]:443"]`) и используйте клиентские примеры
с `[IPv6]:port` (`examples/client-reality-vless/client-ipv6.toml`).
Иначе клиенты уйдут на IPv6 мимо туннеля — см. `docs/RISKS.md` §IPv6-утечки.

## Ссылки

- [sonicdpi / DPI evasion notes](https://github.com/by-sonic/sonicdpi) — контекст методов ТСПУ
- `docs/RISKS.md` — модель угроз
- `examples/README.md` — транспорты SkadiCore
