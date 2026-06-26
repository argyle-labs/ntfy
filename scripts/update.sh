#!/usr/bin/env bash
# Pull the latest ntfy image and recreate the container. Mirrors the orca
# `ntfy.upgrade` tool.
#
#   ./update.sh [compose-file] [tag]
set -euo pipefail

COMPOSE_FILE="${1:-compose.yml}"
TAG="${2:-latest}"

docker pull "binwiederhier/ntfy:${TAG}"
docker compose -f "${COMPOSE_FILE}" up -d
echo "ntfy updated to binwiederhier/ntfy:${TAG}"
