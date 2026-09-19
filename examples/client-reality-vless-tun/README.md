# skadicore client — TUN + VLESS + REALITY

Linux-only: полный туннель через TUN с outbound REALITY (без Xray).

См. также [client-reality-vless](../client-reality-vless/) (только SOCKS5).

## Проверка конфига

```bash
skadicore client --config examples/client-reality-vless-tun/client.toml
# или dry-run через load в тестах; на сервере:
skadicore check-config --config examples/reality-vless/server.toml
```

## Важно

- `remote.server` — TCP-адрес прокси; `remote.reality.server_name` — SNI.
- `kex_mode` на клиенте и `transport.reality.kex_mode` на сервере должны совпадать.
- При `routing.auto = true` IP прокси автоматически уходит в bypass.
