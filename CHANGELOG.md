# Changelog

All notable changes to this project are documented in this file.

## [7.0.0] - 2026-07-12

### Fixed

- Blocked whole-installation delete-data preflight when project-owned
  persistent volumes have no ownership-bound recovery adapter, preventing
  terminal Engine cleanup from erasing unprotected dedicated-service data,
  including Engine-observed volumes missing from SQLite state.
- Updated v8 operations guidance to describe the confirmed daemon-owned
  delete-data lifecycle instead of the former unavailable-mode behavior.
- Refused delete-data cleanup for unmarked runtime directories, malformed
  terminal markers, missing installed-service state, and runtime symlinks so
  local filesystem cleanup cannot follow or infer ownership.
- Refused installation-deletion freeze while any durable daemon operation is
  queued or running, preventing backup, restore, command, or migration work
  from racing the serialized teardown sequence.
- Prevented a terminally deleted installation from being moved back into the
  deleting lifecycle by replaying the teardown transition.
- Reverified the exact stored backup manifest, identity, checksum, and size
  immediately before every destructive logical prune, so catalog evidence
  cannot authorize deletion after an artifact is missing or tampered with.
- Made the legacy TCP health-check test server accept both expected successful
  connections instead of racing listener shutdown after the first request.
- Reported Redis and Valkey tenant prefixes as logical data-lifecycle
  resources even though their compatible server container is shared.
- Denied Redis and Valkey tenant credentials access to `SCAN`, `KEYS`, and
  `RANDOMKEY`, preventing cross-project key-name enumeration while preserving
  key-prefix enforcement for ordinary application commands and scripts.
- Scoped RabbitMQ and MinIO in-container backup staging paths by deterministic
  tenant identity as well as timestamp to prevent concurrent project overlap.

### Added

- Added an accepted-v7 MongoDB logical migration provider. It streams a
  database-scoped archive through the separately authorized legacy container,
  binds recovery to accepted evidence, replay-safely rebuilds the deterministic
  v8 database and user, remaps every source namespace, verifies both
  authenticated identities, and retires exact Engine resources only after
  confirmation.
- Added accepted-v7 MySQL and MariaDB logical migration providers. They bind
  flavor to accepted image evidence, stream schema-neutral verified dumps,
  reject and clean up dumps carrying explicit legacy definers, replay-safely
  reset and restore deterministic v8 schemas, verify both source and target
  identities, and reuse exact confirmation-time Engine retirement.
- Added confirmation-time accepted-v7 PostgreSQL retirement through a narrow
  Engine capability. Cleanup revalidates the immutable container ID, exact
  legacy labels, and accepted named-volume set before stopping or deleting
  anything, refuses volumes still used by any container, and treats only a
  fully absent source as a successful replay.
- Added the first live accepted-v7 logical-data provider for PostgreSQL. It
  streams an identity-bound custom dump through the Engine, verifies recovery
  before target mutation, replay-safely resets and restores the deterministic
  v8 database, verifies both sides, and keeps source retirement behind a
  confirmation-only strategy.
- Shared the bounded attached and streaming command transport between v8-owned
  containers and separately authorized accepted-v7 targets, keeping timeout,
  output, status polling, and exit handling single-sourced.
- Added a narrow typed Engine command boundary for accepted v7 containers.
  Docker-compatible execution now re-inspects the immutable container ID and
  requires exact legacy managed, container, service, and kind labels before
  creating any attached command session.
- Added one shared recovery-first v7 provider contract for named volumes and
  logical data, plus a logical-data adapter that binds the exact accepted
  driver, container, and logical identity for every supported database, cache,
  bucket, and vhost strategy before any provider mutation.
- Added a concrete v7 named-volume strategy boundary. Registration matches the
  exact accepted container plus configured and Engine-observed volume set,
  requires recovery before restoring and verifying a v8 target, retains the
  legacy source for rollback, and permits retirement only after confirmation.
- Added a concrete installation-scoped v7 trust adapter. It privately backs up
  exact accepted legacy Caddy CAs, installs the Stackctl CA at cutover, restores
  legacy trust from verified backup even when original files disappear, and
  never removes shared legacy trust during project confirmation.
- Added a concrete full-snapshot v7 gateway adapter. It stores a private,
  identity-bound rollback snapshot, publishes and verifies the complete target
  route revision, and refuses rollback when the retained artifact is missing,
  changed, or belongs to different accepted evidence.
- Made the exact v7 adapter registry execution-scoped so stateful strategies
  can safely borrow live typed Engine and gateway providers without global
  handles, ownership transfer, or `'static` adapter workarounds.
- Added concrete recreation adapters for v7 project workloads, stateless
  services, and ephemeral services. Preparation now requires exact active v8
  workload or logical-resource ownership, while only explicitly ephemeral
  services may have no durable target identity.
- Added a concrete protected generated-environment v7 adapter. It re-verifies
  accepted private rollback evidence, binds the exact active managed
  environment revision as its target, and leaves the user-owned project
  `.env` byte-for-byte untouched through cutover and confirmation.
- Added the matching atomic v7 rollback commit for restored route ownership,
  project intent, managed environment, retained logical targets, and execution
  proof. Restored identity is preflighted before adapter side effects run.
- Added an atomic v7 cutover commit for route ownership, application project
  intent, the complete managed environment, and the project-wide execution
  checkpoint. A rejected or skipped journal transition now rolls back every
  desired-state write instead of leaving restart-visible split state.
- Added concrete no-op v7 strategies for absent routes, trust, environments,
  and volumes, plus volume checkpoints whose data lifecycle is owned by a
  logical service adapter. They remain explicit lifecycle participants and
  reject any plan that incorrectly asks them to create recovery material.
- Added an exact per-resource v7 adapter registry. Each immutable checkpoint
  receives its own strategy instance keyed by adapter ID and selected kind;
  missing, duplicate, extra, or kind-mismatched bindings fail before execution
  intent is persisted instead of falling through a shared kind dispatcher.
- Added project-wide v7 cutover, rollback, and confirmation coordinators to
  the common adapter strategy boundary. Cutover journals only after every
  idempotent strategy succeeds, route publication runs last, rollback reverses
  that order, and source retirement remains behind explicit confirmation.
- Added a resumable v7 adapter preparation coordinator behind a common
  strategy registry. It preflights the complete adapter set, verifies every
  required recovery artifact before provisioning targets, journals each
  completed step, and resumes after failure without repeating durable work.
- Added a schema-18 project-wide v7 adapter execution journal. Accepted
  evidence and its deterministic adapter-plan revision now bind an immutable
  service, volume, route, trust, and environment checkpoint set. Recovery and
  target proof advance monotonically, and cutover is refused until every
  selected adapter has reached the durable prepared barrier.
- Added deterministic, evidence-revision-bound v7 migration adapter plans.
  Every accepted legacy service now resolves to an explicit v8 deployment
  strategy and logical-data, named-volume, stateless, workload, or ephemeral
  migration adapter. Routes, installation-scoped legacy Caddy trust, and
  generated environments also receive explicit adapters; unknown drivers,
  route drift, unsupported mounts, missing CA evidence, and unprotected
  environments fail before any mutation instead of receiving a fallback.
- Captured exact legacy generated-environment bytes into randomized private
  rollback envelopes during accepted v7 inventory. Acceptance now revalidates
  stable source metadata, atomically records verified artifact evidence in
  schema 17, and refuses reuse when the protected rollback is missing or
  corrupt without exposing secrets or deterministic secret-derived hashes.
- Added bounded, secret-free v7 host-artifact inventory for generated `.env`
  metadata and key names, project-scoped hosts and Caddy routes, and public
  Caddy CA certificates. Artifact reads reject symlinks, oversize files, and
  concurrent changes, and their evidence is bound into migration acceptance.
- Added explicit, revision-bound v7 inventory acceptance. Read-only inventory
  now returns a purpose-bound confirmation token only when no blockers exist;
  `daemon migration accept [PATH] --confirmation-token TOKEN` re-inventories
  through the selected Engine, rejects stale evidence and project-identity
  collisions, and appends the exact secret-free observation to SQLite without
  changing the legacy source.
- Added `daemon migration inventory [PATH]` as an explicit read-only legacy
  inspection path. The singleton limits requests to authoritative watched
  roots, reads only a bounded regular `.stackctl.toml`, rejects source changes
  during parsing, discovers legacy resources through the selected Engine, and
  returns typed secret-free evidence and automatic-migration blockers over IPC.
- Added a read-only v7 project inventory through a dedicated typed Engine
  capability. It deterministically binds legacy config to stopped or running
  legacy-labelled containers, observed image identities and mounts, routes,
  trust requirements, logical names, credential-field presence, environment
  keys, and runtime customizations without retaining secret values. Ambiguous
  ownership, missing observations, volume drift, bind or anonymous mounts, and
  unsupported Swarm sources remain explicit migration blockers.
- Bound project-owned persistent volumes into installation delete-data plans
  with user-visible resource and recovery identities, confirmation tokens that
  include exact artifact evidence, reverification at freeze and immediately
  before cleanup, and an exact Engine volume-name authorization list. Unlisted
  Engine-observed volumes still block teardown before mutation.
- Integrated dedicated project volumes into daemon restore admission with an
  exact desired-service plan, deterministic operation-bound safety backup,
  immediate artifact reverification, empty-volume recreation, archive upload
  before service start, and readiness verification.
- Added an ownership-bound dedicated-volume restore primitive that immediately
  re-verifies cataloged recovery evidence, removes only the exact owned service
  and volume, recreates the desired empty target, streams the archive through
  the Engine API before start, and requires the restored service to become
  ready. The daemon invokes it only after cataloging the current volume as a
  separate verified safety recovery point.
- Added daemon-owned backups for dedicated project-service volumes that resolve
  exact live container and volume ownership, quiesce only the matching service,
  stream the named volume through the Engine API into immutable checksummed
  recovery storage, and restore the prior running state after success or
  failure.
- Added `daemon service uninstall --delete-data --confirm-delete-data` execution
  that plans and confirms exact teardown, resumes interrupted deletion, polls
  durable terminal state, removes matching CA trust, stops the login service,
  and only then removes marked runtime state and verified backups.
- Added typed daemon IPC for token-confirmed installation deletion and durable
  lifecycle progress, including remaining logical-resource counts and active
  operation IDs for deterministic terminal polling.
- Added daemon-owned terminal teardown that waits for empty logical and durable
  work, removes exact installation-owned Engine objects, commits terminal state
  only after cleanup succeeds, and keeps reconciliation frozen while the
  terminal daemon remains available for status polling.
- Added a restart-safe installation-deletion driver that durably queues one
  exact recovery-authorized logical prune at a time, deduplicates lost
  in-memory work against durable operation history, and blocks stale ordinary
  Engine reconciliation while teardown owns mutation.
- Added a confirmed installation-deletion transition that regenerates the
  complete plan, rejects token drift without mutation, rereads every selected
  backup artifact, and only then atomically freezes reconciliation.
- Added a terminal `deleted` installation lifecycle that can only be committed
  after every logical tenant has been retired, atomically clearing residual
  physical ownership, credentials, environments, migrations, and recovery rows.
- Added an installation-scoped Engine cleanup operation that reconstructs
  exact ownership, refuses ambiguous same-installation labels before mutation,
  ignores foreign installations, and removes containers, volumes, then networks.
- Added a secret-free daemon IPC response for complete installation-deletion
  preflight, including stable ordered per-tenant intents and one whole-plan
  confirmation token that changes when any protected state changes.
