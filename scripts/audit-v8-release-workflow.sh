#!/usr/bin/env bash

set -euo pipefail

readonly workflow="${1:-.github/workflows/release.yml}"

if [[ ! -f "${workflow}" ]]; then
  printf 'release workflow is not a regular file: %s\n' "${workflow}" >&2
  exit 1
fi

required_contract=(
  'x86_64-unknown-linux-gnu'
  'aarch64-unknown-linux-gnu'
  'x86_64-apple-darwin'
  'aarch64-apple-darwin'
  'cargo build --release --locked --target'
  'anchore/sbom-action@'
  'actions/attest-build-provenance@'
  'actions/attest-sbom@'
  'gh attestation verify'
  'https://slsa.dev/provenance/v1'
  'https://spdx.dev/Document/v2.3'
  'sha256sum --check SHA256SUMS'
  'gh release create'
)

failed=0
for requirement in "${required_contract[@]}"; do
  if ! grep -Fq -- "${requirement}" "${workflow}"; then
    printf 'release workflow is missing required contract: %s\n' \
      "${requirement}" >&2
    failed=1
  fi
done

if (( failed != 0 )); then
  exit 1
fi

printf 'v8 release workflow audit passed\n'
