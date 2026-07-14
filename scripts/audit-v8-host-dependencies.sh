#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

readonly TRUST_EXECUTOR="src/control_plane/tls/process_host_command_executor.rs"
readonly BROWSER_OPENER="src/cli/support/open_in_browser.rs"
readonly MACOS_TRUST="src/control_plane/tls/mac_os_certificate_trust_store.rs"
readonly DEBIAN_TRUST="src/control_plane/tls/debian_certificate_trust_store.rs"
readonly TRUST_TESTS="src/control_plane/tls/tests.rs"
violations=0

report_matches() {
  local heading="$1"
  local pattern="$2"
  local file="$3"
  local matches

  matches="$(grep -nE "$pattern" "$file" || true)"
  if [[ -z "$matches" ]]; then
    return
  fi

  printf '%s: %s\n%s\n' "$heading" "$file" "$matches" >&2
  violations=$((violations + 1))
}

audit_file() {
  local file="$1"

  if [[ "$file" != "$TRUST_EXECUTOR" && "$file" != "$BROWSER_OPENER" ]]; then
    report_matches \
      "direct host process execution is outside the trust-store boundary" \
      '(^|[^[:alnum:]_])(std::)?process::Command|use[[:space:]]+std::process.*Command' \
      "$file"
  fi

  case "$file" in
    "$MACOS_TRUST"|"$DEBIAN_TRUST"|"$TRUST_TESTS") ;;
    *)
      report_matches \
        "host commands may be constructed only by explicit trust-store adapters" \
        'HostCommand::new' \
        "$file"
      ;;
  esac

  report_matches \
    "strict v8 imports a legacy Docker, host-server, or database runtime" \
    'crate::(docker|serve|database)(::|[,{])' \
    "$file"

  if [[ "$file" != "src/cli/handlers/daemon_cmd/service.rs" ]]; then
    report_matches \
      "strict v8 imports the legacy per-project daemon runtime" \
      'crate::daemon(::|[,{])' \
      "$file"
  fi
}

while IFS= read -r -d '' file; do
  audit_file "$file"
done < <(find src/control_plane src/cli -type f -name '*.rs' -print0)

for removed in config database dependency_order display docker env parallel serve share swarm; do
  if [[ -e "src/$removed.rs" ]] \
    || [[ -d "src/$removed" && -n "$(find "src/$removed" -type f -print -quit)" ]]; then
    printf 'removed pre-v8 source tree still exists: src/%s\n' "$removed" >&2
    violations=$((violations + 1))
  fi
done

while IFS= read -r -d '' file; do
  report_matches \
    "v8 contains a removed Windows platform path" \
    'Windows|named[_ -]pipe|certutil|cfg\(windows\)|target_os[[:space:]]*=[[:space:]]*"windows"' \
    "$file"
done < <(find src docs .github -type f \( -name '*.rs' -o -name '*.md' -o -name '*.yml' -o -name '*.yaml' \) -print0)

if ((violations > 0)); then
  printf 'v8 host-dependency audit failed with %d violating file(s)\n' "$violations" >&2
  exit 1
fi

printf '%s\n' \
  "v8 host-dependency audit passed" \
  "allowed host process boundary: $TRUST_EXECUTOR" \
  "explicit browser-open boundary: $BROWSER_OPENER" \
  "normal v8 runtime boundary: Unix Engine API socket and local IPC"