- Added a deterministic installation-deletion preflight that refuses teardown
  unless every retained logical tenant has one supported destructive adapter,
  one exact credential, and a latest matching verified recovery-point record.
- Added a durable installation-deletion lifecycle barrier that atomically
  clears watched roots, orphans every registered project, disables its managed
  credentials and environment, and prevents reconciliation from recreating
  resources while explicit teardown is in progress.
- Integrated empty-vhost RabbitMQ recovery points into the daemon restore queue
  with exact compatibility selection, a verified topology safety backup,
  idempotent in-place replacement, post-import verification, and replay that
  reuses operation-bound safety evidence without replacing the shared broker.
- Integrated unversioned MinIO recovery points into the daemon restore queue
  with exact compatibility selection, a verified current-object safety backup,
  in-place bucket replacement, and replay that reuses the operation-bound safety
  evidence without provisioning or replacing the shared server.
- Integrated Redis and Valkey recovery points into the daemon restore queue with
  exact compatibility-plan selection, a verified pre-restore safety snapshot,
  in-place tenant-prefix replacement, and crash replay that reuses the durable
  safety evidence instead of taking another snapshot.
- Added a verified Redis and Valkey prefix restore adapter that stages opaque
  values before atomically replacing only the exact tenant namespace, preserves
  remaining TTLs, omits expired records, and keeps administrator secrets out of
  command arguments and durable state.
- Extended recovery-bound logical prune execution to Redis and Valkey with
  administrator-authenticated ACL revocation, atomic exact-prefix deletion,
  idempotent crash replay, and shared-instance preservation.
- Integrated Redis and Valkey prefix backups into the daemon's bounded project
  backup queue with secret-free durable intents, exact shared-administrator
  resolution, deterministic tenant prefixes, and verified recovery points.
- Added a Redis/Valkey logical backup adapter that uses the shared
  administrator for one atomic prefix-only Lua snapshot, preserves binary keys,
  opaque `DUMP` values, and TTL metadata, and rejects cross-prefix artifacts.
- Added an ownership-checked MinIO recovery adapter that re-verifies immutable
  backup identity, checksum, and size before replacing only the exact target
  bucket through scoped credentials and an idempotent mirror operation.
- Extended recovery-bound logical prune execution to MinIO with exact
  machine-readable bucket, user, and policy inventories, administrator-only
  deletion, and idempotent crash replay without touching the shared instance.
- Added daemon-owned MinIO bucket backups with runtime-only tenant credentials,
  machine-readable versioning checks, fail-closed rejection of version history,
  streamed current-object archives, and immutable checksum verification.
- Added daemon-owned RabbitMQ topology backups using exact vhost ownership,
  a fail-closed zero-message policy, scoped broker definition export, immutable
  artifact storage, and checksum verification without exposing credentials.
- Extended common recovery-bound logical prune execution to RabbitMQ with
  user-first access revocation, exact vhost deletion, idempotent crash replay,
  and atomic tenant-state retirement without touching the shared broker.
- Added daemon-owned reversible SQL Server recovery execution with exact
  catalog selection, isolated persistent targets, native restore verification,
  atomic cutover, explicit source retirement, and retained-target rollback.
- Added separately owned persistent SQL Server migration targets with durable
  SA replay, native backup verification and restore, database-user remapping,
  and exact tenant-authenticated target verification.
- Added daemon-owned SQL Server native backups using exact active tenant
  ownership, in-container checksummed `.bak` creation, runtime-only login
  credentials, streamed immutable storage, and checksum verification.
- Extended common token-bound logical prune execution to SQL Server with exact
  recovery and ownership revalidation, runtime-only SA credentials, idempotent
  database/login deletion, and atomic tenant-state retirement.
- Extended common token-bound logical prune execution to MongoDB with exact
  orphan, credential, recovery-point, installation, and compatibility checks,
  idempotent in-container tenant deletion, and atomic state retirement.
- Added daemon-owned reversible MongoDB recovery execution with exact catalog
  selection, isolated persistent targets, tenant-authenticated verification,
  atomic environment cutover, explicit confirmation with source retirement,
  and rollback that retains the restored target and recovery evidence.
- Added exact MongoDB target verification through the restored tenant identity,
  requiring byte-exact database and ping evidence while keeping its encoded
  connection URI confined to the in-container command environment.
- Added a bounded MongoDB restore adapter that re-verifies immutable recovery
  evidence and exact retained-target ownership before streaming an archive with
  an RFC 3986-encoded, runtime-only administrator URI.
- Added separately owned, persistent MongoDB migration target plans and
  convergence with deterministic private bootstrap-secret files, durable
  administrator replay, exact compatibility reuse, and healthy Engine proof.
- Added daemon-owned MongoDB logical backups using the exact active tenant
  identity, an RFC 3986-encoded runtime-only connection URI, in-container
  streamed archives, private immutable storage, and checksum verification.
- Added daemon-owned MySQL/MariaDB recovery execution with exact catalog
  selection, isolated retained targets, authenticated restore verification,
  atomic environment cutover, explicit confirmation, source schema/user
  retirement, and rollback that preserves both recovery proof and target data.
- Corrected migration-decision target reconstruction to select the distinct
  migration-owned logical record instead of resolving the active source by its
  database name.
- Added separately owned, persistent MySQL/MariaDB migration target plans and
  convergence that reuse exact Linux compatibility profiles without changing
  normal sharing, persisting the target administrator before Engine mutation.
- Added a bounded MySQL/MariaDB restore adapter that re-verifies exact durable
  recovery identity, checksum, size, tenant ownership, and isolated target
  ownership before streaming through the flavor-specific in-container client.
- Added daemon-owned MySQL and MariaDB logical backups using flavor-specific
  dump clients inside the exact owned shared container, consistent streaming
  flags, runtime-only tenant credentials, private immutable artifact storage,
  and checksum-verified recovery evidence.
- Generalized immutable logical prune authorization and crash-replayable queued
  execution across PostgreSQL and MySQL/MariaDB strategies, persisting the
  selected adapter in secret-free intent and revalidating exact state before
  service-specific Engine mutation.
- Added an idempotent MySQL-family logical deletion adapter that validates
  exact orphan, credential, installation, flavor, and compatibility ownership
  before attached Engine exec while keeping tenant secrets out of deletion SQL.
- Added explicit daemon uninstall modes with keep-data behavior as the default
  and a confirmation-gated delete-data spelling that fails before service or
  resource mutation until complete persistent deletion coverage is proven.
- Added explicitly confirmed PostgreSQL logical deletion with immutable
  recovery-point binding, stale-token rejection before Engine mutation,
  runtime-only administrator credentials, idempotent database/role removal,
  atomic tenant-state retirement, and crash-aware daemon replay. Verified
  recovery evidence remains retained and unsupported service kinds fail closed.
- Added effect-free PostgreSQL logical prune planning for exact orphaned state,
  requiring an explicitly selected matching recovery point and returning a
  stable secret-free confirmation token over typed singleton IPC. Ambiguous,
  active, unsupported, or unverified retained state fails closed.
- Added a requirement-level v8 completion audit that separates implemented
  repository behavior from missing live, platform, migration, and benchmark
  evidence.
- Added daemon-owned garbage collection for expired seven-day disposable
  orphans with exact Engine ownership and atomic durable-state retirement.
- Added ownership-scoped Engine benchmark snapshots and a non-overwriting raw
  sample harness that never parses Docker or Podman CLI output.
- Added full-suite Linux and macOS x86_64/arm64 CI coverage plus an explicit
  platform matrix that keeps unverified and Windows combinations unsupported.
- Added a CI-enforced v8 host-dependency audit that rejects direct process
  execution and legacy runtime imports outside explicit OS integration seams.
- Added daemon-worker acceptance coverage proving rollback reconstructs the
  retained PostgreSQL source environment and preserves the isolated target.
- Added explicit `stackctl daemon migration confirm` and `rollback` commands
  that follow durable daemon operations and validate exact terminal evidence.
- Added asynchronous confirm-or-rollback execution that reconstructs exact
  source and retained-target ownership from durable migration checkpoints,
  serializes decisions against all other Engine mutation, retires PostgreSQL
  source access only after confirmation, and atomically restores the previous
  managed environment while retaining the target on rollback.
- Added typed confirm-or-rollback admission for exact cutover migrations, with
  project ownership validation, secret-free durable payloads, bounded queuing,
  queued replay after restart, and loud terminalization of ambiguous decisions
  interrupted while running.
- Added `stackctl daemon restore <recovery-point-id> [path]` to queue one exact
  cataloged recovery point, follow its ordered daemon events, and report the
  resulting migration only after it reaches the explicit confirmation gate.
- Added asynchronous singleton PostgreSQL restore execution that resolves the
  exact current compatibility plan, reconciles an isolated retained target,
  restores and verifies immutable catalog evidence, atomically cuts project
  environment state over, and stops at an explicit confirmation gate while
  retaining the untouched source for rollback. Commands, backups, restores,
  and normal Engine reconciliation are serialized around this mutation.
- Added typed singleton restore admission for exact verified recovery points,
  with secret-free durable queue payloads, bounded serialization, queued replay
  after daemon restart, and explicit terminalization of ambiguous interrupted
  restores without deleting retained target state.
- Added one idempotent PostgreSQL migration-target boundary that durably
  prepares credentials, reconciles an isolated retained volume and service
  through typed Engine capabilities, and returns success only for a healthy,
  exactly owned target without publishing host ports.
- Added deterministic project-owned PostgreSQL migration target plans that
  reuse the exact immutable compatibility profile while creating a separate
  private container and retained volume, preventing verified restores from
  targeting the active shared instance. Target preparation now persists and
  reuses its administrator credential before Engine mutation so daemon restarts
  cannot rotate the restore target secret.
- Added an exact recovery-point restore coordinator that selects immutable
  catalog evidence by ID, verifies project, service, logical-resource, kind,
  and compatibility ownership before mutation, and resumes the reversible
  migration state machine at isolated target provisioning without taking a
  redundant source backup.
- Added durable singleton PostgreSQL recovery-point operations with secret-free
  queued identity, runtime-only credential resolution, exact owned shared-service
  matching, direct Engine streaming, verified host artifacts, serialized Engine
  mutation, restart-safe queued replay, and loud terminalization of ambiguous
  in-flight backups.
- Added `stackctl daemon backup <service> [path]` to queue an exact logical
  recovery point, follow its ordered daemon events to completion, and report
  the verified host recovery path, byte size, and SHA-256 evidence.
- Added an immutable SQLite recovery-point catalog with exact project, service,
  logical-resource, compatibility, path, checksum, size, creation, and
  verification evidence. `stackctl daemon backups [path]` lists that durable
  catalog instead of relying on bounded operation events or directory guesses.
- Added explicit data-lifecycle strategy resolution for every currently shared
  authoritative service family, including logical, native, bucket-export, and
  shared-snapshot boundaries. Non-data and unknown logical kinds now fail
  loudly instead of falling through to a generic container backup path, and
  strict v8 status exposes whether recovery is tenant-scoped or instance-wide.
- Added `stackctl daemon migration status` over typed singleton IPC so users
  can inspect exact project-scoped durable migration phases, verified-backup
  state, and confirmation requirements without exposing recovery paths,
  credentials, or retained v7 rollback material.
- Added typed operation-scoped browser execution for Dusk and Selenium with
  durable `--browser` intent, immutable artifacts, official Grid readiness,
  private-network endpoint injection, no host ports, Engine-native 2 GiB shared
  memory, unconditional command cleanup, and interrupted-session recovery.
