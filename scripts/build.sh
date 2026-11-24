#!/usr/bin/env bash
# Compatibility wrapper. All setup/build behavior lives in the shared Node CLI.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
command_name=package
args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --fast) command_name=build; shift ;;
    *) args+=("$1"); shift ;;
  esac
done
if [[ "$command_name" == package ]]; then args=(--profile personal "${args[@]}"); fi
exec node "$root/nexus.mjs" "$command_name" "${args[@]}"
