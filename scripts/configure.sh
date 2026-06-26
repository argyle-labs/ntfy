#!/usr/bin/env bash
# Register a deployed ntfy endpoint with orca so the notifications domain can
# route to it. This is a convenience wrapper over the orca CLI; the canonical
# path is the `ntfy.create` tool.
#
#   ./configure.sh <name> <base-url> <topic> [token]
set -euo pipefail

NAME="${1:?endpoint name required}"
BASE_URL="${2:?base url required, e.g. http://127.0.0.1:80}"
TOPIC="${3:?topic required}"
TOKEN="${4:-}"

ARGS=(--name "${NAME}" --base-url "${BASE_URL}" --topic "${TOPIC}" --enabled true)
if [ -n "${TOKEN}" ]; then
  ARGS+=(--token "${TOKEN}")
fi

orca tool ntfy.create "${ARGS[@]}"
echo "registered ntfy endpoint '${NAME}' -> ${BASE_URL}/${TOPIC}"