- Excluded ephemeral Dusk and Selenium declarations from steady Engine
  reconciliation so browser containers remain command-scoped and disposable
  instead of becoming always-on project workloads.
- Isolated concurrent legacy Caddy filesystem tests with unique temporary home
  directories so one test cannot remove another test's active fixture.
- Added a revisioned built-in v8 artifact catalog for preset-only services,
  including deterministic default versions, exact registry sources, and loud
  failure for unsupported versions or stale catalog locks. Application process
  presets inherit the application artifact instead of creating duplicate lock
  entries.
- Added a narrow direct-Engine registry image resolver that converts exact
  mutable references to validated immutable manifest references without Docker
  or Podman CLI parsing, while keeping immutable image acquisition separate.
  Strict v8 `lock images`, `lock verify`, and `lock diff` now use YAML and typed
  singleton IPC with atomic publication and exact response-key validation.
- Added strict project-local v8 YAML artifact-lock consumption with exact
  source freshness checks, immutable sha256 resolutions, bounded non-symlink
  discovery, and fail-before-Engine planning for malformed or stale locks.
- Added retained project-volume convergence for stateful dedicated presets with
  canonical container mount paths, exact ownership, adoption gating, durable
  lifecycle, and explicit-migration failure on data identity drift. Log target
  resolution now excludes same-scope volume records.
- Added the v8 dedicated-project-service Engine substrate for conservative
  presets, with exact ownership, immutable images and versions, deterministic
  names, private networking, restart supervision, durable lifecycle, live
  health, and no implicit host port or gateway route.
- Changed v8 Engine invalidation to atomically discard all in-memory health
  observations before reconnect backoff, preventing disconnected resources from
  retaining a recently healthy projection.
- Added timestamped, non-durable Engine health snapshots to strict v8 project
  status for exact project workloads and shared logical services. Stale
  observations become unknown, and `open` now requires typed ready state rather
  than probing an arbitrary HTTP path or trusting durable lifecycle alone.
- Isolated legacy inferred-environment tests from process-global Docker and
  Podman selection so parallel test execution cannot leak Podman's host alias
  into assertions scoped to Docker behavior, and made doctor runtime fixture
  paths collision-free under concurrent creation. Fake runtime commands are
  now thread-local so unrelated tests cannot execute deleted fixture binaries.
- Added immediate cancellation cleanup and monotonic idle expiry for bounded v8
  project-log sessions so disconnected clients release daemon capacity and
  abort their Engine streams without waiting for a restart.
- Added strict v8 `logs` streaming for exact declared application and shared
  services through bounded daemon-owned Engine sessions, preserving attributed
  stdout and stderr without Docker CLI, host Caddy, or durable log storage.
- Added the typed v8 project-log session protocol and bounded, binary-safe
  in-memory cursor buffer required to stream Engine logs without storing
  application output in SQLite or invoking the Docker CLI.
- Added authoritative v8 log-session registration, polling, and cancellation
  with exact resolution of project application and shared-service ownership
  before an Engine stream can start.
- Added concurrent v8 Engine log producers with live ownership verification,
  bounded channel backpressure, binary-safe stream attribution, and clean
  cancellation without Docker CLI or host log tools.
- Added strict v8 `open` using only daemon-published HTTPS routes, with exact
  service or all-route selection, machine-readable output, explicit platform
  opener failures, and no legacy curl, database-port, or health-path probing.
- Added typed strict v8 execution for Deno and the whitelisted PHPStan, ECS,
  PHP-CS-Fixer, Psalm, Pint, Pest, PHPUnit, and Rector project tools through
  owned application containers, with exact arguments and declarative runtime
  version enforcement.
- Added a strict v8 dispatch boundary that prevents unported YAML commands from
  entering v7 config, Docker CLI, or host-tooling execution paths, while
  preserving normal legacy dispatch for explicitly discovered TOML projects.
- Added strict v8 `url` lookup through authoritative singleton project status,
  returning only exact published HTTPS routes and rejecting legacy kind or
  driver selectors instead of deriving host ports through the v7 config path.
- Added strict v8 Artisan and non-interactive exec dispatch through the
  singleton command queue, preserving exact non-shell arguments and rejecting
  interactive PTY and browser-bootstrap flows until the daemon IPC can support
  them without silently falling back to the legacy runtime.
- Added explicit strict v8 managed-environment export through user-only IPC,
  with redacted diagnostics, exact active-project selection, sorted escaped
  dotenv output, user-only file permissions, and create-once behavior that
  never silently merges with or overwrites a developer's `.env` file.
- Added strict v8 `ps` over singleton IPC with table and JSON output for exact
  registered routes, project-owned runtime resources, shared logical tenants,
  and their durable lifecycle without exposing credentials or invoking the
  container engine from the CLI.
- Added strict v8 YAML dispatch for Composer, Bun, and Node package-manager
  commands through the singleton IPC boundary, with exact service selection,
  resumable binary-safe output, declarative runtime-version enforcement, and
  explicit npm, pnpm, or Yarn execution instead of changing `stackctl node`
  into a raw Node.js invocation.
- Added durable typed project-command IPC for Composer, Node package managers,
  Bun, and declared hooks, with exact owned-application resolution, serialized
  background Engine
  execution, binary-safe stdout/stderr events, bounded operation retention, and
  restart recovery that resumes queued work but never ambiguously replays a
  command that was already running.
- Added bounded lossless stdout and stderr capture for in-container project
  commands so singleton IPC clients can receive command output without host
  shell execution.
- Added a bounded monotonic SQLite-backed singleton event journal with
  restart-safe resumable cursor polling, explicit stale-cursor diagnostics,
  and accepted, completed, or failed lifecycle events for reconciliation and
  project adoption.
- Added daemon startup integrity checks and private, bounded, consistent SQLite
  recovery snapshots before state migration or mutation, with fail-closed
  handling that preserves the original database when verification fails.
- Added durable reference-counted shared-service idling that stops unused
  compatibility instances only after their final active logical consumer is
  released, while retaining containers, volumes, and tenant data.
- Added exact per-project logical-service reconciliation that orphans omitted
  tenants, disables their credentials, releases active shared references, and
  replaces generated managed environments atomically, including empty sets.
- Added explicit project adoption through the singleton IPC boundary, restoring
  orphaned workloads, logical tenants, credentials, and managed environments
  atomically while preserving superseded retained resources as history.
- Added authoritative follow-up discovery after mutating daemon requests so
  adopted projects re-enter the normal validated Engine convergence pipeline.
- Added a pre-mutation adoption gate that blocks reappearing project services
  when their only durable workload ownership is orphaned or retained, while
  permitting active scopes to keep older replacement history.
- Added durable per-service workload ownership scopes, transactional SQLite
  migration, application and worker ownership publication, and safe orphan
  convergence that stops removed project workloads without deleting retained
  containers or data.
- Connected Horizon, queue workers, and schedulers to their exact immutable
  application runtime and source mount, with explicit application dependencies,
  merged non-conflicting environments, preset commands, separate restart
  policy, and durable process ownership.
- Added exact-compatible stateless Gotenberg sharing with one disposable
  process, project-specific endpoint environments, durable consumer claims, and
  no unnecessary credentials or volumes.
- Added SQL Server shared-instance convergence with explicit YAML EULA
  acceptance, edition-aware compatibility grouping, policy-compliant stable
  credentials, isolated databases/logins, persistent physical/logical
  ownership, and managed `sqlsrv` environments.
- Added MongoDB shared-instance convergence with exact compatibility grouping,
  stable bootstrap and project credentials, a private read-only bootstrap
  secret mount, isolated database users, persistent physical/logical ownership,
  and managed MongoDB environments.
- Added attributed Mailpit convergence with exact compatibility grouping,
  replay-stable SMTP identities, deterministic bcrypt authentication, one
  persistent physical process, project-specific mail environments, and
  `{project}-mailpit.stackctl.localhost` routes in the atomic gateway snapshot.
- Added RabbitMQ shared-instance convergence with exact compatibility grouping,
  replay-stable project credentials, one atomically published hash-only
  definitions snapshot, isolated vhost/user pairs, persistent physical and
  logical ownership, and managed broker environments.
- Added MinIO shared-object-store convergence with exact compatibility
  grouping, durable root and project credentials, deterministic bucket-scoped
  policies, one physical process and volume, and project-specific AWS
  environments. Garage and RustFS remain fail-closed until their IAM isolation
  contracts are proven.
- Added Redis and Valkey shared-instance strategies with exact engine/version/
  image/platform fingerprints, durable admin and project credentials, one
  daemon-owned atomic ACL snapshot, project-scoped users and key prefixes,
  persistent physical ownership, and managed cache environments.
- Added a backend-neutral prepared shared-instance strategy boundary and routed
  PostgreSQL, MySQL, and MariaDB through it. Exact MySQL-family compatibility
  groups now reserve stable bootstrap/project credentials, start one physical
  instance, provision isolated schema/users, and publish durable ownership plus
  managed application environments.
- Connected explicit immutable PostgreSQL demand to daemon convergence: shared
  groups and credentials resolve before mutation, each physical instance starts
  once, tenants provision before dependent apps, and merged ownership plus
  environment state publishes before gateway routes.
- Made PostgreSQL tenant ownership reference the stable persistent volume rather
  than an ephemeral process, and persist both the shared container and volume as
  durable physical ownership before dependent applications start. Replaced
  backend process identities are retained explicitly instead of remaining
  falsely active.
- Added an atomic state transaction for publishing logical shared-service
  ownership together with its complete project environment, rolling back both
  on identity conflicts or adoption requirements.
- Added durable PostgreSQL preparation that reserves one bootstrap secret per
  compatibility group and one tenant secret per project service, reproducing
  identical instance, logical database, and environment plans on replay.
- Added execution-plan resolution for PostgreSQL compatibility demand so exact
  version, digest, platform, persistence, and isolation matches share one
  physical instance while any material difference produces a separate group.
- Connected complete immutable-application Engine plans to the singleton daemon,
  reconciling every workload before atomically publishing its content-derived
  route snapshot through the containerized gateway's private admin socket.
- Added immutable custom-application planning from resolved services to exact
  per-service Engine ownership, content-derived revisions, and gateway routes,
  rejecting mutable or unresolved image artifacts before mutation.
- Allowed immutable application images to retain their default command when a
  v8 service does not declare an explicit command override.
- Added a distinct dependency-ordered execution plan that resolves validated
  desired services to explicit deployment strategies and is retained by the
  daemon as the sole input for later Engine-side service dispatch.
- Retained the last complete validated project registry in the daemon Engine
  schedule while conflicted scans block further mutation, preserving exact
  desired context for recovery after the conflict is corrected.
- Made complete gateway snapshot revisions derive from their sorted domain and
  upstream content, preventing changed route sets from reusing a stale
  caller-supplied revision and being skipped as already active.
- Made application workload plans preserve each declared service identity in
  deterministic container names and gateway routes instead of rewriting every
  application-like service to `app`.
- Limited deterministic gateway route ownership to project applications,
  attribution-aware shared UIs, and explicit custom-image services so database,
  queue, and scheduler services no longer reserve nonexistent HTTP domains.
- Added checksum verification for the runtime installer artifact and included
  its exact bytes in content-addressed derived-image build contexts, removing
  the undocumented requirement that base images provide an implicit installer.
