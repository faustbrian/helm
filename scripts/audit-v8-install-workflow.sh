#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

recipe="$({
  awk '
    /^install:$/ { inside = 1; next }
    inside && /^[[:alnum:]_-]+:/ { exit }
    inside { print }
  ' justfile
})"

install_line="$(printf '%s\n' "${recipe}" | grep -n -E \
  '^[[:space:]]+cargo install --path \. --locked$' | cut -d: -f1)"
restart_line="$(printf '%s\n' "${recipe}" | grep -n -E \
  '^[[:space:]]+stackctl daemon service restart --if-installed$' | cut -d: -f1)"

if [[ -z "${install_line}" || -z "${restart_line}" ]]; then
  printf '%s\n' \
    'install recipe must use the lockfile and restart an installed daemon' >&2
  exit 1
fi
if (( restart_line <= install_line )); then
  printf '%s\n' 'daemon restart must follow binary replacement' >&2
  exit 1
fi

printf '%s\n' 'v8 install workflow policy passed'
