# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Fixed

- Fixed inferred Laravel app runtime env to inject
  `LIVEWIRE_TEMPORARY_FILE_UPLOAD_DISK=local` by default so HTTPS app domains
  avoid mixed-content failures during Livewire temporary uploads when using
  local S3-compatible object stores.
- Fixed object-store startup reliability by automatically ensuring configured
  buckets exist during `up` flows for object-store services.

## [4.1.0] - 2026-02-27

### Changed

- Changed `recreate` default behavior to wait for healthy services before
  finishing, with explicit opt-out available via `--no-wait`.

### Fixed

- Fixed object-store bucket bootstrap on Linux by adding host-gateway mapping
  for bootstrap helper containers.
- Fixed object-store startup flakiness by improving readiness handling around
  bucket bootstrap timing and service health dependencies.
- Fixed `helm artisan test` ZPL conversion runtime dependencies by installing
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

- Added container runtime engine selection via `.helm.toml` using
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

- Existing configs remain valid. If `container_engine` is omitted, Helm
  defaults to `docker`.
- Podman support targets core Docker-compatible CLI flows. Some advanced
  behavior may still differ across host/network/runtime setups.

## [3.6.0] - 2026-02-23

### Added

- Added automatic Playwright bootstrap for `helm artisan test` when
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

- Fixed `helm artisan test` browser test failures caused by missing PHP
  `sockets` extension in app test runtime containers.
- Fixed `PlaywrightOutdatedException` in containerized browser tests by
  ensuring Playwright package and browser/runtime dependencies are provisioned
  before test execution.
- Fixed `helm artisan test` config loading to prefer `.helm.testing.toml`
  when present, with fallback to `.helm.toml`, so test runtimes no longer
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

- Added adaptive `helm artisan test` runtime pool sizing based on available
  host resources with optional Docker resource hints.
- Added deterministic workspace-scoped runtime naming for pooled test runs to
  isolate concurrent workspaces safely.

### Changed

- Changed test runtime pool-size resolution precedence to:
  explicit override, then `HELM_TEST_RUNTIME_POOL_SIZE`, then adaptive sizing.
- Changed pooled runtime lock wait behavior to allow longer acquisition under
  heavier concurrent test load.

### Fixed

- Fixed pooled runtime container-name collisions across different workspaces
  by including workspace identity in runtime environment names.

## [3.3.0] - 2026-02-20

### Added

- Added env-scoped config discovery for `--env <name>` so Helm now prefers
  `.helm.<env>.toml` when present and falls back to `.helm.toml`.
- Added Docker runtime policy controls for operation scheduling:
  `--docker-max-heavy-ops`, `--docker-max-build-ops`,
  and `--docker-retry-budget`.
- Added `--test-runtime-pool-size` for `helm artisan test` runtime pooling.

### Changed

- Changed `helm artisan test` runtime namespace allocation to use pooled
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
  `helm port --format json` and `helm inspect --output json`.

### Changed

- Changed command dispatch and handler selection wiring to route operational
  command execution through shared service-scope filtering with profile and
  multi-service support.
- Changed JSON output handling for `port` and `inspect` to keep existing
  `--json` behavior while accepting the new format-style flags.
- Changed release metadata to `3.2.0`.

### Migration Notes

- `port` JSON output:
  - previous: `helm port --json`
  - now also supported: `helm port --format json`
- `inspect` JSON output:
  - previous: `helm inspect --json`
  - now also supported: `helm inspect --output json`
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
  `helm doctor --format json`, `helm health --format json`, and
  `helm about --format json`.
- Added lifecycle selector parity by supporting `--profile` on
  `helm down`, `helm stop`, `helm rm`, `helm recreate`, and
  `helm restart`.
- Added repeatable `--service` selection for lifecycle and diagnostics paths
  so multiple explicit services can be targeted in one invocation.
- Added `helm logs --since <VALUE>` and `helm logs --until <VALUE>` to align
  with Docker log time-window filtering behavior.
- Added stop-timeout controls for teardown flows via
  `helm stop --timeout <SECONDS>` and `helm down --timeout <SECONDS>`.
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

- Added `helm share` tunnel management with `start`, `status`, and `stop`
  subcommands for app services, including provider-backed session tracking,
  persisted runtime metadata, and JSON/text output modes.
- Added Expose client sharing support to `helm share` via
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
- Fixed `helm artisan test` runtime isolation so test startup now derives
  injected env values after test-port remapping and keeps app targets on
  localhost TLS, preventing test containers from corrupting active dev
  runtime networking (for example Redis host/port reachability).
- Fixed `helm artisan test` runtime cleanup to force-remove prior test
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
- Added host-level port occupancy checks to `helm doctor` so startup conflicts
  are detected before container launch.
- Added Laravel runtime bootstrap to `helm start` for app targets:
  `storage:link`, `migrate`, and conditional `key:generate` when `APP_KEY`
  is missing.