- Made login-service definitions publish atomically without following an
  existing destination symlink, preventing partial writes or writes through
  redirected service paths.
- Changed launchd login-service installation to obtain the effective user ID
  from the operating system directly instead of invoking the host `id` tool.
- Added an executable v8 retention policy that keeps active resources, ages out
  disposable orphans, and blocks persistent deletion without explicit prune
  intent and verified backup evidence.
- Bound persistent prune authorization to checksum-verified backup bytes and
  the exact installation, resource, and compatibility fingerprint being deleted.
- Added idempotent RabbitMQ project-user revocation that retains orphaned vhost
  data and uses bounded, credential-free direct Engine exec output.
- Added ownership-safe RabbitMQ reconciliation that atomically publishes complete
  core definitions before broker convergence and reloads them through direct Engine exec.
- Added one compatibility-keyed Mailpit with deterministic bcrypt SMTP identities,
  authenticated project tags, retained storage, readiness, and per-project UI routes.
- Changed MailHog to remain dedicated until it proves the same project-attribution
  contract used for safe Mailpit sharing.
- Added an ownership-validated direct-Engine completion wait for bounded,
  disposable provisioning containers without Docker CLI process calls.
- Added bounded direct-Engine attached command streaming for multi-megabyte
  backup and restore payloads with concurrent stdin/stdout flow, drained
  value-safe stderr, exact exit status, and redacted command diagnostics.
- Added asynchronous immutable backup persistence so Engine command output
  streams directly into private, checksummed, atomically published recovery
  points without whole-artifact buffering or a second scratch copy.
- Bound backup manifests and storage paths to exact Engine or logical resource
  identities so project databases sharing one service cannot collide or
  authorize recovery using another tenant's evidence.
- Added direct-Engine PostgreSQL custom dumps that stream through bounded
  memory into logical-resource recovery points, publish only after exit zero,
  validate ownership and credentials, and remove cancelled pending artifacts.
- Changed resumable migration operations to receive their durable phase
  checkpoint so restore and cutover adapters can enforce journaled checksum,
  size, target, and rollback evidence after daemon restarts.
- Added direct-Engine PostgreSQL restores that reject non-absolute or linked
  recovery points, verify logical ownership and journaled checksum and size,
  then stream the exact custom dump into its isolated target transaction.
- Fixed PostgreSQL logical provisioning to connect with the same managed
  bootstrap administrator configured by the shared official image instead of
  assuming the image also created a separate `postgres` role.
- Made PostgreSQL logical provisioning authenticate explicitly with the active
  managed bootstrap credential through redacted command environment state,
  avoiding image-version-dependent local authentication assumptions.
- Required PostgreSQL restore to run as the exact active project role rather
  than the bootstrap administrator, ensuring `--no-owner` restores create
  application objects with usable project ownership.
- Added a bounded PostgreSQL target gate that proves the restored database is
  owned by the project role and has no invalid indexes or unvalidated
  constraints before migration cutover becomes eligible.
- Added idempotent PostgreSQL migration target creation using the normal
  deterministic project database and role identity, with exact checkpoint,
  ownership, service, installation, and compatibility validation.
- Added an atomic SQLite migration cutover transaction that replaces exact
  project route ownership and managed environment state together with the
  monotonic cutover checkpoint, rolling back every write on any failure.
- Changed migration operations to return validated cutover desired state so
  the coordinator, rather than resource adapters, owns its atomic persistence
  and resumes directly from the committed checkpoint after a daemon restart.
- Added symmetric atomic rollback planning that restores retained project
  routes and environment state, marks v8 logical targets as retained, and
  preserves recovery evidence with the terminal rollback checkpoint.
- Added atomic migration target ownership so a provisioned logical tenant,
  its exact stable project credential, and the target checkpoint become
  durable together or all remain absent after a failed validation or restart.
- Added a concrete PostgreSQL migration adapter that binds streamed backup,
  deterministic provisioning, restore, catalog verification, atomic cutover
  and rollback plans, and confirmation-only source retirement to exact state.
- Added direct-Engine PostgreSQL source retirement that validates the exact
  owned source and journaled cutover before idempotently dropping only its
  database and role while retaining the shared service container.
- Added a fail-closed singleton discovery reconciliation boundary that reads
  authoritative watched roots and publishes a registry only after a complete,
  issue-free scan, preserving the last valid registry on partial discovery.
- Added bounded request/response serving on the user-only Unix daemon socket,
  with strict typed decoding, request correlation, and oversized-frame
  rejection before dispatch.
- Replaced per-project v8 IPC reconciliation with one complete watched-root
  operation that returns applied, project, and issue counts while converting
  scan or transaction failures into stable correlated diagnostics.
- Added a Unix singleton daemon runtime that exclusively owns its private
  state, lease, and IPC paths, performs scheduled complete reconciliation,
  serves bounded requests, and safely recovers only stale socket files.
- Changed `daemon watch` and the login-service path to run one v8 control
  plane backed by SQLite instead of starting per-project daemons, and removed
  exclusion and project-limit flags that could produce partial registries.
- Replaced project-scoped daemon start, status, stop, logs, and hidden run
  commands with singleton status and reconciliation over typed IPC, removing
  TOML PID sessions and the legacy per-project supervisor implementation.
- Added bounded native watched-root notifications to the singleton daemon,
  feeding the existing debounce scheduler while retaining periodic complete
  scans as the correctness fallback.
- Added first-run singleton installation initialization with one cryptographic
  installation identity and one persisted platform-default Docker endpoint,
  reusing both exactly across daemon restarts without engine auto-switching.
- Added quiet direct-Engine connection supervision to the long-running Unix
  singleton, with cancellable connection deadlines and bounded jittered retry
  while offline discovery and IPC remain available when Docker is starting.
- Added safe singleton reconciliation for one deterministic `stackctl` Engine
  network after complete valid registry scans, adopting only exact labeled
  current-installation ownership and loudly blocking ambiguous network state.
- Added the production singleton gateway request using the official Caddy
  2.11.4 multi-platform manifest by immutable digest, with the image command
  contract, global network, loopback ports, and ownership fixed in core code.
- Added restart-safe certificate bundle recovery that verifies every immutable
  stored revision, deterministically selects the latest renewal generation,
  and blocks unexpected or ambiguous private TLS state.
- Restricted the gateway container to individual read-only wildcard leaf
  mounts so the Stackctl CA private key never enters the workload plane, and
  tied gateway replacement identity to the immutable certificate generation.
- Wired the singleton daemon to prepare restart-safe TLS/bootstrap assets,
  resolve the pinned gateway image, and reconcile one loopback-only gateway
  container after the global network and a complete valid registry scan.
- Run the gateway as the daemon user's numeric UID and GID so its private
  `0600` admin socket remains accessible to the owning user on native Linux
  while the image's low-port capability still permits loopback ports 80/443.
- Added `daemon trust <install|status|remove>` as the explicit one-time trust
  lifecycle for the exact immutable CA shared with the singleton gateway.
- Changed macOS trust setup from privileged System Keychain mutation to
  per-user trust with local certificate verification and exact file removal.
- Added deterministic disposable provisioning jobs with ownership preflight,
  bounded completion, stale-job recovery, and safe post-exit cleanup.
- Added compatibility-keyed MinIO instances with persistent shared data,
  deterministic bucket identities, scoped policies, and secret-safe reconciliation.
- Kept RustFS dedicated until its external admin-client lifecycle and recovery
  behavior prove the same bucket, identity, and policy isolation contract.
- Added compatibility-keyed SQL Server instances with persistent storage,
  readiness, and idempotent per-project database and login reconciliation.
- Added private stateless Gotenberg sharing by exact immutable profile with
  module-aware readiness and daemon-managed project endpoints.
- Replaced in-place v7 config rewriting with non-destructive strict v8 YAML
  candidates, checksum-bound semantic reports, and explicit review blockers.
- Added transactional backup restoration with exact resource verification,
  checksum-tracked isolated staging, target-native validation, atomic cutover,
  and mandatory rollback after every post-staging failure.
- Added a durable monotonic migration journal that requires verified backup,
  artifact reference, target, readiness, and rollback evidence before reversible
  cutover and rejects skipped phases, identity drift, proof replacement, and
  terminal-state changes.
- Added a crash-resumable migration coordinator that advances only completed
  backup, target, restore, verification, and cutover checkpoints, retains v7
  rollback material, and requires explicit confirmation before source retirement.
- Added streaming atomic private backup recovery points with portable resource
  manifests, immutable history, crash-safe pending recovery, and reread verification.
- Hardened durable resource reconciliation against immutable ownership drift and
  required an explicit exact-match adoption transaction to reactivate retained data.
- Added complete atomic project adoption that binds the registered canonical path
  and reactivates only exact resources, credentials, and environment revision.
- Preserved complete preset, image, version, extension, database, and dependency
  declarations in deterministic v8 desired state instead of discarding them.
- Added durable logical tenant-resource ownership and active reference counting
  for shared instances, including orphaning and exact project adoption behavior.
- Added bounded deterministic watched-root discovery with canonical deduplication,
  isolated TOML migration diagnostics, size limits, and symlink-safe YAML loading.
- Added deterministic daemon discovery scheduling with initial and periodic full
  scans, editor-event debounce, and a bounded settle deadline for continuous writes.
- Added per-resource deterministic equal-jitter exponential retry state with
  explicit recovery reset and an absolute maximum delay.
- Added a closed deployment-strategy resolver for every current preset and alias,
  defaulting conditional sharing to dedicated until isolation is proven.
- Added complete-scan project reconciliation that validates the full registry before
  atomically registering discoveries and orphaning every project whose config vanished.
- Added executable dedicated application-container requests with exact ownership,
  Linux platform, private networking, source mount, environment, command, and no host ports.
- Added typed project runtime environment composition that requires active matching
  daemon ownership, rejects conflicting declared values, and redacts every value in diagnostics.
- Added strict v8 YAML process commands and deterministic environment mappings,
  including pre-mutation validation of executables, variable names, NUL bytes,
  and debug redaction of environment values.
- Added a bundled draft 2020-12 JSON Schema for the complete strict v8 project
  YAML shape, including DNS identities, string versions, commands, and environment values.
- Added offline `stackctl config schema` output that requires no project configuration,
  daemon state, or container-engine connection.
- Added read-only `stackctl config validate [PATH]` parsing and desired-state
  resolution that runs without daemon or container-engine availability.
- Added dedicated Engine-backed project process plans for workers and schedulers,
  with immutable Linux images, private networking, no published ports, restart supervision,
  and secret-safe desired-plan diagnostics.
- Added ownership-safe project application reconciliation with idempotent start,
  unhealthy restart, disposable revision replacement, and duplicate failure.
- Added immutable per-resource ownership labels so multiple project workers and
  schedulers remain distinguishable after daemon state loss or Engine restart.
- Reused one ownership-safe workload reconciliation policy for applications,
  workers, and schedulers while selecting processes by exact resource identity.
- Added atomic v8 project unregistration that releases route ownership while
  preserving project-owned resources as timestamped orphans for reconciliation.
- Added v8 SQLite persistence for immutable installation identity, the selected
  Docker Engine endpoint, and atomically replaced canonical watched roots.
- Added typed v8 Engine container settings for private networks, dual-stack
  loopback port publication, read-only bind mounts, and restart policy so the
  shared gateway can own ports 80/443 without a host web server.
- Added validated managed-container environment injection with deterministic
  Engine mapping and key-only diagnostics that never expose secret values.
