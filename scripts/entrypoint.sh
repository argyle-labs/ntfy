#!/usr/bin/env bash
# Container entrypoint for the wrapper image. Delegates to the official ntfy
# binary; the bundled backup/restore helpers are installed alongside in
# /usr/local/bin for ad-hoc state management.
set -euo pipefail
exec ntfy "${@:-serve}"
