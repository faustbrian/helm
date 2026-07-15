# V8 Verification Ownership

Verification must run on the system that owns the behavior. A macOS developer
machine is authoritative for repository checks and macOS behavior; it is not a
substitute for a Linux host. GitHub Actions owns repeatable Ubuntu and
publication checks. Physical-host transitions and comparative measurements
remain explicit external release verification.

## Local repository verification

Run these checks on the active development host after implementation changes:

```sh
just lint
just build
cargo test -q
scripts/audit-v8-host-dependencies.sh
scripts/audit-v8-workflow-actions.sh
git diff --check
```

These prove source formatting, compilation, deterministic tests, the host
dependency boundary, immutable GitHub Action references, and patch hygiene.
They do not prove login startup, trust integration, Engine recovery, Linux host
behavior, or resource efficiency.

The live Docker Engine adapter test is explicitly ignored by the ordinary Rust
suite and invoked with `--ignored` only by native Ubuntu CI. Local verification
must compile it but must not infer a passing Engine check from its skipped
status.

## CI-owned verification

| Workflow | Automated evidence | Published artifact |
| --- | --- | --- |
| `CI` | Release build and full Rust suite on Linux and macOS, both x86_64 and arm64 | Job logs |
| `CI` | Production Engine adapter API negotiation plus managed container, network, image, volume, and published-port inventory on native Ubuntu x86_64 and arm64 runners | Job logs |
| `CI` | Pinned gateway HTTP/1.1, HTTP/2, WebSocket, streaming, large-body, reload, and restart acceptance on native Linux x86_64 and arm64 runners | `gateway-acceptance-linux-*` |
| `Runtime Images` | Multi-architecture runtime publication, manifest architecture checks, SBOM, provenance, and keyless signature verification | `runtime-image-evidence-php-8.5` |

The runtime-image workflow runs automatically for `v8.*` release tags and may
also be dispatched deliberately for a release candidate. Publication evidence
is retained for 90 days. Gateway evidence is retained for 30 days. Release
preparation must archive the exact artifacts with the release record; a link to
an expired workflow artifact is not durable evidence.

CI may exercise only behavior genuinely provided by its runner. A hosted
Ubuntu runner can prove Linux compilation, tests, Engine protocol behavior, and
container image behavior. It cannot honestly prove an interactive user login,
a persistent machine reboot, laptop sleep/wake, macOS Keychain behavior, or a
controlled Docker Desktop restart.

## External release verification

The following checks require a clean, persistent host or deliberately prepared
benchmark environment and must be recorded outside the ordinary local gate:

| Owner | Required checks |
| --- | --- |
| macOS x86_64 and arm64 hosts | Fresh install, launchd login startup, Keychain trust install/rotation/removal, `.localhost` loopback resolution, Docker Desktop unavailable/start/restart recovery, daemon and workload crash recovery, reboot, laptop sleep/wake, FSEvents, bind mounts, file watching, gateway traffic, backup/restore, and uninstall keep-data/delete-data |
| Linux x86_64 and arm64 hosts | Fresh install, systemd user login startup, system trust install/rotation/removal, `.localhost` loopback resolution, Docker Engine unavailable/start/restart recovery, daemon and workload crash recovery, reboot, suspend/wake where supported, inotify, bind mounts, file watching, gateway traffic, backup/restore, and uninstall keep-data/delete-data |
| Controlled benchmark host | All six benchmark scenarios using one immutable run ID, identical Engine limits and filesystem mode, an independent host/VM collector, exact baseline inventories, and raw Stackctl samples |

Each record must contain the Stackctl revision, release candidate, host image
and build, architecture, Engine identity and limits, immutable image digests,
exact commands, raw outputs, timestamps, pass/fail/skip state, and cleanup
result. A skipped check remains a release gap; it is never converted into a
pass by unit tests or documentation.

## Release gate

The completion audit may cite local checks directly. CI-owned requirements must
cite the archived artifact from the exact release revision. External
requirements must cite the corresponding physical-host or benchmark record.
No developer is expected to reproduce Linux host behavior on macOS, fabricate
published-image evidence without publication, or run a comparison benchmark
without its required baseline inputs.
