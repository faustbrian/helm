# Usage

This document describes every command and flag exposed by `helm`.

## CLI Shape

```bash
helm [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS]
```

## Global Options

These apply to all commands.

- `-q, --quiet`
  - Reduce output/noise.
- `--no-color`
  - Disable colored output.
- `--dry-run`
  - Print planned actions without changing containers/config where supported.
- `--config <PATH>`
  - Use an explicit `.helm.toml` path.
  - Conflicts with `--project-root`.
- `--project-root <DIR>`
  - Resolve `.helm.toml` from a specific directory.
- `--env <NAME>`
  - Runtime namespace (for example `testing` / `test`).
- `--repro`
  - Enable reproducibility mode (lockfile + deterministic checks).

## Common Selectors

Many commands support these selectors:

- `--service <NAME>`: one named service.
- `--kind <KIND>`: filter by service kind.
- `--driver <DRIVER>`: filter by backend driver.

If `--service` is omitted, commands operate on all matching services.

## Value Enums

### `KIND`

- `database`
- `cache`
- `object-store`
- `search`
- `app`

### `DRIVER`

- `postgres`
- `mysql`
- `redis`
- `valkey`
- `minio`
- `rustfs`
- `meilisearch`
- `typesense`
- `frankenphp`
- `gotenberg`
- `mailhog`

### Pull Policy (`--pull`)

- `always`
- `missing` (default)
- `never`

### Port Strategy (`--port-strategy`)

- `random` (default)
- `stable` (uses `--port-seed` if provided)

### Node Package Manager (`helm node --manager`)

- `bun` (default)
- `npm`
- `pnpm`
- `yarn`

## Top-Level Commands

### `helm init`

Initialize a new `.helm.toml` in the current directory.

### `helm config [--format <toml|json>] [migrate]`

- Without subcommand: print resolved config.
- `--format <FORMAT>`: output format (`toml` default, `json` supported).
- `helm config migrate`: migrate local config schema to latest supported version.

### `helm preset <SUBCOMMAND>`

- `helm preset list`: list available preset names.
- `helm preset show <NAME> [--format <toml|json>]`: show resolved defaults.

### `helm profile <SUBCOMMAND>`

- `helm profile list`: list built-in profile names.
- `helm profile show <NAME> [--format <FORMAT>]`: show services in profile.
  - `json`: structured JSON
  - `markdown`: markdown table
  - other values/default (`table`): plain tab-separated output

Built-in profiles include: `full`, `all`, `infra`, `data`, `app`, `web`, `api`.

### `helm doctor [--fix] [--repro]`

Validate local setup and configuration health.

- `--fix`: attempt automatic fixes where possible.
- `--repro`: run reproducibility checks.

### `helm lock <SUBCOMMAND>`

- `helm lock images`: resolve configured images to immutable digests.
- `helm lock verify`: verify lockfile exists and is in sync.
- `helm lock diff`: preview lockfile changes.

### `helm setup`

Prepare services before startup.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `1`)

### `helm up`

Start service containers.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait` (enabled by default behavior)
- `--wait-timeout <SECONDS>` (default: `30`)
- `--pull <always|missing|never>` (default: `missing`)
- `--force-recreate`
- `-P, --publish-all` (enabled by default behavior)
- `--port-strategy <random|stable>` (default: `random`)
- `--port-seed <SEED>`
- `--save-ports` (requires `--publish-all`)
- `--env-output`
- `--no-deps`
- `--seed`
- `--parallel <N>` (default: `1`)

### `helm apply`

Converge services and apply configured seed files.

Flags:

- `--no-deps`

### `helm update`

Pull and restart selected services.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--force-recreate`
- `--no-build`
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)

### `helm down`

Stop and remove services.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--no-deps`
- `-f, --force` (conflicts with `--no-deps`)
- `--parallel <N>` (default: `1`)

### `helm stop`

Stop services without removing containers.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--parallel <N>` (default: `1`)

### `helm rm`

Remove service containers.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `-f, --force`
- `--parallel <N>` (default: `1`)

### `helm recreate`

Destroy and recreate service containers.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)
- `-P, --publish-all`
- `--save-ports` (requires `--publish-all`)
- `--env-output`
- `--parallel <N>` (default: `1`)

### `helm restart`

Restart service containers.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `1`)

### `helm url`

Print service connection URLs.

