# Примеры конфигурации SkadiCore

Готовые сценарии для локальной проверки и как шаблоны для продакшена.

| Каталог | Сценарий | Транспорт |
|---------|----------|-----------|
| [reality-vless/](reality-vless/) | VLESS + REALITY (рекомендуется) | REALITY |
| [reality-xhttp-vless/](reality-xhttp-vless/) | VLESS + REALITY + XHTTP | REALITY + XHTTP |
| [awg-vpn/](awg-vpn/) | AmneziaWG VPN (UDP) | AmneziaWG |
| [vless-tls/](vless-tls/) | VLESS поверх TLS | TLS 1.3 |
| [socks5-tls/](socks5-tls/) | SOCKS5 user-pass поверх TLS | TLS 1.3 |

Запуск из корня репозитория:

```bash
cargo run --bin skadicore -- --config examples/<name>/server.toml
```

Проверка без запуска:

```bash
cargo run --bin skadicore -- check-config --config examples/<name>/server.toml
```

Для TLS-примеров сначала сгенерируйте сертификаты (`./generate-certs.sh` в каталоге примера).
