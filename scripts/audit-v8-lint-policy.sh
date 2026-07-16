#!/usr/bin/env bash

set -euo pipefail

ROOT_DIRECTORY=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT_DIRECTORY"

failures=0

if ! grep -qE '^unused = \{ level = "deny", priority = -1 \}$' Cargo.toml; then
  printf '%s\n' 'Cargo.toml must deny unused production and test code' >&2
  failures=1
fi

report_matches() {
  local description=$1
  local pattern=$2
  shift 2

  local path
  local matches
  local grep_status
  for path in "$@"; do
    set +e
    matches="$(grep -R -nE "$pattern" "$path")"
    grep_status=$?
    set -e
    if (( grep_status == 0 )); then
      printf '%s\n%s\n' "$description" "$matches" >&2
      failures=1
    elif (( grep_status > 1 )); then
      printf 'lint policy audit could not scan: %s\n' "$path" >&2
      failures=1
    fi
  done
}

report_matches \
  'crate-wide compiler warning suppression is forbidden:' \
  '#!\[allow\([^]]*(dead_code|unused_imports)' \
  src

report_matches \
  'broad Clippy group suppression is forbidden:' \
  '#!\[allow\([^]]*clippy::(all|pedantic|nursery|cargo)' \
  src

report_matches \
  'build and run recipes must not disable compiler warnings:' \
  'RUSTFLAGS=.*-A ?warnings|RUSTFLAGS=.*--allow[= ]warnings' \
  justfile scripts .github

if (( failures != 0 )); then
  exit 1
fi

printf '%s\n' 'v8 lint policy audit passed'
