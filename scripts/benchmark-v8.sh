#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

usage() {
  printf '%s\n' \
    "usage: $0 <scenario> <new-output-directory> [sample-count] [interval-seconds]" \
    "" \
    "scenarios:" \
    "  baseline-engine-idle" \
    "  baseline-one" \
    "  baseline-forty" \
    "  v8-one" \
    "  v8-forty-compatible" \
    "  v8-forty-split" \
    "" \
    "environment:" \
    "  STACKCTL_BIN                         stackctl executable" \
    "  STACKCTL_BENCHMARK_RUN_ID            shared identity for all scenarios" \
    "  STACKCTL_BENCHMARK_COLLECTOR         host/VM collector and version" \
    "  STACKCTL_BENCHMARK_ENGINE            Engine product" \
    "  STACKCTL_BENCHMARK_ENGINE_VERSION    exact Engine version" \
    "  STACKCTL_BENCHMARK_ENGINE_BACKEND    VM/backend identity" \
    "  STACKCTL_BENCHMARK_ENGINE_LIMITS     CPU and memory limits" \
    "  STACKCTL_BENCHMARK_FILESYSTEM        sharing/filesystem mode" \
    "  STACKCTL_BENCHMARK_HOST_METRICS_FILE required external host/VM samples" \
    "  STACKCTL_BENCHMARK_EXTERNAL_INVENTORY_FILE" \
    "                                        required for baseline scenarios"
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
readonly STACKCTL_BIN="${STACKCTL_BIN:-target/release/stackctl}"

case "$SCENARIO" in
  baseline-engine-idle|baseline-one|baseline-forty)
    if (( $# != 2 )); then
      printf 'baseline scenarios do not accept daemon sample arguments\n' >&2
      exit 64
    fi
    readonly SCENARIO_MODE="baseline"
    readonly SAMPLE_COUNT=0
    readonly INTERVAL_SECONDS=0
    ;;
  v8-one|v8-forty-compatible|v8-forty-split)
    readonly SCENARIO_MODE="v8"
    readonly EVIDENCE_SCENARIO="$SCENARIO"
    readonly SAMPLE_COUNT="${3:-12}"
    readonly INTERVAL_SECONDS="${4:-5}"
    ;;
  *)
    printf 'unsupported benchmark scenario: %s\n' "$SCENARIO" >&2
    usage >&2
    exit 64
    ;;
esac

if [[ "$SCENARIO_MODE" == "v8" && ! "$SAMPLE_COUNT" =~ ^[1-9][0-9]*$ ]]; then
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
if [[ "$SCENARIO_MODE" == "v8" && ! -x "$STACKCTL_BIN" ]]; then
  printf 'stackctl benchmark executable is not runnable: %s\n' "$STACKCTL_BIN" >&2
  exit 69
fi
if [[ "$SCENARIO_MODE" == "baseline" ]]; then
  if [[ -z "${STACKCTL_BENCHMARK_EXTERNAL_INVENTORY_FILE:-}" ]]; then
    printf '%s\n' \
      'required benchmark environment is missing: STACKCTL_BENCHMARK_EXTERNAL_INVENTORY_FILE' \
      >&2
    exit 64
  fi
  if [[ ! -s "$STACKCTL_BENCHMARK_EXTERNAL_INVENTORY_FILE" ]]; then
    printf 'external inventory file does not exist or is empty: %s\n' \
      "$STACKCTL_BENCHMARK_EXTERNAL_INVENTORY_FILE" >&2
    exit 66
  fi
fi

required_environment=(
  STACKCTL_BENCHMARK_RUN_ID
  STACKCTL_BENCHMARK_COLLECTOR
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
  if [[ "${!name}" == *$'\n'* || "${!name}" == *$'\r'* ]]; then
    printf 'benchmark environment contains a line break: %s\n' "$name" >&2
    exit 64
  fi
done
if [[ ! "$STACKCTL_BENCHMARK_RUN_ID" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
  printf 'benchmark run ID must contain only letters, digits, dot, underscore, or hyphen\n' >&2
  exit 64
fi
if [[ ! -s "$STACKCTL_BENCHMARK_HOST_METRICS_FILE" ]]; then
  printf 'host metrics file does not exist or is empty: %s\n' \
    "$STACKCTL_BENCHMARK_HOST_METRICS_FILE" >&2
  exit 66
fi

mkdir -p "$OUTPUT_DIRECTORY/samples"

metadata="$OUTPUT_DIRECTORY/metadata.txt"
{
  printf 'scenario=%s\n' "$SCENARIO"
  printf 'benchmark_run_id=%s\n' "$STACKCTL_BENCHMARK_RUN_ID"
  printf 'started_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'stackctl_revision=%s\n' "$(git rev-parse HEAD)"
  if [[ "$SCENARIO_MODE" == "v8" ]]; then
    printf 'stackctl_version=%s\n' "$("$STACKCTL_BIN" --version)"
  else
    printf 'stackctl_version=not-applicable\n'
  fi
  printf 'host=%s\n' "$(uname -a)"
  printf 'host_metrics_collector=%s\n' "$STACKCTL_BENCHMARK_COLLECTOR"
  printf 'engine=%s\n' "$STACKCTL_BENCHMARK_ENGINE"
  printf 'engine_version=%s\n' "$STACKCTL_BENCHMARK_ENGINE_VERSION"
  printf 'engine_backend=%s\n' "$STACKCTL_BENCHMARK_ENGINE_BACKEND"
  printf 'engine_limits=%s\n' "$STACKCTL_BENCHMARK_ENGINE_LIMITS"
  printf 'filesystem=%s\n' "$STACKCTL_BENCHMARK_FILESYSTEM"
  printf 'sample_count=%s\n' "$SAMPLE_COUNT"
  printf 'interval_seconds=%s\n' "$INTERVAL_SECONDS"
} > "$metadata"

cp "$STACKCTL_BENCHMARK_HOST_METRICS_FILE" "$OUTPUT_DIRECTORY/host-metrics.txt"

if [[ "$SCENARIO_MODE" == "baseline" ]]; then
  cp "$STACKCTL_BENCHMARK_EXTERNAL_INVENTORY_FILE" \
    "$OUTPUT_DIRECTORY/external-runtime-inventory.txt"
  printf 'completed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
    >> "$metadata"
  printf 'wrote immutable raw baseline evidence to %s\n' "$OUTPUT_DIRECTORY"
  exit 0
fi

for ((sample = 1; sample <= SAMPLE_COUNT; sample++)); do
  file="$(printf '%s/samples/%03d.json' "$OUTPUT_DIRECTORY" "$sample")"
  temporary="${file}.tmp"
  if ! "$STACKCTL_BIN" daemon benchmark \
    --evidence-scenario "$EVIDENCE_SCENARIO" > "$temporary"; then
    rm -f "$temporary"
    exit 1
  fi
  mv "$temporary" "$file"
  if (( sample < SAMPLE_COUNT )); then
    sleep "$INTERVAL_SECONDS"
  fi
done

printf 'completed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$metadata"
printf 'wrote immutable raw benchmark samples to %s\n' "$OUTPUT_DIRECTORY"
