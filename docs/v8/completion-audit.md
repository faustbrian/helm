# V8 Completion Audit

This is an evidence inventory, not a declaration of completion. `Implemented`
means the repository contains a production path and focused automated tests.
`Partial` means only part of the acceptance criterion is implemented. `Pending
live evidence` means repository behavior exists but the required real platform
or workload record has not been committed. No row with either pending state may
be treated as release acceptance.

Snapshot date: 2026-07-14. The local full-suite evidence at the snapshot was
`cargo test --quiet`: 1,514 passed, 0 failed. `just lint`, `just build`,
`scripts/audit-v8-host-dependencies.sh`, and `git diff --check` also passed.
Those local commands do not substitute for the platform and benchmark artifacts
identified below.

## Product and configuration

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-01 | One login-started per-user daemon is authoritative | Partial | `src/control_plane/daemon/unix_daemon_runtime.rs`; singleton lease and Unix service-manager tests | Live login/reboot records; Windows named-pipe daemon and startup runtime |
| AC-02 | Watched-root YAML addition activates without manual start | Implemented | `reconcile_watched_roots`; discovery, debounce, complete-plan, and daemon reconciliation tests | Live claimed-platform add/edit/remove record |
| AC-03 | TOML is absent from normal v8 config and state paths | Implemented | strict YAML loader, v8 legacy fallback guard, YAML lock tests, isolated `config migrate` module | Remove isolated compatibility parser only after the v7 migration window ends |
| AC-04 | Invalid YAML, unknown fields, duplicates, invalid names, and collisions fail before mutation | Implemented | configuration tests; `complete_discovered_registry_collision_fails_before_persistence`; transactional registry tests | None at repository-test level |
| AC-05 | Domains are exactly `{project}-{service}.stackctl.localhost` without repair | Implemented | identity and `composite_name_collision_fails_instead_of_receiving_a_fallback_domain` tests | None at repository-test level |
| AC-06 | Default domains resolve without hosts edits or a DNS daemon | Pending live evidence | `.localhost` resolver preflight tests; host-dependency audit | Loopback-resolution acceptance artifact for every claimed platform |

## Gateway, TLS, and workload plane

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-07 | HTTPS is trusted after one-time setup | Pending live evidence | Stackctl CA/leaf tests and OS trust adapters | Install, renewal, rotation, removal, and browser trust records per claimed platform |
| AC-08 | No host Caddy or nginx is required | Implemented | pinned gateway container request; host-dependency audit | Live clean-host installation record |
| AC-09 | One managed gateway routes all projects | Implemented | gateway plane, atomic full-snapshot, readiness, rollback, and port-conflict tests | Full HTTP/1.1, HTTP/2, WebSocket, streaming, large-body, and crash acceptance artifact |
| AC-10 | Application containers own no separate trusted CA | Implemented | gateway terminates TLS; app upstream plans are internal plain HTTP; dependency audit | Live container inspection artifact |
| AC-11 | Project runtimes and hooks execute in Linux containers | Partial | immutable application, project command, worker, scheduler, Reverb, and Engine exec paths | Representative live runtime/hook acceptance on each claimed platform |
| AC-12 | Declared PHP extensions work without host PHP | Partial | content-addressed runtime-image planning and extension validation tests | Built-image and application acceptance for the supported extension catalog on amd64 and arm64 |
| AC-13 | App containers publish no routine web ports | Implemented | application plan and gateway network tests | Live Engine inventory artifact |

## Shared services and state

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-14 | Compatible safe services share exact-fingerprint instances | Implemented for documented safe strategies | `docs/v8/services.md`; shared-instance resolver and service-specific plan tests | Live multi-project acceptance for every strategy advertised as shared |
| AC-15 | Incompatible profiles split with an exact explanation | Implemented | compatibility fingerprint and shared resolver tests | Live mixed-version/profile artifact |
| AC-16 | Logical resources and credentials converge idempotently | Implemented | PostgreSQL, MySQL/MariaDB, MongoDB, Redis/Valkey, object-store, RabbitMQ, Mailpit, and SQL Server preparation/reconciliation tests | Live authenticated readiness and drift records per advertised service |
| AC-17 | Credentials remain stable across daemon and Engine restarts | Implemented | SQLite insert-if-absent, redaction, restart, and shared bootstrap credential tests | Live restart artifact |
| AC-18 | Persistent resources are never pruned implicitly | Implemented | retention policy tests; orphan stop behavior; exact seven-day disposable GC tests; common token-bound PostgreSQL, MySQL/MariaDB, MongoDB, SQL Server, Redis/Valkey, RabbitMQ, and MinIO prune execution with immediate stored-artifact reverification; keep-data-default and terminal-marker-gated delete-data uninstall, typed secret-free whole-installation plan/confirm/status IPC, recovery-bound project-volume confirmation and exact Engine authorization with unlisted observed-volume refusal, idle artifact-reverified confirmed freeze, restart-safe one-at-a-time logical prune scheduling, durable deleting/deleted reconciliation freeze, daemon-owned dependency-ordered exact Engine cleanup, and logical-and-operation-empty terminal deletion invariant | Complete recovery/deletion coverage and live uninstall acceptance remain incomplete |

