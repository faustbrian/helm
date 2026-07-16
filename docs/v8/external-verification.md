# V8 Verification Ownership

Verification must run on the system that owns the behavior. A macOS developer
machine is authoritative for repository checks and macOS behavior; it is not a
substitute for a Linux host. GitHub Actions owns repeatable Ubuntu and
publication checks. Physical-host transitions and comparative measurements
remain explicit external release verification.

`External` does not mean that every check is manual. It means the evidence is
owned by an environment outside the ordinary per-change local gate. Missing
external evidence MUST NOT pause implementation, local verification, or
checkpoint commits. It blocks only a release-support claim whose acceptance
criteria require that evidence.

## Local repository verification

Run these checks on the active development host after implementation changes:

```sh
just lint
just build
cargo test -q
scripts/audit-v8-host-dependencies.sh
scripts/audit-v8-workflow-actions.sh
scripts/audit-v8-supply-chain.sh
git diff --check
```

These prove source formatting, compilation, deterministic tests, dependency
security and policy, the host dependency boundary, immutable GitHub Action
references, and patch hygiene.
They do not prove login startup, trust integration, Engine recovery, Linux host
behavior, or resource efficiency.

The live Docker Engine adapter tests are explicitly ignored by the ordinary
Rust suite and invoked with `--ignored` only by native Ubuntu CI. Local
verification must compile them but must not infer passing Engine checks from
their skipped status.

## CI-owned verification

| Workflow | Automated evidence | Published artifact |
| --- | --- | --- |
| `CI` | Release build and full Rust suite on Linux and macOS, both x86_64 and arm64 | Job logs |
| `CI` | Production Engine adapter API negotiation; real owned application and private-network inventory with no application host ports; two-project network isolation proving direct cross-project and unauthorized shared-endpoint denial while one gateway reaches both apps; public digest-locked FrankenPHP base with locally installed PHP extensions; immutable Composer/Node/Bun derived-runtime build plus in-container PHP, Composer, npm, Bun, and hook execution; exact PostgreSQL 17/18 compatibility splitting; dedicated Dragonfly authentication, Memcached protocol readiness, authenticated Meilisearch, Typesense, Elasticsearch, and OpenSearch readiness with incorrect-credential rejection, Garage, LocalStack, and RustFS bucket provisioning and drift repair, Elasticsearch and OpenSearch retained-volume container replacement recovery, and Soketi health and deterministic-route verification; exact-authorization persistent-volume retain/delete semantics; durable recovery-bound installation deletion from confirmation token through terminal state; real watched-YAML Redis/Valkey discovery, publication, verified prefix backup and staged restore, removal, durable orphaning, Engine access revocation, retained-data and sibling continuity, restoration, exact adoption, stable-credential reactivation, and same-container reuse; authenticated retained-target PostgreSQL readiness plus verified custom-format backup and isolated restore with preserved rollback source and sibling database; authenticated retained-target MySQL/MariaDB readiness plus verified logical backup and isolated restore with preserved rollback source and sibling schema; authenticated retained-target MongoDB readiness plus verified archive backup and isolated restore with preserved rollback source and sibling database; authenticated retained-target SQL Server readiness plus verified native backup and isolated restore with preserved rollback source and sibling database; RabbitMQ verified topology and persistent classic-queue backup plus readiness-gated in-place selected-vhost restore with sibling preservation; MinIO verified current-object backup plus in-place selected-bucket restore with post-backup object removal and sibling preservation; shared-service isolation and lifecycle on native Ubuntu x86_64 and arm64 where images support both; and SQL Server isolation, recovery, and lifecycle on its published amd64 architecture | `engine-acceptance-linux-*` |
| `CI` | Pinned gateway HTTP/1.1, HTTP/2, WebSocket, streaming, large-body, reload, and restart acceptance on native Linux x86_64 and arm64 runners | `gateway-acceptance-linux-*` |
| `CI` | Isolated-home real Laravel bootstrap failure rejection, framework `/up` health, strict CA-file HTTPS verification, application command, worker liveness, scheduler execution, two-schema automatic restore with primary-only migration, replay rejection across reconcile and daemon restart, live Docker Engine restart with unchanged application and restored data, source visibility, stable CA identity, and exact owned-resource cleanup on native Linux x86_64 and arm64 | `laravel-acceptance-linux-*` |
| `Release` | Locked native Stackctl binary builds on Linux and macOS x86_64/arm64, executable architecture assertions, SPDX SBOM generation, GitHub Sigstore build-provenance and SBOM attestations, attestation and checksum verification, and exact tag-to-package version matching | `release-*` and `verified-release-evidence` |

Gateway and Engine acceptance evidence is retained for 30 days, while release
evidence is retained for 90 days and attached to tagged GitHub releases.
Release preparation must archive the exact CI artifacts with the release
record; a link to an expired workflow artifact is not durable evidence.

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

External verification has two different execution models:

| Model | Appropriate checks | Human involvement |
| --- | --- | --- |
| Controlled automation on persistent or dedicated hosts | Linux install and systemd-user startup, Engine stop/start recovery, daemon and workload crash recovery, gateway and service acceptance, backup/restore, uninstall data semantics, and the 40-project benchmark | A person qualifies the environment and reviews the immutable result; the test run itself SHOULD be scripted |
| Attended physical-host acceptance | Interactive login, persistent reboot, laptop sleep/wake, macOS Keychain and browser trust, Docker Desktop lifecycle, and behavior that hosted runners cannot reproduce faithfully | A person or managed hardware harness must perform and attest the real transition |

The benchmark is not a manual exploratory test. It SHOULD run unattended on a
dedicated, stable benchmark host. Shared hosted CI runners are unsuitable
because their undisclosed contention and changing VM baseline make resource
comparisons non-reproducible.

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

These release records are therefore tracked as an evidence backlog, not as
commands in the local implementation verification cycle.
