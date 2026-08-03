# ntfy

Push notification server — send notifications to phones and desktops via HTTP.

**Status:** running — Docker on `<ip>:80`

- **Host**: `<host>` (`<ip>`)
- **Port**: 80 (as mapped in [`compose.yml`](../compose.yml))
- **Public URL**: front with a reverse proxy (e.g. Caddy) for TLS

## Notes

Publishes push notifications to subscribed topics over HTTP — useful for wiring
up backup jobs, monitoring, and automations to send alerts. Set `NTFY_BASE_URL`
in `compose.yml` to your public/base URL, and put a reverse proxy in front for
TLS if exposing it beyond the host.
