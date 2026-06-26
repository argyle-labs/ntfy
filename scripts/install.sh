#!/usr/bin/env bash
# Provision a self-hosted ntfy server via Docker Compose. Mirrors what the
# orca `ntfy.install` tool shells out to; usable standalone via curl-bootstrap.
#
#   ./install.sh [compose-file]
#
# Defaults to ./compose.yml in the current directory.
set -euo pipefail

COMPOSE_FILE="${1:-compose.yml}"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required but not installed" >&2
  exit 1
fi

mkdir -p ./state/etc ./state/cache
docker compose -f "${COMPOSE_FILE}" up -d
echo "ntfy provisioned from ${COMPOSE_FILE}"
