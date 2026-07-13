#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

readonly TRUST_EXECUTOR="src/control_plane/tls/process_host_command_executor.rs"
readonly MACOS_TRUST="src/control_plane/tls/mac_os_certificate_trust_store.rs"
readonly WINDOWS_TRUST="src/control_plane/tls/windows_certificate_trust_store.rs"
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

  if [[ "$file" != "$TRUST_EXECUTOR" ]]; then
    report_matches \
      "direct host process execution is outside the trust-store boundary" \
      '(^|[^[:alnum:]_])(std::)?process::Command|use[[:space:]]+std::process.*Command' \
      "$file"
  fi

  case "$file" in
    "$MACOS_TRUST"|"$WINDOWS_TRUST"|"$DEBIAN_TRUST"|"$TRUST_TESTS") ;;
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
done < <(find src/control_plane src/cli/handlers/daemon_cmd -type f -name '*.rs' -print0)

while IFS= read -r -d '' file; do
  audit_file "$file"
done < <(find src/cli/handlers -maxdepth 1 -type f -name 'v8_*.rs' -print0)

if ((violations > 0)); then
  printf 'v8 host-dependency audit failed with %d violating file(s)\n' "$violations" >&2
  exit 1
fi

printf '%s\n' \
  "v8 host-dependency audit passed" \
  "allowed host process boundary: $TRUST_EXECUTOR" \
  "normal v8 runtime boundary: typed Engine API and local IPC"