## Engine, daemon recovery, and lifecycle

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-19 | Normal operation uses typed Engine APIs, not CLI parsing | Implemented | capability traits, Bollard adapter, v8 host-dependency audit, strict legacy guard | Live Engine compatibility negotiation record |
| AC-20 | Engine unavailability and restart recover automatically | Implemented at unit/integration level | event supervisor, bounded backoff, health invalidation, and rescan recovery tests | Docker Desktop/Engine restart and sleep/wake records per claimed platform |
| AC-21 | Daemon restart, login, reboot, service crash, sleep, and wake recover | Partial | queue/state restore, idempotent reconciliation, restart, and crash tests | Login, reboot, sleep/wake, and service-crash platform artifacts |
| AC-22 | Removing/restoring config follows retention rules | Partial | atomic orphaning, credential disablement, adoption, PostgreSQL, MySQL/MariaDB, MongoDB, and SQL Server backup and crash-replayable prune, daemon-owned Redis/Valkey prefix backup, prune, and safety-backed in-place restore, recovery-bound RabbitMQ prune plus safety-backed in-place topology restore for empty vhosts, recovery-bound MinIO current-object backup, exact tenant prune, and safety-backed in-place restore for unversioned buckets, ownership-reverified quiesced backup, safety-backed empty-target restore, and recovery-bound installation deletion for dedicated project volumes, disposable GC, reversible PostgreSQL, MySQL/MariaDB, MongoDB, and SQL Server restore/cutover/confirm/rollback, and explicit keep-data/delete-data paths with terminal-marker and failed-operation retry tests | RabbitMQ non-empty message backup and restore plus live rename/remove/restore/uninstall acceptance |
| AC-23 | Existing v7 projects have tested migration and rollback | Partial | TOML-to-YAML semantic migration; watched-root-scoped daemon IPC/CLI inventory; secret-free typed v7 config/container/image/mount/route/trust/runtime evidence; bounded generated-environment metadata, project-scoped hosts/Caddy routes, and public Caddy CA revisions; randomized user-only exact environment rollback capture and concrete managed-environment binding without project `.env` mutation; typed active-target bindings for recreated workloads, stateless logical resources, and explicitly targetless ephemeral services; shared recoverable-provider lifecycle plus exact accepted-container and configured/observed-volume strategy binding with recovery-first target restore, rollback source retention, and confirmation-only retirement; exact accepted driver, project, service, kind, container-label, immutable Engine ID, logical-data identity, and configured/observed named-volume binding for PostgreSQL, plus driver/container/logical identity binding for MySQL, MongoDB, SQL Server, Redis, Valkey, MinIO, and RabbitMQ provider kinds; separate typed v7 Engine command targets with per-command exact legacy-label revalidation instead of false v8 ownership and one bounded attached/streaming transport shared with v8-owned sessions; live PostgreSQL v7 custom-dump streaming into accepted-revision-bound private recovery, pre-mutation tamper refusal, replay-safe deterministic v8 target reset/restore, target catalog verification, retained-source verification, and confirmation-time exact Engine container/volume retirement with stopped-container use scans and ambiguity refusal; private identity-bound full-gateway snapshot backup, verified replacement, tamper-blocked rollback, and target revision proof; private identity-bound legacy CA backup, exact Stackctl CA cutover, tamper-blocked legacy trust restoration without original files, and project-safe shared-trust retention; exact legacy-label and volume-drift blockers; fresh-scan confirmation tokens and append-only SQLite acceptance with stale-evidence and identity-collision refusal; evidence-bound deployment plus service/volume/route/trust/environment adapter selection for every v7 driver; schema-18 immutable project-wide adapter journal with a verified preparation barrier and restart-safe monotonic transitions; exact execution-scoped per-resource strategy registry that can borrow live typed providers, concrete explicit no-op bindings, and recovery-first resumable preparation coordinator; project-wide replay-safe cutover with atomic route/application/environment desired state, reverse-order rollback with atomic restored state and retained targets, and confirmation-gated source retirement; reversible PostgreSQL, MySQL/MariaDB, MongoDB, and SQL Server backup/restore/cutover/confirm/rollback tests | Bind remaining recoverable strategies to live Engine/service providers, add remaining data-service operations, and prove live rollback |

## Supply chain, platforms, and efficiency

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-24 | macOS, Windows, and Linux claims have platform evidence | Pending live evidence | `docs/v8/platform-support.md`; Unix architecture CI matrix | Complete live Unix records; Windows runtime is explicitly unsupported |
| AC-25 | Built-in images are immutable and verified with no mutable installer pipelines | Implemented at repository-test level | digest validation, artifact lock, installer checksum, runtime fingerprint, and dependency audit tests | Published-image SBOM, provenance, signature, amd64, and arm64 release artifacts |
| AC-26 | Host dependency audit proves removed executables absent | Implemented | `scripts/audit-v8-host-dependencies.sh` and required CI job | Clean-host runtime acceptance |
| AC-27 | Forty-project benchmark substantially improves idle usage | Pending live evidence | `scripts/benchmark-v8.sh`; typed ownership-scoped daemon samples; `docs/v8/benchmarks.md` | Immutable v7, Engine baseline, v8 compatible, and v8 split raw records plus threshold comparison |
| AC-28 | Relevant unit, integration, migration, chaos, platform, build, and lint checks pass | Partial | 1,533 local tests plus lint/build at this snapshot; Unix architecture CI definition | Required live platform, migration breadth, gateway protocol, image publication, and benchmark suites above |

## Release blockers

The current audit therefore blocks a v8 completion claim on:

1. Windows named-pipe IPC, login service, and live Windows recovery evidence.
2. Live macOS and Linux install/login/reboot/sleep/Engine recovery records.
3. Live persistent deletion and uninstall keep-data/delete-data acceptance.
4. Complete v7 adapter execution, coordinated cutover, rollback, and live proof.
5. Published runtime image SBOM, provenance, signature, and architecture proof.
6. Gateway protocol and failure acceptance against the real pinned image.
7. The immutable 40-project v7/baseline/v8 benchmark record.

Every blocker must link raw, reproducible evidence here before its row changes
to `Complete`. A passing compile, unit test, interface, plan, or document cannot
change a live-evidence row by itself.
