#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

usage() {
  printf '%s\n' \
    "usage: $0 <scenario> <new-output-directory> [sample-count] [interval-seconds]" \
    "" \
    "scenarios:" \
    "  v8-one" \
    "  v8-forty-compatible" \
    "  v8-forty-split" \
    "" \
    "environment:" \
    "  STACKCTL_BIN                         stackctl executable" \
    "  STACKCTL_BENCHMARK_ENGINE            Engine product" \
    "  STACKCTL_BENCHMARK_ENGINE_VERSION    exact Engine version" \
    "  STACKCTL_BENCHMARK_ENGINE_BACKEND    VM/backend identity" \
    "  STACKCTL_BENCHMARK_ENGINE_LIMITS     CPU and memory limits" \
    "  STACKCTL_BENCHMARK_FILESYSTEM        sharing/filesystem mode" \
    "  STACKCTL_BENCHMARK_HOST_METRICS_FILE required external host/VM samples"
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if (( $# < 2 || $# > 4 )); then
  usage >&2
  exit 64
fi

readonly SCENARIO="$1"
readonly OUTPUT_DIRECTORY="$2"
readonly SAMPLE_COUNT="${3:-12}"
readonly INTERVAL_SECONDS="${4:-5}"
readonly STACKCTL_BIN="${STACKCTL_BIN:-target/release/stackctl}"

case "$SCENARIO" in
  v8-one|v8-forty-compatible|v8-forty-split) ;;
  *)
    printf 'unsupported benchmark scenario: %s\n' "$SCENARIO" >&2
    usage >&2
    exit 64
    ;;
esac

if [[ ! "$SAMPLE_COUNT" =~ ^[1-9][0-9]*$ ]]; then
  printf 'sample count must be a positive integer\n' >&2
  exit 64
fi
if [[ ! "$INTERVAL_SECONDS" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  printf 'interval seconds must be zero or a positive number\n' >&2
  exit 64
fi
if [[ -e "$OUTPUT_DIRECTORY" ]]; then
  printf 'benchmark output already exists: %s\n' "$OUTPUT_DIRECTORY" >&2
  exit 73
fi
if [[ ! -x "$STACKCTL_BIN" ]]; then
  printf 'stackctl benchmark executable is not runnable: %s\n' "$STACKCTL_BIN" >&2
  exit 69
fi

required_environment=(
  STACKCTL_BENCHMARK_ENGINE
  STACKCTL_BENCHMARK_ENGINE_VERSION
  STACKCTL_BENCHMARK_ENGINE_BACKEND
  STACKCTL_BENCHMARK_ENGINE_LIMITS
  STACKCTL_BENCHMARK_FILESYSTEM
  STACKCTL_BENCHMARK_HOST_METRICS_FILE
)
for name in "${required_environment[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    printf 'required benchmark environment is missing: %s\n' "$name" >&2
    exit 64
  fi
done
if [[ ! -f "$STACKCTL_BENCHMARK_HOST_METRICS_FILE" ]]; then
  printf 'host metrics file does not exist: %s\n' \
    "$STACKCTL_BENCHMARK_HOST_METRICS_FILE" >&2
  exit 66
fi

mkdir -p "$OUTPUT_DIRECTORY/samples"

metadata="$OUTPUT_DIRECTORY/metadata.txt"
{
  printf 'scenario=%s\n' "$SCENARIO"
  printf 'started_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'stackctl_revision=%s\n' "$(git rev-parse HEAD)"
  printf 'stackctl_version=%s\n' "$("$STACKCTL_BIN" --version)"
  printf 'host=%s\n' "$(uname -a)"
  printf 'engine=%s\n' "$STACKCTL_BENCHMARK_ENGINE"
  printf 'engine_version=%s\n' "$STACKCTL_BENCHMARK_ENGINE_VERSION"
  printf 'engine_backend=%s\n' "$STACKCTL_BENCHMARK_ENGINE_BACKEND"
  printf 'engine_limits=%s\n' "$STACKCTL_BENCHMARK_ENGINE_LIMITS"
  printf 'filesystem=%s\n' "$STACKCTL_BENCHMARK_FILESYSTEM"
  printf 'sample_count=%s\n' "$SAMPLE_COUNT"
  printf 'interval_seconds=%s\n' "$INTERVAL_SECONDS"
} > "$metadata"

cp "$STACKCTL_BENCHMARK_HOST_METRICS_FILE" "$OUTPUT_DIRECTORY/host-metrics.txt"

for ((sample = 1; sample <= SAMPLE_COUNT; sample++)); do
  file="$(printf '%s/samples/%03d.json' "$OUTPUT_DIRECTORY" "$sample")"
  "$STACKCTL_BIN" daemon benchmark > "$file"
  if (( sample < SAMPLE_COUNT )); then
    sleep "$INTERVAL_SECONDS"
  fi
done

printf 'completed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$metadata"
printf 'wrote immutable raw benchmark samples to %s\n' "$OUTPUT_DIRECTORY"
