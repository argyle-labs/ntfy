# ntfy

Push notification server — send notifications to phones and desktops via HTTP.

**Status:** running — Docker on `<ip>:8080`

- **Host**: `<host>` (`<ip>`)
- **Port**: 8080
- **Public URL**: `ntfy.<domain>` (fronted by a reverse proxy such as Caddy)

## Notes

Used by other services (backups, monitoring, automations) to publish push notifications to subscribed topics. Fronted by a reverse proxy (e.g. Caddy) for TLS at `ntfy.<domain>`.