- Added explicit Linux platform selection to managed container creation so
  compatibility profiles retain their architecture at the Engine boundary.
- Added one enforced v8 gateway Engine request that fixes its deterministic
  name, loopback ports, private network attachment, TLS mount, and restart policy.
- Added a narrow v8 NetworkManager capability backed by direct Engine API calls
  and complete ownership labels for the shared Stackctl bridge network.
- Hardened v8 network deletion to require typed ownership proof and a fresh
  Engine-label match instead of accepting an arbitrary network identifier.
- Hardened v8 container start, stop, and deletion operations to accept only
  typed owned containers and revalidate Engine labels immediately before mutation.
- Added v8 container-handle reconstruction from complete current-installation
  Engine labels so daemon-state loss can recover without adopting user containers.
- Added bounded direct Engine rescans for Stackctl-marked networks and volumes
  with backend-independent observations for full-state recovery.
- Added ownership-safe network and volume handle reconstruction from complete
  current-installation labels after daemon database loss or Engine restart.
- Added a bounded direct-Engine image resolver that accepts only sha256-pinned
  references, reuses local content, and pulls missing immutable images by digest.
- Added a typed direct-Engine managed-container event stream with installation
  filters, lifecycle and health actions, and reconnect cursor deduplication.
- Added an ownership-validated direct-Engine log source with typed tail options
  and byte-preserving stdout, stderr, stdin, and console frames.
- Added ownership-validated direct-Engine command execution with structured
  non-shell requests, attached streaming I/O, exit inspection, and redacted debug output.
- Added an ownership-validated direct-Engine health observer that distinguishes
  missing, stopped, unverified, starting, healthy, and unhealthy containers.
- Added ownership-validated direct-Engine resource sampling with deterministic
  CPU basis points plus memory, process, and aggregated network measurements.
- Added content-addressed direct-Engine derived image builds with immutable base
  validation, mandatory ownership labels, offline networking, and cache verification.
- Replaced opaque image-build archives with deterministic typed context files so
  the validated Dockerfile is exactly the one sent to the direct Engine API.
- Added deterministic offline runtime-image plans keyed by pinned base, Linux
  platform, PHP, Composer, JavaScript, system packages, and installer revision.
- Allowed derived Engine content IDs as immutable container inputs while keeping
  registry pulls restricted to digest-pinned repository references.
- Made Engine image identities validated sha256 values so malformed backend
  responses cannot enter runtime planning or container reconciliation.
- Added project-runtime reconciliation that validates the complete application
  request before building or reusing its derived image and starting the workload.
- Added bounded non-shell Composer, Node, Bun, and repository-hook execution
  inside the matching owned application container through the direct Engine API.
- Added ownership-safe shared-volume reconciliation that creates or adopts an
  exact compatibility volume and never deletes persistent data during convergence.
- Added compatibility-keyed shared-service reconciliation that detects foreign
  ownership before mutation and replaces drifted containers without deleting data.
- Composed PostgreSQL instance reconciliation with idempotent attached database
  and role provisioning for stable project credentials and managed environments.
- Composed MySQL/MariaDB and MongoDB shared-instance reconciliation with isolated
  schema/database users provisioned through bounded attached Engine commands.
- Added atomic Redis/Valkey ACL snapshot reconciliation with exact read-only mount
  validation, shared process convergence, and authenticated in-container reload.
- Added a native-JSON Caddy gateway provider that atomically loads complete route
  snapshots over a private Unix socket while using only Stackctl-owned TLS files.
- Changed the gateway HTTP listener to deterministic 308 redirects so application
  traffic is never proxied in plaintext while HTTPS remains the only upstream path.
- Added an explicit non-shell gateway health check that validates the mounted
  Caddy configuration without relying on curl, a shell, or image defaults.
- Added a non-mutating gateway port preflight that checks both loopback families,
  reports every known owner, and never falls back to random public ports.
- Added unfiltered structured Engine discovery for running containers' published
  TCP ports so gateway diagnostics can identify foreign container owners.
- Composed Engine port inventory with host listener probing so conflicts name
  exact containers without invoking Docker, Podman, lsof, or netstat CLIs.
- Added idempotent gateway reconciliation that creates missing containers,
  restarts stopped owned containers, and leaves healthy owned state untouched.
- Added atomic route-revision reconciliation that skips matching state and
  verifies the desired revision after every gateway configuration load.
- Added bounded gateway readiness convergence so route loads cannot race a
  starting, stopped, missing, or unhealthy gateway container.
- Composed gateway container recovery, bounded readiness, and atomic route
  reconciliation into one ordered control-plane transaction.
- Added deterministic local certificate lifecycle reconciliation that preserves
  the trusted CA while renewing wildcard gateway leaf material when due.
- Persisted certificate renewal deadlines inside atomic private bundle revisions
  and added validated bundle loading for daemon restart recovery.
- Extended the singleton gateway container plan with an immutable bootstrap config,
  a private writable admin-socket mount, and an explicit in-container Caddy command.
- Added atomic user-private persistence for immutable gateway bootstrap JSON and
  the host runtime directory used by the mounted Caddy admin socket.
- Added a replaceable `.stackctl.localhost` setup preflight that fails closed
  when the operating system returns no address or any non-loopback address.
- Added direct v8 volume lifecycle management with deterministic local volumes,
  complete ownership labels, typed ownership proof, and immediate label
  revalidation required for deletion.
- Added typed named-volume container mounts distinct from host bind mounts so
  persistent shared-service data stays Engine-owned and retention-aware.
- Added v8 SQLite resource ownership, compatibility, retention, and orphan-state
  persistence with a transactional, data-preserving schema-v1 migration.
- Added durable v8 project credentials with redacted diagnostics, stable
  insert-once reconciliation, and disable-on-project-removal lifecycle state.
- Extended durable credentials with an optional project owner so shared-instance
  bootstrap secrets cannot be disabled by unrelated project lifecycle changes.
- Added complete versioned managed-environment persistence with key-only debug
  output, atomic replacement, restart recovery, and disable-on-orphan behavior.
- Added v8 dedicated application-container planning with immutable reusable
  images, deliberate source mounts, private networking, and zero host ports.
- Added atomic user-private Unix persistence for immutable v8 TLS bundles with
  redacted debug output and verified idempotent revisions.
- Added Rust-native v8 generation for a Stackctl-owned local CA and renewable
  `*.stackctl.localhost` gateway certificate without host OpenSSL or Caddy PKI.
- Added leaf-only v8 TLS renewal that preserves the installed Stackctl CA and
  rotates only the wildcard gateway certificate and private key.
- Added an OpenSSL-free SHA-256 identity for validated Stackctl CA certificates
  so trust-store reconciliation and removal can target exact certificate bytes.
- Added an idempotent OS trust-store boundary that installs missing Stackctl CAs
  once and removes only the exact certificate identity supplied by Stackctl.
- Added a macOS System Keychain trust adapter with exact SHA-256 inspection,
  explicit root installation, and fingerprint-scoped certificate removal.
- Added a Windows Current User root-store adapter with exact SHA-1 thumbprint
  lookup, non-elevated installation, and thumbprint-scoped removal.
- Added an explicit Debian-family trust adapter that owns one fingerprint-named
  local root, refuses conflicting file contents, and refreshes system CA state.
- Added a replaceable v8 gateway boundary that validates and atomically applies
  complete deterministic domain-to-internal-HTTP route snapshots.
- Added deterministic v8 shared-instance planning that groups all project
  demand by compatibility fingerprint and deduplicates logical consumers.
- Changed shared-instance plans to retain their canonical compatibility profile
  so reconciliation can instantiate the exact image, platform, and policy.
- Added Rust-native 256-bit managed credential generation from OS entropy with
  injectable randomness for tests and redaction-safe secret diagnostics.
- Added deterministic PostgreSQL database and restricted-role provisioning
  plans that fail on overlong names and keep passwords in attached stdin only.
- Added executable shared PostgreSQL container and volume plans with exact
  profile ownership, private networking, secrets, restart, and platform policy.
- Added version-aware PostgreSQL persistence targets, retaining the legacy data
  directory through 17 and using the official parent mount for 18 and newer.
- Added bounded attached PostgreSQL logical-resource execution that streams
  secret SQL over stdin, drains output, and requires a successful exit status.
- Added complete PostgreSQL project resource composition covering logical SQL,
  stable credential state, and private-host managed application environment.
- Added separate executable MySQL and MariaDB shared-instance plans with exact
  profiles, root credential ownership, private networking, and retained volumes.
- Added MySQL-family project composition with isolated schemas/users, scoped
  grants, stable credentials, and private managed application environments.
- Added one reusable bounded attached-command runner for secret stdin, output
  draining, status polling, timeouts, and redacted execution diagnostics.
- Added direct MySQL and MariaDB logical-resource execution using attached SQL
  stdin and environment-only root authentication inside owned containers.
- Added deterministic Redis-compatible ACL snapshots with anonymous access
  disabled, project-scoped keys and channels, restricted command categories,
  collision rejection, redacted diagnostics, and atomic private persistence.
- Added separate Redis and Valkey shared-instance plans with hash-only readable
  ACL mounts, private networking, pinned Linux images, explicit persistence,
  retained data volumes, and no credentials in container arguments.
- Added Redis-compatible project composition with deterministic ACL identities,
  stable durable credentials, isolated key prefixes, and complete managed
  application connection environments for shared cache containers.
- Added bounded live Redis and Valkey ACL reloads with implementation-specific
  CLI authentication environments, explicit server-error exit propagation, and
  raw admin credentials excluded from command arguments and diagnostics.
- Added deterministic RabbitMQ core definitions with project-isolated virtual
  hosts and users, official salted SHA-256 password encoding, collision
  rejection, stable ordering, and no raw credentials in rendered JSON.
- Added executable RabbitMQ shared-instance plans with atomic hash-only
  definitions mounts, core boot import, stable node identity, private
  networking, pinned Linux images, and retained broker data volumes.
- Added RabbitMQ project composition with deterministic virtual-host users,
  durable credentials, and complete managed AMQP connection environments for
  project applications sharing a compatible broker.
- Added bounded live RabbitMQ core-definition imports through the broker's
  built-in CLI so project vhosts and users converge without management plugins,
  host tools, credentials, or shared-container restarts.
- Added shared MongoDB instance and logical-resource plans with private
  file-backed root initialization, database-scoped read-write users, idempotent
  create/update scripts over stdin, and retained owned data volumes.
- Added atomic immutable managed-secret persistence with exact-content
  reconciliation and user-only Unix directory and file permissions for
  file-backed container bootstrap credentials.
- Added complete MongoDB project composition and bounded provisioning with
  stable credentials, managed connection environments, database-scoped roles,
  and both administrative and project secrets confined to redacted stdin.
- Added deterministic v8 shared-service compatibility fingerprints covering
  implementation, major, digest, extensions, settings, persistence, isolation,
  and platform architecture.
- Added structured v8 Engine operation deadlines across negotiation, lifecycle,
  inspection, and discovery so stalled requests remain bounded and cancellable.
- Added a narrow v8 container discovery capability with direct Engine API
  rescans, managed-label filtering, and backend-independent observations.
- Added v8 observed-resource ownership classification that reconstructs complete
  labels while isolating unmanaged, foreign, incompatible, and malformed objects.
- Added strict v8 project, service, and route identities that preserve valid
  input exactly and reject normalization or overlong route labels.
- Added all-at-once v8 route registry validation so repeated discovery of one
  canonical path deduplicates while ownership collisions fail with every path.
