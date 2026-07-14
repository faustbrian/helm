# V8 Implementation Milestones

Documentation is not completion. Each milestone requires executable evidence.

## M1: Establish the v8 domain

- Characterization tests define behavior retained within the new v8 model.
- Pure raw, desired, resolved, observed, and result types exist.
- Tests prove effect-free planning boundaries.

## M2: Strict YAML and identity

- Service-map parsing is strict and deterministic.
- Unknown/duplicate fields, multiple documents, non-string versions, invalid
  names, overlong labels, and collisions fail before effects.
- Domains are exactly `{project}-{service}.stackctl.localhost`.
- JSON Schema and read-only validation are tested.
- Runtime config loading rejects TOML and directs users to create a fresh v8
  project file.

## M3: Singleton state and IPC

- One daemon owns multiple watched roots and projects.
- SQLite schema initialization and transactions survive forced interruption.
- Filesystem events plus rescan converge adds, changes, removals, and editor
  rename writes.
- The Unix-socket contract is versioned and permission-tested.

## M4: Engine API and ownership

- Reconciliation uses typed Engine API capabilities without CLI parsing.
- Labeled objects are rediscovered and safely adopted after state loss.
- Unlabelled and foreign-installation resources are never mutated.
- Event loss and engine restart recover through rescan.

## M5: Gateway and TLS

- One pinned gateway binds loopback 80/443 and applies routes atomically.
- `.localhost`, CA, wildcard leaf, renewal, rotation, and removal pass platform
  checks.
- No host Caddy/nginx, hosts entry, container CA, curl, or OpenSSL is required.
- HTTP/1.1, HTTP/2, WebSockets, streaming, large bodies, rollback, and crash
  recovery pass integration tests.

## M6: Project application plane

- Each project has a dedicated Linux runtime.
- PHP extensions, Composer, JavaScript, hooks, workers, Reverb, and schedulers
  execute inside containers.
- Runtime images are immutable, verified, and reused by fingerprint.
- App containers publish no normal host web ports.

## M7: Shared infrastructure

- Compatible data/cache/object/broker/mail/search resources share instances.
- Logical resources and credentials converge idempotently and remain stable.
- Incompatible profiles produce separate explained instances.
- Dedicated/ephemeral strategies match the service matrix.

## M8: Lifecycle, recovery, and platforms

- Environment, orphaning, adoption, backup, restore, rotation, deletion, and GC
  pass destructive-safety tests. Unsupported privilege-expanding configuration
  fails strict schema validation before planning.
- Backup, restore, replacement, and deletion workflows have verified rollback.
- Login, reboot, engine restart, daemon/service crash, sleep, and wake pass on
  every claimed platform and architecture.

## M9: Efficiency and removal

- The 40-project benchmark meets published thresholds.
- The host dependency audit proves obsolete commands absent from the core path.
- Per-project daemon, host Caddy/hosts/trust, CLI Engine control, and normal TOML
  paths are removed.
- Full build, lint, unit, integration, migration, chaos, and platform checks
  pass.

## Completion audit

Map every objective requirement to a file, test, command output, platform
record, or benchmark. Missing or indirect evidence remains incomplete.