### Changed

- Changed startup behavior to prioritize first-run app readiness as part of
  the default `helm start` flow.
- Changed zero-config persistence defaults so common stateful services survive
  routine stop/down/recreate cycles without manual volume setup.
- Changed Linux compatibility for host-service access by making loopback host
  alias wiring automatic in Docker run paths.

### Fixed

- Fixed `helm start --env <name>` key bootstrap detection to inspect the
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

- Fixed `helm artisan test` runtime startup to assign random host ports by
  default (matching `helm up`) and recreate remapped services automatically,
  preventing host-port collision failures.

## [1.7.0] - 2026-02-13

### Fixed

- Fixed swarm dependency `inject_env` `:port`/`:url` resolution to use live
  Docker host-port bindings when available, with fallback to configured ports.
- Fixed `helm open` database status URLs to report runtime published ports
  instead of static config ports.
- Fixed Caddy reverse-proxy forwarding for app routes to always send HTTPS
  `X-Forwarded-*` headers, preventing login/form redirects from downgrading
  to `http://`.
- Fixed serve container env precedence so inferred HTTPS `APP_URL` and
  `ASSET_URL` cannot be downgraded by explicit `service.env` `http://`
  overrides.
- Fixed `helm up` app env merge precedence so swarm/project dependency
  injected values cannot downgrade inferred HTTPS `APP_URL`/`ASSET_URL` to
  `http://`.

## [1.6.0] - 2026-02-13

### Added

- Added service lifecycle hooks in `.helm.toml` via `[[service.hook]]` with
  `post_up`, `pre_down`, and `post_down` phases.
- Added hook run modes for container `exec` commands and host `script`
  commands with optional `on_error` behavior (`fail` or `warn`).
- Added lifecycle integration so configured hooks run during `helm up`,
  `helm apply`, and `helm down` for selected targets.
- Added usage and init-template examples for hook configuration.

### Fixed

- Fixed `helm recreate --publish-all` to support `--parallel > 1`.
- Fixed `helm swarm recreate` to work with default random-port publishing
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

- Added formal Helm ownership labels to created containers:
  `com.helm.managed`, `com.helm.service`, `com.helm.kind`, and
  `com.helm.container`.
- Added `helm relabel` command to migrate existing containers by recreating
  selected services and applying Helm ownership labels.
- Added label-aware ownership checks before scoped prune removes containers.
- Added command polish flags for Docker parity:
  `helm cp` now supports `-L/--follow-link` and `-a/--archive`.
- Added command polish flags for Docker parity:
  `helm inspect` now supports `--size` and `--type`.
- Added command polish flags for Docker parity:
  `helm attach` now supports `--detach-keys`.
- Added structured JSON output modes:
  `helm inspect --json`, `helm port --json`, and `helm events --json`.
- Added `helm events --allow-empty` to explicitly allow empty service
  selections without failing.

### Changed

- Changed default `helm events` scoping from name filters to label-based Helm
  ownership filters.
- Changed `helm events` behavior to return non-zero when no services match,
  unless `--allow-empty` is supplied.
- Changed `helm events --all` behavior to print an explicit warning before
  streaming global daemon events.
- Changed default `helm prune` behavior to enforce Helm-scoped cleanup
  semantics with ownership validation.
- Changed `helm prune --all` behavior to require an explicit `--force` guard
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
- Added `service:/path` shorthand resolution for `helm cp` endpoints.
- Added Docker-compatible flags:
  `helm cp` now supports `-L/--follow-link` and `-a/--archive`.
- Added Docker-compatible flags:
  `helm inspect` now supports `--size` and `--type`.
- Added Docker-compatible flags:
  `helm attach` now supports `--detach-keys`.
- Added scoped selectors for event/prune workflows:
  `helm events` now supports `--service` and `--kind`.
- Added scoped selectors for event/prune workflows:
  `helm prune` now supports `--service`, `--kind`, and `--parallel`.

### Changed

- Changed restore file-progress output from an in-place status bar to
  persistent incremental log events with percent and MiB totals.
- Changed `helm events` default behavior to Helm container scope by applying
  container filters from resolved service names.
- Changed `helm events` UX to require explicit `--all` for global daemon
  event streaming.
- Changed `helm prune` default behavior to remove only stopped
  Helm-configured service containers (safe by default).
- Changed `helm prune` UX to require explicit `--all` for global
  `docker container prune` behavior.

## [1.0.0] - 2026-02-12

### Added

- Stable `1.0.0` release line for Helm.
- `INSTALLATION.md` with explicit install and verification steps.
- Release changelog baseline.

### Changed

- Refined release README messaging around zero-BS, low-config Laravel
  orchestration.
- Updated `helm init` defaults to a smaller preset-driven config.
- Fixed generated `helm init` template output so it writes valid TOML.
- Updated package metadata and versioning to `1.0.0`.
