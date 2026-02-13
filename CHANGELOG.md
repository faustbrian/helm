# Changelog

All notable changes to this project are documented in this file.

## [1.3.0] - 2026-02-13

### Added

- Added `helm events --json` for newline-delimited JSON event output via
  Docker's `{{json .}}` formatter.

### Changed

- Changed `helm events` option handling to reject incompatible usage of
  `--json` with `--format`.
- Changed `helm events --help` output to document JSON mode and formatter
  semantics.

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
