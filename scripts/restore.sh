#!/usr/bin/env bash
# Restore the ntfy state tree from a tarball produced by backup.sh. Mirrors the
# orca `ntfy.restore` tool. Stop the container before restoring.
#
#   ./restore.sh <tarball> <state-path>
set -euo pipefail

FROM="${1:?backup tarball required}"
STATE_PATH="${2:-/opt/ntfy}"

if [ ! -f "${FROM}" ]; then
  echo "backup tarball '${FROM}' not found" >&2
  exit 1
fi

mkdir -p "${STATE_PATH}"
tar -xzf "${FROM}" -C "${STATE_PATH}"
echo "restored ${FROM} into ${STATE_PATH}"
