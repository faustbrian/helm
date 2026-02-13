# Changelog

All notable changes to this project are documented in this file.

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