- Added a strict v8 `.stackctl.yaml` parser contract covering the canonical
  service map, exact scalar types, schema version, and unsafe YAML features.
- Added pure v8 desired-project resolution with exact identities, dependency
  validation, stable ordering, and complete cycle diagnostics.
- Added a transactional v8 SQLite state-store contract for durable project and
  route ownership, atomic migrations, restart recovery, and interrupted writes.
- Added an OS-backed per-user v8 daemon lease contract with exclusive ownership,
  stale-PID-independent recovery, and user-only Unix permissions.
- Added a versioned v8 local IPC frame contract with typed requests, request
  IDs, cancellation, strict decoding, and a user-only Unix socket listener.
- Added a v8 multi-project application boundary that plans the complete
  discovered registry before atomically persisting any project ownership.
- Added the v8 Engine capability boundary with mandatory installation ownership
  metadata and replaceable container lifecycle strategies.
- Added a nonblocking direct Docker-compatible API adapter for Docker Unix
  sockets and Windows named pipes, also usable with Podman's compatible API.
- Added the normative v8 two-plane architecture, strict YAML and deterministic
  naming rules, complete service-sharing matrix, operational policies,
  independently verifiable milestones, and 40-project benchmark protocol.
- Added dual-format Stackctl config support so project discovery, loading, and
  saving now autodetect `.stackctl.toml` and `.stackctl.yaml` files while still
  rejecting the `.yml` extension.
- Added top-level `stackctl phpstan`, `stackctl ecs`, `stackctl php-cs-fixer`, and
  `stackctl psalm` wrappers so common PHP quality tools can run directly inside
  the selected app container without going through `stackctl exec`.
- Added top-level `stackctl pint`, `stackctl pest`, `stackctl phpunit`, and
  `stackctl rector` wrappers so common PHP formatting, test, and refactor tools
  can run directly inside the selected app container.
- Added `opensearch` and `elasticsearch` search service presets with local
  single-node defaults, inferred Scout env wiring, named data volumes, and
  shared runtime handling alongside existing search backends.
- Added project-wide `domain_strategy` config with `directory` and `random`
  modes so app services can resolve `.stackctl` domains automatically without
  repeating explicit per-service `domain` entries.
- Added service-level `restart` config with Docker-compatible policies so
  projects can opt out of or override Stackctl's default restart behavior.
- Added a `stackctl daemon` command group with `start`, `status`, `stop`, and
  `logs` subcommands that resolve target projects from an explicit `--path`
  before normal config loading.
- Added per-project daemon session persistence under `~/.config/stackctl/daemon/`
  so daemon commands can track pid and log metadata across separate CLI runs.
- Added `stackctl daemon watch` with repeatable `--dir` roots, `--once`, and
  `--interval` so Stackctl can discover `.stackctl.toml` projects and start missing
  per-project daemons automatically.
- Added `stackctl daemon service <install|status|print|uninstall>` so users can
  install a login-time watch service through `launchd` or `systemd --user`
  without hand-writing unit definitions.
- Added watch policy flags for `stackctl daemon watch` and installed daemon watch
  services so users can exclude subtrees and cap the number of auto-started
  projects under broad parent directories.

### Changed

- Changed durable project-command payloads to retain only command identity and
  intent. Managed environment values are now rehydrated from current in-memory
  control-plane state immediately before Engine execution, preventing database
  passwords and other generated credentials from being copied into operation
  rows while remaining compatible with already queued payloads.
- Changed prepared PostgreSQL tenant records to use the actual deterministic
  database name as their logical data identity and the lifecycle adapter's
  `postgres_database_and_role` kind, allowing backup and migration strategies
  to consume the same durable resource contract as shared provisioning.
- Changed v8 Engine socket connection to negotiate API compatibility and reject
  versions older than 1.41 before reconciliation can begin.
- Completed v8 Engine ownership labels with stable project IDs, compatibility
  fingerprints, schema versions, desired revisions, and retention classes.
- Renamed the project, CLI, config files, env vars, Docker labels, and
  user-facing documentation to `stackctl`.
- Updated direct Rust dependencies in `Cargo.toml` to the latest available
  release versions and refreshed the resolved Cargo lockfile to match.
- Changed Rust dependencies and CI workflow action pins to their latest
  available releases, including `tabled 0.21`, refreshed transitive lockfile
  versions, and `actions/checkout@v5` in the GitHub Actions pipeline.
- Changed `stackctl init` to write `domain_strategy = "directory"` and rely on
  automatic `.stackctl` domain generation for app services instead of emitting an
  explicit `domain = "...localhost"` entry in new configs.
- Changed Docker `run` generation to default Stackctl-managed services to
  `--restart unless-stopped`, improving recovery after Docker restarts
  and laptop sleep without requiring a manual `stackctl up`.
- Changed the daemon child path to bootstrap projects through the existing
  `start` flow, then supervise service containers by polling for non-running
  containers and retrying recovery with exponential backoff.
- Changed daemon project discovery to deduplicate overlapping watch roots,
  skip nested child projects once a valid parent project is managed, and
  report invalid `.stackctl.toml` files without stopping sibling discovery.

### Fixed

- Fixed parallel Docker, post-restore, health, and doctor tests reusing the same
  temporary path when the system clock returns an identical timestamp.
- Fixed CI `clippy` failures in daemon and artisan runtime helpers by
  removing panic-prone `expect()` usage from production code and marking
  the long-running daemon supervisor loop as intentional.
- Fixed repeated `stackctl artisan test --browser` Playwright browser downloads by
  caching browser binaries under `.stackctl/cache/playwright` in the mounted
  workspace and reusing that cache across reset test-runtime containers.
- Fixed repeated `stackctl artisan test --browser` system dependency installs by
  baking Playwright's `install-deps chromium` step into the cached derived app
  image used for browser test runtimes instead of rerunning it inside each
  fresh test container.
- Fixed `stackctl artisan test --browser` mixed-service projects such as
  `app + mailhog` so browser-runtime targeting stays scoped to FrankenPHP app
  services and does not route other app presets through PHP module inspection.
- Fixed Stackctl's default PHP memory limit for derived app images and
  `stackctl artisan test` runs to `4096M`, reducing coverage-run failures
  caused by the previous `2048M` ceiling.
- Fixed `stackctl artisan test` runtime planning to automatically include the
  `pcov` PHP extension in derived app images so Laravel coverage runs have a
  driver available without repeating `php_extensions = ["pcov"]` in app
  config.
- Fixed app-service startup waits to stop forcing HTTP `/up` probes onto
  non-HTTP worker presets such as Horizon, while adding default HTTP health
  paths for `frankenphp`, `reverb`, and `soketi` so `start --wait` uses the
  correct readiness checks for each app preset.
- Fixed `stackctl restore --service <db> --migrate` and `--schema-dump` to
  resolve and pass the restored service's Laravel database connection
  into `stackctl artisan`, and clarified post-restore logs to print the exact
  `--database=...` connection used for each restored service.
- Fixed Docker heavy-operation slot scheduling to reclaim stale PID lock
  files and wait long enough for slow runtime cleanup, preventing random
  `stackctl artisan test` setup failures while `docker rm` or `docker volume rm`
  is queued behind abandoned or long-running heavy ops.
- Fixed `stackctl artisan test` pooled-runtime stale-slot probing to silence
  expected `kill -0` stderr for dead PIDs, preventing random `kill: <pid>:
  No such process` noise during normal slot reclamation.
- Fixed standalone lifecycle commands such as `stackctl up` to treat missing
  workspace swarm context as a no-op for project dependencies, and fixed
  `--no-deps` to skip workspace swarm injected-env resolution entirely,
  so non-workspace projects no longer fail unless they actually opt into
  swarm wiring.
- Fixed app serve/open/down routing decisions to fall back to the local
  published endpoint when no domain is configured, so legacy projects can
  keep app domains optional instead of failing through the Caddy path.
- Fixed swarm child command construction to append `--no-deps` for nested
  `up`, `recreate`, `start`, and `down` invocations, preventing child Stackctl
  processes from re-resolving workspace dependencies that the parent swarm
  run already planned.
- Fixed object-store bucket bootstrap to prefer running the AWS CLI helper in
  the target container's network namespace and reach the service over
  `localhost`, while falling back to the legacy host-gateway path for
  published-port compatibility and runtimes that do not support
  `--network container:...`.
- Fixed random-port startup planning to reuse the published host port of an
  already running container, preventing `up` and `recreate` flows from
  reporting a new random port that the live service is not actually using.
- Fixed HTTP status probing to parse the trailing curl status line after
  preserving the response body, so doctor and open-summary checks can read
  status codes reliably without discarding probe output.
- Fixed app health-check timeout errors to include the last observed HTTP
  status, response body, or probe transport error, so `stackctl swarm recreate`
  now reports actionable diagnostics for failures such as `502 Bad Gateway`.
- Fixed Docker build-slot scheduling to wait long enough for real derived
  image builds, preventing sibling swarm targets from failing after a short
  slot-acquisition timeout while another app image is still building.
- Fixed swarm target stream forwarding to drop blank child-output lines, and
  clarified derived-image build queue logging so queued targets report that
  they are waiting for a Docker build slot before the actual build begins.
- Fixed Gotenberg health validation to evaluate the raw JSON response body
  instead of a truncated display summary, preventing healthy `200 {"status":
  "up"}` readiness responses from being misclassified as failures.
- Fixed app health-check timeout diagnostics for proxy-backed `.stackctl` URLs to
  explain that `502` means Caddy reached the public route but the upstream app
  container was not yet serving valid HTTP.
- Fixed Caddy reload fallback to stop any existing process before starting a
  replacement, preventing duplicate `caddy run` instances from keeping stale
  upstream port mappings alive after app ports change.
- Fixed parallel swarm Caddy apply races by serializing route-state writes and
  reload/start operations through one lock, preventing concurrent targets from
  spawning overlapping `caddy run` processes on `:443`.
- Fixed Caddy apply locking to recover stale lock files and wait long enough
  for queued updates, preventing abandoned `apply.lock` files or normal
  contention from causing downstream swarm targets to fail.
- Fixed Caddy apply to stage and validate config changes before replacing
  live files, then roll back and recover interrupted backup state so failed
  or crashed updates do not leave Caddy stopped with stale routes or a
  half-written config directory.
- Fixed Caddy command execution to capture output through temporary files
  instead of inherited pipes, preventing `stackctl ... recreate` from hanging
  when `caddy reload` or `caddy start` leaves descendant processes holding
  stdout or stderr open.
- Refactored Caddy temporary-file subprocess capture into a dedicated
  module so lifecycle code stays focused on reload, start, and trust
  behavior without changing runtime behavior.
- Fixed CI stability for Docker host-gateway args tests by pinning their
  runtime engine explicitly, and replaced panicking Node toolchain
  `expect()` paths with actionable errors that satisfy strict Clippy
  `expect_used` checks.

## [6.0.0] - 2026-03-06

### Added

- Added service-level JavaScript toolchain configuration via
  `[service.javascript]`
  with `runtime`, `package_manager`, `version_manager`, and `version`
  fields so app runtimes can resolve Node, Bun, or Deno execution
  explicitly.
- Added `stackctl node --package-manager`, `--version-manager`, and
  `--node-version`, plus matching overrides for `stackctl task deps bump`,
  so callers can choose package-manager and version-manager behavior per
  invocation.
