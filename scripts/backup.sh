#!/usr/bin/env bash
# Archive the ntfy state tree (config + message/auth db) to a timestamped
# tarball. Mirrors the orca `ntfy.backup` tool.
#
#   ./backup.sh <state-path> <destination-dir>
set -euo pipefail

STATE_PATH="${1:-/opt/ntfy}"
DESTINATION="${2:?destination directory required}"

if [ ! -d "${STATE_PATH}" ]; then
  echo "state path '${STATE_PATH}' is not a directory" >&2
  exit 1
fi

mkdir -p "${DESTINATION}"
STAMP="$(date -u +%Y%m%d-%H%M%S)"
ARCHIVE="${DESTINATION%/}/ntfy-state-${STAMP}.tar.gz"
tar -czf "${ARCHIVE}" -C "${STATE_PATH}" .
echo "${ARCHIVE}"
