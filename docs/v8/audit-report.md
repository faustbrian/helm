# Stackctl v8 release-readiness audit report

## Recommendation

Do not publish v8 as a supported release yet.

The repository-owned implementation and regression work described below is in
place, and no known Critical or High implementation finding remains open.
Release readiness is still unproven until the exact release revision passes all
defined CI acceptance jobs and the platform owners attach the required
physical-host and controlled-benchmark records. Preview documentation must not
be presented as a support claim.

This report audits the implementation baseline through `c95d260`, including
the automatic-lock correction, plus the command-reference and report changes
that contain this document. Evidence ownership follows
[External verification](external-verification.md); absence of an external host
never converts a pending check into a pass.

## Fixed findings

The detailed root cause, missing test layer, fix, and evidence for every item
is maintained in the [failure inventory](failure-inventory.md).

| Severity | Findings fixed in the repository |
| --- | --- |
| Critical | FI-01 automatic declarative restores; FI-02 reset rollback; FI-21 one-command setup and automatic missing artifact locks |
| High | FI-03 exact streamed-command failures; FI-04 responsive queued restores; FI-05 operational service readiness; FI-06 Laravel `/up` health; FI-07 bounded discovery; FI-08 public runtime images; FI-09 current Laravel environment; FI-10 `~/.stackctl` state; FI-11 stable exact CA trust; FI-12 bounded stable resource names; FI-13 workload identity and project networks; FI-15 persistent diagnostics; FI-17 clean Laravel acceptance; FI-18 bounded YAML; FI-19 dropped workload capabilities; FI-20 mutually exclusive workflow triggers |
| Medium | FI-14 host-port diagnosis; FI-16 warning and dependency gates; FI-22 singleton-owner diagnostics; FI-23 explicit service restart |

The threat analysis and mitigations, including automatic project artifact
publication, are in the [threat model](security.md). The evidence-backed state
of each product acceptance criterion is in the
[completion audit](completion-audit.md).

## Local verification

These checks ran on macOS arm64 against the audited tree on 2026-07-16.

| Command | Result |
| --- | --- |
| `just lint` | Passed: lint-policy audit, nightly formatting check, and all-target/all-feature Clippy with no warnings |
| `just build` | Passed: optimized v8 binary built successfully |
| `cargo test --quiet` | Passed: 880 tests, 0 failed, and 24 explicitly ignored CI-owned live tests |
| `./scripts/audit-v8-host-dependencies.sh` | Passed: no removed host-runtime, unsupported-host, or compatibility boundary was found |
| `./scripts/audit-v8-workflow-actions.sh` | Passed: every external workflow action uses an immutable 40-character commit |
| `./scripts/audit-v8-supply-chain.sh` | Passed: 224 locked dependencies scanned; RustSec advisories, bans, licenses, duplicate policy, and sources passed |
| `./scripts/benchmark-v8-discovery.sh target/audit-discovery-c95d260` | Passed: p95 was 1.463 ms for 1 project, 1.563 ms for 10, and 2.081 ms for 40 against 50/75/125 ms budgets |
| `bash -n scripts/accept-v8-laravel.sh` | Passed: clean-room acceptance script syntax is valid |
| `git diff --check` | Passed: no patch whitespace errors |

The ignored Rust tests are not local passes. They compile in the ordinary suite
and run only in native Ubuntu CI with a real Engine. The discovery benchmark
record above is a synthetic code-regression measurement, not the controlled
host-level 40-project resource comparison.

## CI-only verification

The exact release revision must complete and archive these records before the
recommendation can change:

| CI record | Required behavior |
| --- | --- |
| Linux and macOS x86_64/arm64 build/test jobs | Release compilation and complete non-ignored suite on all four claimed host/architecture combinations |
| Linux Engine acceptance | Typed Engine lifecycle, private networks, shared-service isolation/reuse, ownership, backup/restore, retention, and delete-data authorization |
| Linux gateway acceptance | HTTP/1.1, HTTP/2, WebSocket, streaming, large bodies, atomic reload, and restart through the pinned public gateway image |
| Linux Laravel clean room | Public-image runtime derivation, PHP extensions, fatal-bootstrap rejection, strict HTTPS `/up`, command/worker/scheduler operation, automatic two-database restore, replay rejection, Engine restart, daemon restart, stable CA, source visibility, and exact cleanup |
| Discovery performance | Raw 1/10/40-project release-mode samples under the documented hosted-runner noise budget |
| Publication workflows | Binary architectures, SBOM, provenance, signatures, and immutable image/platform evidence for the exact release candidate |

Passing workflow definitions are not evidence. The resulting raw artifacts
must be retained with the release record.

## External verification still required

The following checks belong to persistent or physical hosts and remain
release-blocking for their corresponding support claims:

- macOS launchd login, reboot, sleep/wake, FSEvents, Keychain prompts and exact
  browser trust, Docker Desktop unavailable/start/restart, bind mounts, and
  keep-data/delete-data uninstall on x86_64 and arm64;
- Linux systemd-user login, reboot, suspend/wake where supported, inotify,
  Engine unavailable/start/restart, trust-store lifecycle, bind mounts, and
  keep-data/delete-data uninstall on x86_64 and arm64; and
- the unattended six-scenario 1/40-project resource comparison on one
  controlled host with an independent collector and identical Engine limits.

Exact record contents and ownership are specified in
[External verification](external-verification.md). These are evidence gaps, not
permission to add mocks or weaken acceptance criteria.

## Known limitations

- macOS and Ubuntu Linux targets remain Preview until their platform evidence
  is complete.
- Docker Engine and Docker Desktop are the only preview Engine contract.
  Podman is unsupported because behavior parity and recovery are unproven.
- SQL Server acceptance is amd64-only because that is its published image
  architecture contract.
- Stackctl intentionally supports only the documented macOS and GNU/Linux host
  targets. It does not support host application runtimes, pre-v8 configuration,
  migration from older Stackctl state, random domain repair, or per-project CA
  identities.
- Existing stale or malformed artifact locks fail closed. The daemon creates
  only a missing lock; changing an existing source requires the explicit
  `stackctl lock images` refresh path.
- Physical login, reboot, sleep/wake, interactive trust prompts, Docker Desktop
  lifecycle, and stable comparative host resource measurements cannot be
  claimed from this macOS repository gate or hosted Ubuntu CI alone.

## Release decision rule

Re-run every local command above on the final tree, archive every CI-owned
artifact from that exact revision, and attach every support-claim-specific
external record. Recommend release only if all machine-verifiable gates pass,
no Critical or High finding is open, and every remaining limitation is an
explicitly accepted product boundary rather than missing evidence.