- Added `stackctl bun` with `--bun-version`, plus Bun runtime detection from
  `bun.lock`, `bun.lockb`, and `package.json.packageManager`.
- Added `stackctl deno` with `--deno-version`, plus Deno runtime detection
  from `deno.json`, `deno.jsonc`, and `deno.lock`.
- Added explicit `stackctl task deps bump --node`, `--bun`, and `--deno`
  selectors so dependency workflows map directly to the runtime being
  managed.
- Added `stackctl task deps audit`, `stackctl task deps normalize`, and
  `stackctl task deps install` so app dependency maintenance now covers
  vulnerability checks, manifest normalization, and install workflows
  across Composer plus the explicit Node, Bun, and Deno runtime
  selectors already used by `stackctl task deps bump`.
- Added project-file Node inference for `package.json.packageManager`,
  `package.json.volta.node`, `.nvmrc`, `.node-version`, and
  `package.json.engines.node` so Node workflows can derive toolchain
  settings from existing repo metadata.

### Changed

- Changed app runtime image generation to install the selected Node
  version-manager, Bun runtime, or Deno runtime instead of always
  installing a hardcoded NodeSource LTS toolchain under the hood.
- Changed Node package-manager execution to resolve from config and
  project metadata instead of defaulting `stackctl node` to `bun`.
- Changed JavaScript runtime naming from `JsRuntime` to
  `JavaScriptRuntime`, and split Bun into its own `stackctl bun` command
  instead of treating it as a Node package-manager option.
- Changed the shared runtime domain module from `src/node/` to
  `src/javascript/` so internal naming matches the broader
  JavaScript-runtime scope.
- Changed the service config section name from `[service.node]` to
  `[service.javascript]` so configuration naming matches Bun and Deno
  support. This is a breaking config change.
- Changed Node-related CLI flags by removing `--manager` in favor of the
  explicit `--package-manager` name. This is a breaking CLI change.

### Fixed

- Fixed flaky Rust tests that mutate Docker runtime test state by
  serializing dry-run, engine, and fake-command overrides through one
  shared test scope.
- Fixed inferred Laravel app runtime env to inject
  `LIVEWIRE_TEMPORARY_FILE_UPLOAD_DISK=local` by default so HTTPS app domains
  avoid mixed-content failures during Livewire temporary uploads when using
  local S3-compatible object stores.
- Fixed object-store startup reliability by automatically ensuring configured
  buckets exist during `up` flows for object-store services.
- Fixed `stackctl artisan` with no explicit subcommand to run `php artisan list`
  by default, so command discovery works without requiring `-- <command>`.
- Fixed `stackctl artisan` runtime env composition to include swarm
  `inject_env` dependency values, so injected `*_API_BASE_URL` overrides are
  applied consistently in artisan command execution.
- Fixed package-manager wrapper defaults so `stackctl composer` now runs
  `composer list` when no subcommand is provided, and `stackctl node` now
  executes the selected package manager without forcing explicit args.

## [5.0.0] - 2026-03-09

### Added

- Added `stackctl task deps bump` for opinionated dependency maintenance
  workflows in app containers, with `--composer`, `--node`, and `--all`
  targets, lockfile-based Node package-manager inference, and documented
  manifest-skip behavior when `composer.json` or `package.json` is absent.

### Changed

- Changed `recreate` default behavior to wait for healthy services before
  finishing, with explicit opt-out available via `--no-wait`.
- Changed config project-mode resolution to require an explicit
  `project_type` outcome from `.stackctl.toml` or `composer.json` `type`
  (`project`/`library`), with hard failure when neither source resolves.
- Changed Laravel-only workflows to respect project mode: `start` skips
  Laravel bootstrap for `library` projects, and `artisan`/`app-create` are
  rejected unless `project_type = "project"`.

### Fixed

- Fixed object-store bucket bootstrap on Linux by adding host-gateway mapping
  for bootstrap helper containers.
- Fixed `stackctl swarm recreate` hangs after healthy targets by bounding Caddy
  reload/start command execution and output-drain waits, so swarm runs now
  complete instead of blocking indefinitely on stuck child process I/O.
- Fixed object-store startup flakiness by improving readiness handling around
  bucket bootstrap timing and service health dependencies.
- Fixed `stackctl artisan test` ZPL conversion runtime dependencies by installing
  `ghostscript` (`gs`) in default derived app images.
- Fixed derived image cache invalidation by hashing rendered Dockerfile
  content, so runtime dependency/template changes rebuild automatically.
- Fixed Linux `/etc/hosts` update reliability by hardening privileged append
  escalation paths (`sudo`/`pkexec`) and retry behavior for permission-related
  failures including read-only filesystem edge cases.
- Fixed app runtime env defaults to always provide `APP_NAME` when absent, to
  prevent runtime metadata and health regressions in app services that require
  it.
- Fixed serve health checks for Gotenberg by preserving and validating
  response body content instead of status-only probes.
- Fixed serve app container local networking by mapping configured local
  domains and injected local peer domains to host-gateway, preventing
  container-local loopback resolution failures for self and swarm calls.

## [4.0.0] - 2026-02-24

### Added

- Added container runtime engine selection via `.stackctl.toml` using
  `container_engine = "docker"` or `container_engine = "podman"`.
- Added global CLI override `--engine <docker|podman>` to select runtime
  engine for the current invocation.

### Changed

- Changed runtime command execution to resolve the container CLI binary from
  selected engine (`docker` or `podman`) through shared command runners.
- Changed host-loopback alias resolution to be engine-aware:
  Docker uses `host.docker.internal` and Podman uses
  `host.containers.internal`.

### Migration Notes

- Existing configs remain valid. If `container_engine` is omitted, Stackctl
  defaults to `docker`.
- Podman support targets core Docker-compatible CLI flows. Some advanced
  behavior may still differ across host/network/runtime setups.

## [3.6.0] - 2026-02-23

### Added

- Added automatic Playwright bootstrap for `stackctl artisan test` when
  `pestphp/pest-plugin-browser` is detected in `composer.json`, including:
  `npm install playwright@latest`, `npx playwright install-deps`, and
  `npx playwright install`.

### Changed

- Changed adaptive default test runtime pool sizing from small fixed tiers to
  host-aware `8/16/32` slot tiers, using Docker CPU/memory hints when
  available.
- Changed Laravel app preset PHP extension defaults to include `sockets` so
  Pest browser plugin socket helpers are available in test containers.

### Fixed

- Fixed `stackctl artisan test` browser test failures caused by missing PHP
  `sockets` extension in app test runtime containers.
- Fixed `PlaywrightOutdatedException` in containerized browser tests by
  ensuring Playwright package and browser/runtime dependencies are provisioned
  before test execution.
- Fixed `stackctl artisan test` config loading to prefer `.stackctl.testing.toml`
  when present, with fallback to `.stackctl.toml`, so test runtimes no longer
  start services that were excluded from testing config.

## [3.5.0] - 2026-02-22

### Added

- Added `garage` object-store preset as a MinIO alternative for local
  S3-compatible storage workflows.

### Changed

- Changed object-store preset and runtime wiring to include Garage driver
  support across preset expansion, env inference, and runtime defaults.

## [3.4.0] - 2026-02-21

### Added

- Added adaptive `stackctl artisan test` runtime pool sizing based on available
  host resources with optional Docker resource hints.
- Added deterministic workspace-scoped runtime naming for pooled test runs to
  isolate concurrent workspaces safely.

### Changed

- Changed test runtime pool-size resolution precedence to:
  explicit override, then `STACKCTL_TEST_RUNTIME_POOL_SIZE`, then adaptive sizing.
- Changed pooled runtime lock wait behavior to allow longer acquisition under
  heavier concurrent test load.

### Fixed

- Fixed pooled runtime container-name collisions across different workspaces
  by including workspace identity in runtime environment names.

## [3.3.0] - 2026-02-20

### Added

- Added env-scoped config discovery for `--env <name>` so Stackctl now prefers
  `.stackctl.<env>.toml` when present and falls back to `.stackctl.toml`.
- Added Docker runtime policy controls for operation scheduling:
  `--docker-max-heavy-ops`, `--docker-max-build-ops`,
  and `--docker-retry-budget`.
- Added `--test-runtime-pool-size` for `stackctl artisan test` runtime pooling.

### Changed

- Changed `stackctl artisan test` runtime namespace allocation to use pooled
  runtime leases, reducing unbounded parallel test runtime churn.
- Changed heavy Docker execution paths to use scheduler gating for build and
  cleanup operations.
- Changed CLI bootstrap/config loading to support env-specific config file
  selection while preserving existing fallback behavior.

### Fixed

- Fixed derived-image lockfile write race conditions by adding lock-file
  coordination and atomic lockfile replacement.

## [3.2.0] - 2026-02-20

### Added

- Added selector parity across operational command families by supporting
  `--profile` on `pull`, `health`, `logs`, `relabel`, `top`, `stats`,
  `inspect`, `kill`, `pause`, `unpause`, `wait`, and `port`.
- Added repeatable `--service` selection across the same operational command
  families so multiple explicit targets can be selected in one invocation.
- Added output-format parity flags in docker-ops output flows:
  `stackctl port --format json` and `stackctl inspect --output json`.

### Changed

- Changed command dispatch and handler selection wiring to route operational
  command execution through shared service-scope filtering with profile and
  multi-service support.
- Changed JSON output handling for `port` and `inspect` to keep existing
  `--json` behavior while accepting the new format-style flags.
- Changed release metadata to `3.2.0`.

### Migration Notes

- `port` JSON output:
  - previous: `stackctl port --json`
  - now also supported: `stackctl port --format json`
- `inspect` JSON output:
  - previous: `stackctl inspect --json`
  - now also supported: `stackctl inspect --output json`
- Profile-based selection on ops commands:
  - previous: commands commonly required explicit `--service`/`--kind`
  - now also supported: `--profile <infra|data|app|web|api|full|all>`
- Multi-target service selection:
  - previous: single `--service <name>`
  - now also supported: repeat `--service` (for example
    `--service db --service cache`)

## [3.1.0] - 2026-02-20

### Added

- Added global `--non-interactive` mode to disable interactive behaviors in
  automation contexts, including browser-open and TTY-dependent command paths.
- Added JSON output support for diagnostics commands:
  `stackctl doctor --format json`, `stackctl health --format json`, and
  `stackctl about --format json`.
- Added lifecycle selector parity by supporting `--profile` on
  `stackctl down`, `stackctl stop`, `stackctl rm`, `stackctl recreate`, and
  `stackctl restart`.
- Added repeatable `--service` selection for lifecycle and diagnostics paths
  so multiple explicit services can be targeted in one invocation.
- Added `stackctl logs --since <VALUE>` and `stackctl logs --until <VALUE>` to align
  with Docker log time-window filtering behavior.
- Added stop-timeout controls for teardown flows via
  `stackctl stop --timeout <SECONDS>` and `stackctl down --timeout <SECONDS>`.
- Added selector parity (`--kind` and `--profile`) across app/runtime command
  families: `exec`, `artisan`, `composer`, `node`, `serve`, and `open`.

### Changed

- Changed shared service-selection internals to route more command families
  through common filter/profile resolution, reducing per-command selector
  drift.
- Changed command dispatch wiring and usage docs to keep new selector and
  output-format capabilities consistent and discoverable.

## [3.0.0] - 2026-02-17

### Added

- Added `stackctl share` tunnel management with `start`, `status`, and `stop`
  subcommands for app services, including provider-backed session tracking,
  persisted runtime metadata, and JSON/text output modes.