Flags:

- `--service <NAME>`
- `--format <FORMAT>` (default: `table`)
  - `json`: structured JSON
  - other values/default (`table`): plain text URL output
- `--kind <KIND>`
- `--driver <DRIVER>`

### `helm restore`

Restore SQL data into a database service.

Flags:

- `--service <NAME>`
- `--file <PATH>`
- `--reset`
- `--migrate`
- `--schema-dump`
- `--gzip`

### `helm dump`

Dump a database service to SQL.

Flags:

- `--service <NAME>`
- `--file <PATH>`
- `--stdout`
- `--gzip`

### `helm ps`

Show runtime status for services.

Flags:

- `--format <FORMAT>` (default: `table`)
  - `json`: structured JSON
  - other values/default (`table`): human status view
- `--kind <KIND>`
- `--driver <DRIVER>`

### `helm about`

Show runtime project overview.

### `helm health`

Run health checks against selected services.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--timeout <SECONDS>` (default: `30`)
- `--interval <SECONDS>` (default: `2`)
- `--retries <N>`
- `--parallel <N>` (default: `1`)

### `helm env [generate]`

Manage `.env` values based on resolved/running services.

Main command flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--env-file <PATH>`
- `--sync`
- `--purge`
- `--persist-runtime` (requires `--sync`)
- `--create-missing`

Subcommands:

- `helm env generate --output <PATH>`
  - Generate a full env file from managed Helm app variables.

### `helm logs`

Show container logs.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--all`
- `--prefix`
- `-f, --follow`
- `--tail <N>`
- `-t, --timestamps`
- `--access` (tail local Caddy access logs instead)

### `helm pull`

Pull service images.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--parallel <N>` (default: `1`)

### `helm exec [-- <COMMAND...>]`

Run a command inside a service container.

Flags:

- `--service <NAME>`
- `--tty`
- `--no-tty`
- Trailing command is optional after flags.
- If no command is provided, Helm opens an interactive shell session.

### `helm app-create`

Bootstrap Laravel runtime tasks.

Flags:

- `--target <NAME>`
- `--no-migrate`
- `--seed`
- `--no-storage-link`

### `helm artisan -- <COMMAND...>`

Run `php artisan` inside the app container.

Flags:

- `--target <NAME>`
- `--tty`
- `--no-tty`
- Trailing Artisan command/args.

### `helm composer -- <COMMAND...>`

Run `composer` inside the app container.

Flags:

- `--target <NAME>`
- `--tty`
- `--no-tty`
- Trailing Composer command/args.

### `helm node -- <COMMAND...>`

Run JS package manager commands inside app container.

Flags:

- `--target <NAME>`
- `--manager <bun|npm|pnpm|yarn>` (default: `bun`)
- `--tty`
- `--no-tty`
- Trailing package-manager command/args.

### `helm ls`

List configured services.

Flags:

- `--format <FORMAT>` (default: `table`)
  - `json`: JSON array of service names
  - other values/default (`table`): one service name per line
- `--kind <KIND>`
- `--driver <DRIVER>`

### `helm swarm -- <COMMAND...>`

Run a Helm command across workspace swarm targets.

Flags:

- `--only <name1,name2,...>`
- `--no-deps`
- `-f, --force` (conflicts with `--no-deps`)
- `--parallel <N>` (default: `1`)
- `--fail-fast` (requires parallel semantics compatible with fail-fast)
- `--port-strategy <random|stable>` (default: `random`)
- `--port-seed <SEED>`
- `--env-output`
- Trailing command is required (examples: `up`, `down`, `ps --format json`).

### `helm completions <SHELL>`

Generate shell completion scripts.

`<SHELL>` is one of clap-complete supported targets (for example `bash`,
`zsh`, `fish`, `powershell`, `elvish`).

### `helm serve`

Start and expose an app target through local HTTPS routing.

Flags:

- `--target <NAME>`
- `--recreate`
- `--detached`
- `--env-output`
- `--trust-container-ca`

### `helm open`

Print or open serve URLs and health summary.

Flags:

- `--target <NAME>`
- `--all` (conflicts with `--target`)
- `--health-path <PATH>`
- `--no-browser`
- `--json`

### `helm env-scrub`

Scrub sensitive `.env` values and replace with local-safe placeholders.

Flags:

- `--env-file <PATH>`
