#!/usr/bin/env sh
# Git and Node are prerequisites. System installations require explicit approval.
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
exec node "$root/nexus.mjs" install "$@"