- Added Expose client sharing support to `stackctl share` via
  `--provider expose` and `--expose`.
- Added share provider shorthand flags `--cloudflare`, `--expose`, and
  `--tailscale` as alternatives to
  `--provider <cloudflare|expose|tailscale>` across share flows.

### Changed

- Changed random-port allocation and runtime host normalization to scope
  allocation, remapping, and conflict tracking by bind host, improving
  mixed-host and wildcard host behavior.
- Changed doctor port validation coverage to include SMTP listeners and use
  shared host-aware checks across startup and recreate flows.
- Changed swarm target argument validation to enforce consistent parallel
  execution guard behavior and remove duplicate forwarding paths.

### Fixed

- Fixed release build breakage after refactors by repairing visibility
  regressions in internal modules.
- Fixed `stackctl artisan test` runtime isolation so test startup now derives
  injected env values after test-port remapping and keeps app targets on
  localhost TLS, preventing test containers from corrupting active dev
  runtime networking (for example Redis host/port reachability).
- Fixed `stackctl artisan test` runtime cleanup to force-remove prior test
  containers and purge reusable named volumes before startup, ensuring each
  run starts from fresh service state.
- Fixed random-port runtime flow failures in CLI orchestration by repairing
  compilation and remap control paths used for service startup.
- Fixed SMTP remap handling to avoid duplicate host-port collisions and ensure
  doctor conflict checks include SMTP service ports.
- Fixed wildcard IPv6 host detection in config/runtime normalization so host
  binding behavior remains consistent across port allocation paths.
- Fixed swarm clone target execution to preserve `git clone` repository and
  destination argument order.
- Fixed serve-mode trust-info emission to use the selected options target,
  ensuring status output is routed to the correct service context.

## [2.0.0] - 2026-02-14

### Added

- Added automatic host-gateway alias injection
  (`host.docker.internal:host-gateway`) for service/container runs that
  require host-loopback reachability.
- Added default persistent named-volume mounts for stateful backends when no
  explicit volume mapping is configured.
- Added host-level port occupancy checks to `stackctl doctor` so startup conflicts
  are detected before container launch.
- Added Laravel runtime bootstrap to `stackctl start` for app targets:
  `storage:link`, `migrate`, and conditional `key:generate` when `APP_KEY`
  is missing.

### Changed

- Changed startup behavior to prioritize first-run app readiness as part of
  the default `stackctl start` flow.
- Changed zero-config persistence defaults so common stateful services survive
  routine stop/down/recreate cycles without manual volume setup.
- Changed Linux compatibility for host-service access by making loopback host
  alias wiring automatic in Docker run paths.

### Fixed

- Fixed `stackctl start --env <name>` key bootstrap detection to inspect the
  effective runtime env file (`.env.<name>`) instead of always reading `.env`.
- Fixed duplicate Laravel bootstrap execution across multi-app profiles by
  targeting only the primary web app container (FrankenPHP app target with no
  custom command).
- Fixed Laravel bootstrap migration commands to run with `--isolated`,
  preventing concurrent migration execution when multiple app processes start
  in parallel.
- Fixed doctor host-port validation to probe the configured bind host directly
  without fallback to `127.0.0.1`, preventing false negatives on non-loopback
  host bindings.

## [1.9.0] - 2026-02-13

### Added

- Added `dragonfly` cache preset with DragonflyDB image defaults and
  Redis-compatible runtime behavior.
- Added `sqlserver` database preset with `mssql` alias and SQL Server image
  defaults.
- Added `localstack` object-store preset with S3-oriented default settings.
- Added SQL Server (`sqlsrv://`) connection URL support for service runtime
  summaries and tooling output.

### Changed

- Changed driver/runtime mapping coverage to include `dragonfly`,
  `sqlserver`, and `localstack` across preset expansion, inferred env output,
  default ports, and health checks.
- Changed SQL driver gating so SQL Server is treated as a database backend,
  while dump/restore SQL-admin operations remain scoped to Postgres/MySQL.

### Fixed

- Fixed SQL restore failure handling to consume stderr with
  `wait_with_output`, preventing potential deadlocks and preserving actionable
  restore error output (including non-UTF8 payloads).
- Fixed RabbitMQ preset default coverage to assert zero-config broker creds via
  `username`/`password` defaults (`guest`/`guest`), matching runtime env
  injection behavior.
- Fixed serve teardown reporting to fail on real `docker stop`/`docker rm`
  errors instead of always reporting success.
- Fixed serve teardown behavior for already-absent containers by treating
  `No such container` as a non-fatal state.
- Fixed managed env update behavior to error when the target env file is
  missing, instead of silently no-oping.
- Fixed env file writers to escape quotes, backslashes, and control characters
  so generated `.env` entries remain valid.
- Fixed service connection URL construction to percent-encode credentials and
  path components.
- Fixed service connection URL host formatting to bracket raw IPv6 literals.

## [1.8.0] - 2026-02-13

### Fixed

- Fixed `stackctl artisan test` runtime startup to assign random host ports by
  default (matching `stackctl up`) and recreate remapped services automatically,
  preventing host-port collision failures.

## [1.7.0] - 2026-02-13

### Fixed

- Fixed swarm dependency `inject_env` `:port`/`:url` resolution to use live
  Docker host-port bindings when available, with fallback to configured ports.
- Fixed `stackctl open` database status URLs to report runtime published ports
  instead of static config ports.
- Fixed Caddy reverse-proxy forwarding for app routes to always send HTTPS
  `X-Forwarded-*` headers, preventing login/form redirects from downgrading
  to `http://`.
- Fixed serve container env precedence so inferred HTTPS `APP_URL` and
  `ASSET_URL` cannot be downgraded by explicit `service.env` `http://`
  overrides.
- Fixed `stackctl up` app env merge precedence so swarm/project dependency
  injected values cannot downgrade inferred HTTPS `APP_URL`/`ASSET_URL` to
  `http://`.

## [1.6.0] - 2026-02-13

### Added

- Added service lifecycle hooks in `.stackctl.toml` via `[[service.hook]]` with
  `post_up`, `pre_down`, and `post_down` phases.
- Added hook run modes for container `exec` commands and host `script`
  commands with optional `on_error` behavior (`fail` or `warn`).
- Added lifecycle integration so configured hooks run during `stackctl up`,
  `stackctl apply`, and `stackctl down` for selected targets.
- Added usage and init-template examples for hook configuration.

### Fixed

- Fixed `stackctl recreate --publish-all` to support `--parallel > 1`.
- Fixed `stackctl swarm recreate` to work with default random-port publishing
  under parallel target execution.

## [1.5.0] - 2026-02-13

### Added

- Added `pgsql` as a PostgreSQL preset alias (alongside `postgres` and `pg`).
- Added Laravel queue worker app preset:
  `queue-worker` (with `queue` alias), using
  `php artisan queue:work` defaults.

## [1.4.0] - 2026-02-13

### Added

- Added Laravel worker/runtime presets:
  `reverb`, `horizon`, `scheduler`, and `dusk`.
- Added app-service presets and aliases for broader Laravel local stacks:
  `selenium`, `mailpit`, `rabbitmq`, and `soketi`.
- Added infrastructure presets for common Laravel-adjacent backends:
  `mongodb` and `memcached`.
- Added Scout-oriented env defaults for search presets so `meilisearch` and
  `typesense` inject expected Laravel variables by default.

### Changed

- Changed startup defaults and usage documentation to align generated config
  and command behavior.
- Changed preset coverage and docs to reflect new Laravel service options,
  including Mailpit and standalone Selenium naming.

### Removed

- Removed `laravel-full` preset alias because it duplicated `laravel`
  behavior without adding distinct defaults.
- Removed `laravel-minimal` preset because it no longer matched active
  project usage.

## [1.2.0] - 2026-02-13

### Added

- Added formal Stackctl ownership labels to created containers:
  `com.stackctl.managed`, `com.stackctl.service`, `com.stackctl.kind`, and
  `com.stackctl.container`.
- Added `stackctl relabel` command to migrate existing containers by recreating
  selected services and applying Stackctl ownership labels.
- Added label-aware ownership checks before scoped prune removes containers.
- Added command polish flags for Docker parity:
  `stackctl cp` now supports `-L/--follow-link` and `-a/--archive`.
- Added command polish flags for Docker parity:
  `stackctl inspect` now supports `--size` and `--type`.
- Added command polish flags for Docker parity:
  `stackctl attach` now supports `--detach-keys`.
- Added structured JSON output modes:
  `stackctl inspect --json`, `stackctl port --json`, and `stackctl events --json`.
- Added `stackctl events --allow-empty` to explicitly allow empty service
  selections without failing.

### Changed

- Changed default `stackctl events` scoping from name filters to label-based Stackctl
  ownership filters.
- Changed `stackctl events` behavior to return non-zero when no services match,
  unless `--allow-empty` is supplied.
- Changed `stackctl events --all` behavior to print an explicit warning before
  streaming global daemon events.
- Changed default `stackctl prune` behavior to enforce Stackctl-scoped cleanup
  semantics with ownership validation.
- Changed `stackctl prune --all` behavior to require an explicit `--force` guard
  before global Docker prune execution, with warning output when used.
- Changed global prune dry-run behavior to preview candidate stopped containers
  before execution.
- Changed command help text to call out scoped-vs-global behavior for `events`
  and `prune`.
- Changed `events` and `prune` help output to include concrete usage examples.

## [1.1.0] - 2026-02-13

### Added

- Added Docker passthrough commands for service containers:
  `top`, `stats`, `inspect`, `attach`, `cp`, `kill`, `pause`, `unpause`,
  `wait`, `events`, `port`, and `prune`.
- Added `service:/path` shorthand resolution for `stackctl cp` endpoints.
- Added Docker-compatible flags:
  `stackctl cp` now supports `-L/--follow-link` and `-a/--archive`.
- Added Docker-compatible flags:
  `stackctl inspect` now supports `--size` and `--type`.
- Added Docker-compatible flags:
  `stackctl attach` now supports `--detach-keys`.
- Added scoped selectors for event/prune workflows:
  `stackctl events` now supports `--service` and `--kind`.
- Added scoped selectors for event/prune workflows:
  `stackctl prune` now supports `--service`, `--kind`, and `--parallel`.

### Changed

- Changed restore file-progress output from an in-place status bar to
  persistent incremental log events with percent and MiB totals.
- Changed `stackctl events` default behavior to Stackctl container scope by applying
  container filters from resolved service names.
- Changed `stackctl events` UX to require explicit `--all` for global daemon
  event streaming.
- Changed `stackctl prune` default behavior to remove only stopped
  Stackctl-configured service containers (safe by default).
- Changed `stackctl prune` UX to require explicit `--all` for global
  `docker container prune` behavior.

## [1.0.0] - 2026-02-12

### Added

- Stable `1.0.0` release line for Stackctl.
- `INSTALLATION.md` with explicit install and verification steps.
- Release changelog baseline.

### Changed

- Refined release README messaging around zero-BS, low-config Laravel
  orchestration.
- Updated `stackctl init` defaults to a smaller preset-driven config.
- Fixed generated `stackctl init` template output so it writes valid TOML.
- Updated package metadata and versioning to `1.0.0`.
- Improved `stackctl artisan test` failure reporting to include the app
  container exit code when `docker exec` returns non-zero after test
  output has already been streamed.
- Fixed `stackctl artisan` serve-container failures to report the actual
  `docker exec` exit code instead of the generic container error.
