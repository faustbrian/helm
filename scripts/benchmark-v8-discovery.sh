#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if (( $# != 1 )); then
  printf 'usage: %s <new-output-directory>\n' "$0" >&2
  exit 64
fi

readonly OUTPUT_DIRECTORY="$1"
if [[ -e "$OUTPUT_DIRECTORY" ]]; then
  printf 'benchmark output already exists: %s\n' "$OUTPUT_DIRECTORY" >&2
  exit 73
fi

mkdir -p "$OUTPUT_DIRECTORY"
metadata="$OUTPUT_DIRECTORY/metadata.txt"
results="$OUTPUT_DIRECTORY/results.txt"
{
  printf 'started_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'stackctl_revision=%s\n' "$(git rev-parse HEAD)"
  printf 'host=%s\n' "$(uname -a)"
  printf 'rustc=%s\n' "$(rustc --version)"
  printf 'budget_multiplier=%s\n' "${STACKCTL_DISCOVERY_BUDGET_MULTIPLIER:-1}"
} > "$metadata"

temporary="$OUTPUT_DIRECTORY/results.txt.tmp"
if ! cargo test --release \
  discovery_performance_reports_and_enforces_1_10_40_project_budgets -- \
  --ignored --nocapture --test-threads=1 > "$temporary" 2>&1; then
  cat "$temporary" >&2
  mv "$temporary" "$results"
  printf 'failed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$metadata"
  exit 1
fi
mv "$temporary" "$results"
printf 'completed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$metadata"
printf 'wrote discovery benchmark evidence to %s\n' "$OUTPUT_DIRECTORY"
