# Примеры конфигурации SkadiCore

Готовые сценарии для локальной проверки и как шаблоны для продакшена.

| Каталог | Сценарий | Транспорт |
|---------|----------|-----------|
| [reality-vless/](reality-vless/) | VLESS + REALITY (рекомендуется) | REALITY |
| [client-reality-vless/](client-reality-vless/) | **Клиент** SOCKS5 → VLESS+REALITY (native) | REALITY |
| [client-reality-vless-tun/](client-reality-vless-tun/) | **Клиент** TUN → VLESS+REALITY (Linux) | REALITY |
| [reality-xhttp-vless/](reality-xhttp-vless/) | VLESS + REALITY + XHTTP (сервер; клиент Xray) | REALITY + XHTTP |
| [client-reality-xhttp-vless/](client-reality-xhttp-vless/) | **Клиент** SOCKS5 → VLESS+REALITY+XHTTP (native) | REALITY + XHTTP |
| [awg-vpn/](awg-vpn/) | AmneziaWG VPN (UDP) | AmneziaWG |
| [hysteria2/](hysteria2/) | Hysteria2 QUIC proxy (сервер) | Hysteria2 |
| [client-hysteria2/](client-hysteria2/) | **Клиент** SOCKS5 → Hysteria2 (binary) | Hysteria2 |
| [tuic/](tuic/) | TUIC QUIC proxy (сервер) | TUIC |
| [client-tuic/](client-tuic/) | **Клиент** SOCKS5 → TUIC (binary) | TUIC |
| [vless-tls/](vless-tls/) | VLESS поверх TLS | TLS 1.3 |
| [socks5-tls/](socks5-tls/) | SOCKS5 user-pass поверх TLS | TLS 1.3 |
| [docker-xray/](docker-xray/) | CI e2e: примеры × Xray-core в Docker | — |

Запуск из корня репозитория:

```bash
cargo run --bin skadicore -- --config examples/<name>/server.toml
```

Проверка без запуска:

```bash
cargo run --bin skadicore -- check-config --config examples/<name>/server.toml
```

Совместимость с Xray (REALITY / VLESS-TLS / REALITY+XHTTP):

```bash
./scripts/examples-xray-docker-e2e.sh
```

Для TLS-примеров сначала сгенерируйте сертификаты (`./generate-certs.sh` в каталоге примера).
