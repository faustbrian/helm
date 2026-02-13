# Changelog

All notable changes to this project are documented in this file.

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
