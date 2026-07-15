# V8 Completion Audit

This is an evidence inventory, not a declaration of completion. `Implemented`
means the repository contains a production path and focused automated tests.
`Partial` means only part of the acceptance criterion is implemented. `Pending
live evidence` means repository behavior exists but the required real platform
or workload record has not been committed. No row with either pending state may
be treated as release acceptance.

Snapshot date: 2026-07-15. The local full-suite evidence at the snapshot was
`cargo test --quiet`: 780 passed, 0 failed. `just lint` (format plus all-target
Clippy with the repository's configured severities), `just build`,
`scripts/audit-v8-host-dependencies.sh`, and `git diff --check` also passed.
Those local commands do not substitute for the platform and benchmark artifacts
identified below.

## Product and configuration

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-01 | One login-started per-user daemon is authoritative | Partial | `src/control_plane/daemon/unix_daemon_runtime.rs`; one-command rollback-aware setup, canonical watched-root validation, lease-protected watched-root publication, symbolic-link-refusing private singleton lease, IPC-readiness-gated transactional login-service installation, crash-recoverable directory-locked definition publication, verified manager cleanup, manager-and-IPC-backed status, bounded symbolic-link-refusing private host logging, and Unix service-manager tests | Live macOS and Linux login/reboot records |
| AC-02 | Watched-root YAML addition activates without manual start | Implemented | `reconcile_watched_roots`; discovery, debounce, complete-plan, and daemon reconciliation tests | Live claimed-platform add/edit/remove record |
| AC-03 | TOML is absent from v8 config and state paths | Implemented | strict YAML loader, unsupported-TOML discovery diagnostic, YAML lock tests, and removal of executable legacy parity harnesses | Clean-install acceptance on every claimed platform |
| AC-04 | Invalid YAML, unknown fields, duplicates, invalid names, and collisions fail before mutation | Implemented | configuration tests; `complete_discovered_registry_collision_fails_before_persistence`; transactional registry tests; typed `configuration_invalid`, `configuration_collision`, and non-overridable `security_approval_blocked` diagnostics with exact claimants and change-deduplicated logging; blocked scans retain and keep reconciling the last complete validated Engine plan without publishing partial desired state | None at repository-test level |
| AC-05 | Domains are exactly `{project}-{service}.stackctl.localhost` without repair | Implemented | identity and `composite_name_collision_fails_instead_of_receiving_a_fallback_domain` tests | None at repository-test level |
| AC-06 | Default domains resolve without hosts edits or a DNS daemon | Pending live evidence | setup and production startup `.localhost` resolver preflight before state mutation; loopback/non-loopback tests; host-dependency audit | Deliberate custom-domain fallback and loopback-resolution acceptance artifact for every claimed platform |

## Gateway, TLS, and workload plane

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-07 | HTTPS is trusted after one-time setup | Pending live evidence | Stackctl CA/leaf tests, exact X.509 leaf-expiry inspection with explicit `certificate_expired` route health on failed replacement activation, real-file certificate bundle loading, symbolic-link-refusing private cross-process certificate transaction locking, interrupted-staging recovery, bidirectional partial OS-command rollback, transactional initial trust finalization, rollback-safe Debian managed-root refresh, atomic active-generation selection, gateway-acknowledged bounded leaf-generation cleanup, zero-outage CA rotation rollback, and OS trust adapters | Install, renewal, rotation, removal, and browser trust records per claimed platform |
| AC-08 | No host Caddy or nginx is required | Implemented | pinned gateway container request; host-dependency audit | Live clean-host installation record |
| AC-09 | One managed gateway routes all projects | Implemented | gateway plane, directory-serialized and symbolic-link-refusing bootstrap publication, atomic full-snapshot, exact-domain health with explicit `gateway_route_drift`, route-aware browser opening, readiness, rollback, and port-conflict tests; `docs/v8/evidence/gateway-protocol-macos-arm64.md` proves the pinned image's HTTP/1.1, HTTP/2, WebSocket, streaming, large-body, atomic reload, and restart contract | Equivalent raw records on the remaining claimed platforms |
| AC-10 | Application containers own no separate trusted CA | Implemented | gateway terminates TLS; app upstream plans are internal plain HTTP; dependency audit | Live container inspection artifact |
| AC-11 | Project runtimes and hooks execute in Linux containers | Partial | immutable application, digest-pinned Composer/Node/Bun stages, project command, worker, daemon-timed scheduler exec, Reverb, and Engine exec paths | Representative live runtime/tool/hook acceptance on each claimed platform |
| AC-12 | Declared PHP extensions work without host PHP | Partial | Stackctl-owned PHP image definition and multi-architecture publication workflow; supported-catalog validation; content-addressed offline module enablement and verification; Engine resolution of every immutable input; exact worker image inheritance and scheduler execution inside the application container; `docs/v8/evidence/php-runtime-macos-arm64.md` | Publish the image, then record application acceptance for the supported extension and tool catalog on amd64 and arm64 |
| AC-13 | App containers publish no routine web ports | Implemented | application plan and gateway network tests | Live Engine inventory artifact |

## Shared services and state

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-14 | Compatible safe services share exact-fingerprint instances | Implemented for documented safe strategies | `docs/v8/services.md`; shared-instance resolver, dedicated-routable strategy, and service-specific plan tests | Live multi-project acceptance for every strategy advertised as shared |
| AC-15 | Incompatible profiles split with an exact explanation | Implemented | compatibility fingerprint and shared resolver tests | Live mixed-version/profile artifact |
| AC-16 | Logical resources and credentials converge idempotently | Partial | PostgreSQL, MySQL/MariaDB, MongoDB, Redis/Valkey, object-store, RabbitMQ, Mailpit, SQL Server, dedicated Dragonfly authenticated readiness, dedicated Garage authenticated bucket verification, dedicated LocalStack bucket provisioning, dedicated Memcached protocol readiness, pinned-client authenticated Elasticsearch, OpenSearch, Meilisearch, and Typesense readiness with distinct HTTP 401/403 `authentication_failed` evidence, dedicated RustFS root and pinned-client bucket provisioning, dedicated Soketi preparation/reconciliation tests, isolated `service_not_ready` retry state, and project-scoped `logical_resource_drift` that preserves the shared instance and continues later tenants without disconnecting a healthy Engine; real-directory-locked shared and project configuration and credential publication | Live authenticated readiness and drift records per advertised service |
| AC-17 | Credentials remain stable across daemon and Engine restarts | Implemented | SQLite insert-if-absent, private database/WAL/SHM permissions, symbolic-link refusal for state and managed secret files, directory-serialized secret publication, redaction, restart, and shared bootstrap credential tests | Live restart artifact |
| AC-18 | Persistent resources are never pruned implicitly | Partial | retention policy tests; orphan stop behavior; exact seven-day disposable container and ownership-proven unreferenced build-image GC tests; common token-bound PostgreSQL, MySQL/MariaDB, MongoDB, SQL Server, Redis/Valkey, RabbitMQ, and MinIO prune execution with immediate stored-artifact reverification; resource-directory-serialized, symbolic-link-refusing recovery-point publication; keep-data-default and terminal-marker-gated delete-data uninstall, typed secret-free whole-installation plan/confirm/status IPC, recovery-bound project-volume confirmation and exact Engine authorization with unlisted observed-volume refusal, idle artifact-reverified confirmed freeze, restart-safe one-at-a-time logical prune scheduling, durable deleting/deleted reconciliation freeze, daemon-owned dependency-ordered exact Engine cleanup including derived images, and logical-and-operation-empty terminal deletion invariant | RabbitMQ non-durable, quorum/stream, and non-persistent message recovery, complete recovery/deletion coverage, and live uninstall acceptance remain incomplete |

## Engine, daemon recovery, and lifecycle

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-19 | Normal operation uses typed Engine APIs, not CLI parsing | Implemented | capability traits, Bollard adapter, v8 host-dependency audit, and direct strict-v8 dispatch | Live Engine compatibility negotiation record |
| AC-20 | Engine unavailability and restart recover automatically | Implemented at unit/integration level | production managed-event subscription, cursor-based bounded reconnect, bounded connection backoff, explicit `engine_unavailable` and container `restarting` project health, health invalidation, event scheduling, and rescan recovery tests | Docker Desktop/Engine restart and sleep/wake records per claimed platform |
| AC-21 | Daemon restart, login, reboot, service crash, sleep, and wake recover | Partial | atomic SIGINT/SIGTERM loop exit, active-operation drain without claiming queued work, normal socket/lease teardown, queue/state restore, directory-serialized stable interrupted-backup staging recovery, idempotent reconciliation, restart, and crash tests | Login, reboot, sleep/wake, and service-crash platform artifacts |
| AC-22 | Removing/restoring config follows retention rules | Partial | atomic orphaning, credential disablement, adoption, daemon-reachable shared-access strategy with ownership-proven RabbitMQ, Redis/Valkey, MySQL/MariaDB, and MongoDB user deletion plus PostgreSQL `NOLOGIN`, SQL Server login disablement, and MinIO identity disablement before service idling, PostgreSQL, MySQL/MariaDB, MongoDB, and SQL Server backup and crash-replayable prune, daemon-owned Redis/Valkey prefix backup, prune, and safety-backed in-place restore, recovery-bound RabbitMQ prune plus broker-wide quiesced tenant-scoped durable persistent classic-queue message backup, credential-free topology, tar-subtree validation, and network-isolated safety-backed restore, recovery-bound MinIO current-object backup, exact tenant prune, and safety-backed in-place restore for unversioned buckets, ownership-reverified quiesced backup, safety-backed empty-target restore, recovery-bound installation deletion and prepared service-request replay for dedicated project volumes, typed isolated `destructive_replacement_required` health for retained project-volume identity drift, disposable GC, reversible PostgreSQL, MySQL/MariaDB, MongoDB, and SQL Server restore/cutover/confirm/rollback, and explicit keep-data/delete-data paths with directory-serialized terminal-marker publication and failed-operation retry tests | RabbitMQ non-durable, quorum/stream, and non-persistent message recovery, live shared-service credential-revocation acceptance, and live rename/remove/restore/uninstall acceptance |
| AC-23 | V8 enforces a clean-install major-version boundary | Implemented | v8-only CLI parser and dispatch, exact `.stackctl.yaml` discovery with no alternate configuration lookup, atomic current-schema initialization with explicit non-empty-state rejection, and no config conversion command | Clean-host installation acceptance |

## Supply chain, platforms, and efficiency

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-24 | macOS and Linux claims have platform evidence | Pending live evidence | `docs/v8/platform-support.md`; Unix architecture CI matrix; top-level unsupported-host compile boundary with no production compatibility fallbacks | Complete live macOS and Linux records |
| AC-25 | Built-in images are immutable and verified with no mutable installer pipelines | Implemented at repository-test level | digest validation, directory-serialized crash-recoverable artifact-lock publication, manifest-pinned PHP image definition, commit-pinned multi-architecture publication workflow with revision tagging, digest-signature verification, architecture assertion, and raw SBOM/provenance evidence upload, safe immutable tool-image references, offline content-addressed project builds, runtime fingerprint, and dependency audit tests | Run the publication workflow and link its published-image evidence bundle |
| AC-26 | Host dependency audit proves removed executables absent | Implemented | whole-v8-source `scripts/audit-v8-host-dependencies.sh`, removed-tree assertions, and required CI job | Clean-host runtime acceptance |
| AC-27 | Forty-project benchmark substantially improves idle usage | Pending live evidence | `scripts/benchmark-v8.sh`; explicit independently inventoried Engine-idle and per-project baseline capture; typed ownership-scoped daemon samples gated on current desired-state convergence with exact registered ownership, service implementation and major-version profiles, fingerprint enforcement, and atomic publication; `docs/v8/benchmarks.md` | Run all baseline and v8 scenarios, then publish raw records plus threshold comparison |
| AC-28 | Relevant unit, integration, recovery, chaos, platform, build, and lint checks pass | Partial | 780 local tests plus format, all-target Clippy policy, build, and host audit at this snapshot; Unix architecture CI definition | Required live platform, recovery, gateway protocol, image publication, and benchmark suites above |

## Release blockers

The current audit therefore blocks a v8 completion claim on:

1. Live macOS and Linux install/login/reboot/sleep/Engine recovery records.
2. Live persistent deletion and uninstall keep-data/delete-data acceptance.
3. Published runtime image SBOM, provenance, signature, and architecture proof.
4. Gateway protocol and failure records on the remaining claimed platforms.
5. The immutable 40-project baseline/v8 benchmark record.

Every blocker must link raw, reproducible evidence here before its row changes
to `Complete`. A passing compile, unit test, interface, plan, or document cannot
change a live-evidence row by itself.
