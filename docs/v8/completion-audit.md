# V8 Completion Audit

This is an evidence inventory, not a declaration of completion. `Implemented`
means the repository contains a production path and focused automated tests.
`Partial` means only part of the acceptance criterion is implemented. `Pending
live evidence` means repository behavior exists but the required real platform
or workload record has not been committed. No row with either pending state may
be treated as release acceptance.

Snapshot date: 2026-07-14. The local full-suite evidence at the snapshot was
`cargo test --quiet`: 1,436 passed, 0 failed. `just lint`, `just build`,
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
| AC-18 | Persistent resources are never pruned implicitly | Implemented | retention policy tests; orphan stop behavior; exact seven-day disposable GC tests; common token-bound PostgreSQL, MySQL/MariaDB, MongoDB, SQL Server, and RabbitMQ prune execution; keep-data-default uninstall and pre-mutation delete-data refusal | Complete recovery/deletion coverage and uninstall delete-data execution remain incomplete |

## Engine, daemon recovery, and lifecycle

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-19 | Normal operation uses typed Engine APIs, not CLI parsing | Implemented | capability traits, Bollard adapter, v8 host-dependency audit, strict legacy guard | Live Engine compatibility negotiation record |
| AC-20 | Engine unavailability and restart recover automatically | Implemented at unit/integration level | event supervisor, bounded backoff, health invalidation, and rescan recovery tests | Docker Desktop/Engine restart and sleep/wake records per claimed platform |
| AC-21 | Daemon restart, login, reboot, service crash, sleep, and wake recover | Partial | queue/state restore, idempotent reconciliation, restart, and crash tests | Login, reboot, sleep/wake, and service-crash platform artifacts |
| AC-22 | Removing/restoring config follows retention rules | Partial | atomic orphaning, credential disablement, adoption, PostgreSQL, MySQL/MariaDB, MongoDB, and SQL Server backup and crash-replayable prune, recovery-bound RabbitMQ prune with scoped definitions backup for empty vhosts, unversioned MinIO current-object backup, disposable GC, reversible PostgreSQL, MySQL/MariaDB, MongoDB, and SQL Server restore/cutover/confirm/rollback, and explicit safe uninstall-mode tests | Remaining persistent-service adapters, RabbitMQ message backup and restore, MinIO prune and restore, delete-data uninstall execution, and live rename/remove/restore acceptance |
| AC-23 | Existing v7 projects have tested migration and rollback | Partial | TOML-to-YAML semantic migration; reversible PostgreSQL, MySQL/MariaDB, MongoDB, and SQL Server backup/restore/cutover/confirm/rollback tests | Complete v7 inventory and adapters for remaining data services, routes, trust, app runtimes, volumes, and generated environment |

## Supply chain, platforms, and efficiency

| ID | Requirement | State | Authoritative evidence | Missing evidence or work |
|---|---|---|---|---|
| AC-24 | macOS, Windows, and Linux claims have platform evidence | Pending live evidence | `docs/v8/platform-support.md`; Unix architecture CI matrix | Complete live Unix records; Windows runtime is explicitly unsupported |
| AC-25 | Built-in images are immutable and verified with no mutable installer pipelines | Implemented at repository-test level | digest validation, artifact lock, installer checksum, runtime fingerprint, and dependency audit tests | Published-image SBOM, provenance, signature, amd64, and arm64 release artifacts |
| AC-26 | Host dependency audit proves removed executables absent | Implemented | `scripts/audit-v8-host-dependencies.sh` and required CI job | Clean-host runtime acceptance |
| AC-27 | Forty-project benchmark substantially improves idle usage | Pending live evidence | `scripts/benchmark-v8.sh`; typed ownership-scoped daemon samples; `docs/v8/benchmarks.md` | Immutable v7, Engine baseline, v8 compatible, and v8 split raw records plus threshold comparison |
| AC-28 | Relevant unit, integration, migration, chaos, platform, build, and lint checks pass | Partial | 1,436 local tests plus lint/build at this snapshot; Unix architecture CI definition | Required live platform, migration breadth, gateway protocol, image publication, and benchmark suites above |

## Release blockers

The current audit therefore blocks a v8 completion claim on:

1. Windows named-pipe IPC, login service, and live Windows recovery evidence.
2. Live macOS and Linux install/login/reboot/sleep/Engine recovery records.
3. Explicit persistent deletion and uninstall keep-data/delete-data execution.
4. Complete v7 resource inventory and non-PostgreSQL migration adapters.
5. Published runtime image SBOM, provenance, signature, and architecture proof.
6. Gateway protocol and failure acceptance against the real pinned image.
7. The immutable 40-project v7/baseline/v8 benchmark record.

Every blocker must link raw, reproducible evidence here before its row changes
to `Complete`. A passing compile, unit test, interface, plan, or document cannot
change a live-evidence row by itself.
