#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

ROOT="$(mktemp -d)"
trap 'rm -rf "${ROOT}"' EXIT INT TERM

mkdir -p "${ROOT}/valid" "${ROOT}/mutable" "${ROOT}/empty"
cat >"${ROOT}/valid/ci.yml" <<'YAML'
steps:
  - name: Checkout
    uses: actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd
  - name: Local action
    uses: ./local-action
YAML
cat >"${ROOT}/mutable/ci.yml" <<'YAML'
steps:
  - name: Checkout
    uses: actions/checkout@main
YAML

PATH=/usr/bin:/bin ./scripts/audit-v8-workflow-actions.sh "${ROOT}/valid"
if PATH=/usr/bin:/bin ./scripts/audit-v8-workflow-actions.sh \
  "${ROOT}/mutable" >/dev/null 2>&1; then
  printf '%s\n' 'workflow action audit accepted a mutable reference' >&2
  exit 1
fi
if PATH=/usr/bin:/bin ./scripts/audit-v8-workflow-actions.sh \
  "${ROOT}/empty" >/dev/null 2>&1; then
  printf '%s\n' 'workflow action audit accepted an empty scan' >&2
  exit 1
fi

PATH=/usr/bin:/bin ./scripts/audit-v8-lint-policy.sh
PATH=/usr/bin:/bin ./scripts/audit-v8-install-workflow.sh

cp .github/workflows/release.yml "${ROOT}/release.yml"
PATH=/usr/bin:/bin ./scripts/audit-v8-release-workflow.sh \
  "${ROOT}/release.yml"
sed '/actions\/attest-sbom@/d' "${ROOT}/release.yml" \
  >"${ROOT}/release-without-sbom.yml"
if PATH=/usr/bin:/bin ./scripts/audit-v8-release-workflow.sh \
  "${ROOT}/release-without-sbom.yml" >/dev/null 2>&1; then
  printf '%s\n' 'release workflow audit accepted missing SBOM attestation' >&2
  exit 1
fi

printf '%s\n' 'v8 policy audit regressions passed'
