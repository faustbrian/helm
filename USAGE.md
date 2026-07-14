# Usage

This document describes every command and flag exposed by `stackctl`.

## CLI Shape

```bash
stackctl [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS]
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
  - Use an explicit `.stackctl.toml` or `.stackctl.yaml` path.
  - Conflicts with `--project-root`.
- `--project-root <DIR>`
  - Resolve `.stackctl.toml` or `.stackctl.yaml` from a specific directory.
- `--env <NAME>`
  - Runtime namespace (for example `testing` / `test`).
- `--engine <docker|podman>`
  - Override container runtime engine for this invocation.
  - When omitted, Stackctl uses `container_engine` from config, then defaults to
    `docker`.
- `--repro`
  - Enable reproducibility mode (lockfile + deterministic checks).
- `--non-interactive`
  - Disable interactive behavior (for example: browser auto-open and TTY usage).

## Runtime Engine

Set a project default engine in `.stackctl.toml` or `.stackctl.yaml`:

```toml
container_engine = "docker" # or "podman"
```

Notes:

- Stackctl defaults to `docker` when `container_engine` is not set.
- Podman support covers core Docker-compatible CLI flows.
- Some advanced runtime/network behavior can differ by engine and host setup.

## Domain Strategy

Set a project default app-domain strategy in `.stackctl.toml` or
`.stackctl.yaml`:

```toml
domain_strategy = "directory" # or "random"
```

Rules:

- `directory` uses the kebab-case project directory name as the base label.
- `random` uses a stable project-specific `stackctl-<hash>` base label.
- Stackctl only auto-generates domains for app services that do not already define
  `domain` or `domains`.
- The generated app service named `app` uses `<base>.stackctl`.
- Other app services use `<base>-<service>.stackctl`.

Example for `/Users/brian/Developer/my-project`:

```toml
domain_strategy = "directory"

[[service]]
preset = "laravel"

[[service]]
preset = "gotenberg"

[[service]]
preset = "mailhog"
```

This resolves to:

- `my-project.stackctl`
- `my-project-gotenberg.stackctl`
- `my-project-mailhog.stackctl`

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
- `mongodb`
- `mysql`
- `sqlserver`
- `redis`
- `valkey`
- `dragonfly`
- `memcached`
- `minio`
- `garage`
- `localstack`
- `rustfs`
- `meilisearch`
- `typesense`
- `frankenphp`
- `reverb`
- `horizon`
- `scheduler`
- `dusk`
- `gotenberg`
- `mailhog`
- `rabbitmq`
- `soketi`

### Pull Policy (`--pull`)

- `always`
- `missing` (default)
- `never`

### Restart Policy (`restart`)

- `no`
- `on-failure`
- `always`
- `unless-stopped` (default when `restart` is omitted)

### Port Strategy (`--port-strategy`)

- `random` (default)
- `stable` (uses `--port-seed` if provided)

### Node Package Manager (`stackctl node --package-manager`)

- `npm`
- `pnpm`
- `yarn`

### Node Version Manager (`stackctl node --version-manager`)

- `system` (default)
- `fnm`
- `nvm`
- `volta`

### JavaScript Runtime (`[service.javascript].runtime`)

- `node` (default)
- `bun`
- `deno`

### Container Engine (`--engine`)

- `docker` (default)
- `podman`

## Top-Level Commands

### `stackctl init`

Initialize a new `.stackctl.toml` in the current directory.

- New configs default `domain_strategy` to `directory`.
- The generated template omits explicit app `domain` entries and relies on the
  configured strategy instead.

### `stackctl config [--format <toml|json>] [migrate]`

- Without subcommand: print resolved config.
- `--format <FORMAT>`: output format (`toml` default, `json` supported).

### `stackctl daemon <start|watch|status|stop|logs>`

Manage a per-project Stackctl daemon target by explicit path instead of the
current working directory.

Flags:

- `--path <DIR>` (required)

Notes:

- `--path` may point at the Stackctl project root or any nested directory inside
  that project.
- Daemon commands resolve `.stackctl.toml` from the explicit path before regular
  config loading, so they do not depend on the caller's current directory.
- `daemon start` persists per-project session metadata and a log path under
  `~/.config/stackctl/daemon/`.
- `daemon watch --dir <DIR>` scans one or more parent directories for
  `.stackctl.toml` projects and starts missing per-project daemons.
- `daemon watch --exclude-dir <DIR>` skips specific subtrees inside watched
  roots.
- `daemon watch --max-projects <N>` caps how many discovered projects Stackctl
  will auto-start from one watch pass.
- `daemon watch --once` runs one discovery pass and exits.
- `daemon watch --interval <SECONDS>` controls the repeat scan delay when
  `--once` is not set.
- `daemon status`, `stop`, and `logs` read that persisted session state instead
  of inferring daemon ownership from the current shell process.
- The daemon child bootstraps the project through Stackctl's existing `start`
  flow, then polls managed services and reruns `up` if any service container is
  missing or no longer `running`.
- Repeated recovery failures use exponential backoff before retrying.
- Watch mode deduplicates overlapping watch roots, ignores nested child
  projects under an already managed project root, and reports invalid
  `.stackctl.toml` files without stopping discovery for valid sibling projects.
- When `--max-projects` is set, extra discovered projects are skipped instead
  of being auto-started, which keeps broad watch roots from starting an
  unbounded number of daemons.

### `stackctl daemon service <install|status|print|uninstall>`

Manage a login-time watch service backed by the local user service manager.

Flags:

- `install --dir <DIR>` (repeatable, required)
- `install --exclude-dir <DIR>` (repeatable)
- `install --max-projects <N>`
- `install --interval <SECONDS>` (default: `30`)
- `print --dir <DIR>` (repeatable, required)
- `print --exclude-dir <DIR>` (repeatable)
- `print --max-projects <N>`
- `print --interval <SECONDS>` (default: `30`)

Notes:

- On macOS Stackctl installs a `launchd` user agent under
  `~/Library/LaunchAgents/`.
- On Linux Stackctl installs a `systemd --user` unit under
  `~/.config/systemd/user/`.
- `install` writes the rendered service definition, enables it for the current
  user, and starts it immediately.
- Installed services preserve the same watch policy flags as interactive
  `stackctl daemon watch`, including exclusions and max project limits.
- `status` reports whether the service definition is currently installed.
- `print` shows the rendered unit/plist without writing it.
- `uninstall` stops the installed service and removes its definition file.

### `stackctl preset <SUBCOMMAND>`

- `stackctl preset list`: list available preset names.
- `stackctl preset show <NAME> [--format <toml|json>]`: show resolved defaults.

### `stackctl profile <SUBCOMMAND>`

- `stackctl profile list`: list built-in profile names.
- `stackctl profile show <NAME> [--format <FORMAT>]`: show services in profile.
  - `json`: structured JSON
  - `markdown`: markdown table
  - other values/default (`table`): plain tab-separated output

Built-in profiles include: `full`, `all`, `infra`, `data`, `app`, `web`, `api`.

### `stackctl doctor [--fix] [--repro] [--reachability]`

Validate local setup and configuration health.

- `--fix`: attempt automatic fixes where possible.
- `--repro`: run reproducibility checks.
- `--reachability`: probe app URLs and health endpoints.
- `--format <FORMAT>` (`table` default, `json` supported)

### `stackctl lock <SUBCOMMAND>`

- `stackctl lock images`: resolve configured images to immutable digests.
- `stackctl lock verify`: verify lockfile exists and is in sync.
- `stackctl lock diff`: preview lockfile changes.

### `stackctl task deps bump`

Run opinionated dependency bump workflows for Composer and selected
JavaScript runtimes.

### `stackctl task deps audit`

Run dependency vulnerability audits for Composer and selected
JavaScript runtimes.

### `stackctl task deps normalize`

Normalize dependency manifests and lockfiles for Composer and selected
JavaScript runtimes.

### `stackctl task deps install`

Install dependencies for Composer and selected JavaScript runtimes.

### `stackctl setup`

Prepare services before startup.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl start`

Run doctor checks, start selected services, then open app URL summaries.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait`
- `--no-wait` (conflicts with `--wait`, default behavior)
- `--wait-timeout <SECONDS>` (default: `30`)
- `--pull <always|missing|never>` (default: `missing`)
- `--force-recreate`
- `--no-open`
- `--health-path <PATH>`
- `--no-deps`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl up`

Start service containers.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait` (enabled by default behavior)
- `--no-wait` (conflicts with `--wait`)
- `--wait-timeout <SECONDS>` (default: `30`)
- `--pull <always|missing|never>` (default: `missing`)
- `--force-recreate`
- `-P, --publish-all` (enabled by default behavior)
- `--no-publish-all` (conflicts with `--publish-all`)
- `--port-strategy <random|stable>` (default: `random`)
- `--port-seed <SEED>`
- `--save-ports` (requires `--publish-all`)
- `--env-output`
- `--no-deps`
- `--seed`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl apply`

Converge services and apply configured seed files.

Flags:

- `--no-deps`

### `stackctl update`

Pull and restart selected services.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--force-recreate`
- `--no-build`
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)

### `stackctl down`

Stop and remove services.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--no-deps`
- `-f, --force` (conflicts with `--no-deps`)
- `--timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

## Service Hooks

Define per-service lifecycle hooks in `.stackctl.toml` with `[[service.hook]]`.
Supported phases are `post_up`, `pre_down`, and `post_down`.

```toml
[[service]]
name = "app"
preset = "laravel"

[[service.hook]]
name = "seed-dev-user"
phase = "post_up"
on_error = "fail" # fail | warn

[service.hook.run]
type = "exec"
argv = ["php", "artisan", "db:seed", "--class=DevUserSeeder"]
```

`run.type = "exec"` runs inside the selected service container.
`run.type = "script"` runs a host script (relative paths are resolved from
the Stackctl project root).

## Service Restart Policy

Stackctl applies Docker restart policies to long-lived service containers.
When `restart` is omitted, Stackctl uses `unless-stopped` so services come
back after Docker restarts or laptop sleep without requiring a manual
morning `stackctl up`.

Override per service in `.stackctl.toml` when a container should opt out or
use a stricter Docker policy:

```toml
[[service]]
preset = "laravel"
restart = "unless-stopped"

[[service]]
name = "browser"
preset = "dusk"
restart = "no"
```

Stackctl passes the configured value through to Docker `run --restart ...`.

### `stackctl stop`

Stop services without removing containers.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl rm`

Remove service containers.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `-f, --force`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl recreate`

Destroy and recreate service containers.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait` (default behavior)
- `--no-wait` (conflicts with `--wait`)
- `--wait-timeout <SECONDS>` (default: `30`)
- `-P, --publish-all`
- `--save-ports` (requires `--publish-all`)
- `--env-output`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl restart`

Restart service containers.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl relabel`

Recreate containers to apply current Stackctl ownership labels.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--wait`
- `--wait-timeout <SECONDS>` (default: `30`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl url`

Print service connection URLs.

Flags:

- `--service <NAME>`
- `--format <FORMAT>` (default: `table`)
  - `json`: structured JSON
  - other values/default (`table`): plain text URL output
- `--kind <KIND>`
- `--driver <DRIVER>`

### `stackctl restore`

Restore SQL data into a database service.

Flags:

- `--service <NAME>`
- `--file <PATH>`
- `--reset`
- `--migrate`
- `--schema-dump`
- `--gzip`

### `stackctl dump`

Dump a database service to SQL.

Flags:

- `--service <NAME>`
- `--file <PATH>`
- `--stdout`
- `--gzip`

### `stackctl ps`

Show runtime status for services.

Flags:

- `--format <FORMAT>` (default: `table`)
  - `json`: structured JSON
  - other values/default (`table`): human status view
- `--kind <KIND>`
- `--driver <DRIVER>`

### `stackctl about`

Show runtime project overview.

Flags:

- `--format <FORMAT>` (`table` default, `json` supported)

### `stackctl health`

Run health checks against selected services.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--format <FORMAT>` (`table` default, `json` supported)
- `--timeout <SECONDS>` (default: `30`)
- `--interval <SECONDS>` (default: `2`)
- `--retries <N>`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl env [generate]`

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

- `stackctl env generate --output <PATH>`
  - Generate a full env file from managed Stackctl app variables.

### `stackctl logs`

Show container logs.

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service`, `--kind`, and `--all`)
- `--all` (conflicts with `--service`)
- `--prefix`
- `-f, --follow`
- `--tail <N>`
- `--since <VALUE>`
- `--until <VALUE>`
- `-t, --timestamps`
- `--access` (tail local Caddy access logs instead)

### `stackctl top [ARGS...]`

Show running processes in container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- Trailing `ARGS...` are passed to `<engine> top` (for example: `aux`).

### `stackctl stats`

Show a live stream of container resource usage.

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--no-stream` (single snapshot mode)
- `--format <FORMAT>` (passed to Docker stats format)

### `stackctl inspect`

Show low-level details for container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--format <FORMAT>`
- `--output <FORMAT>` (`table` default, `json` supported)
- `--json` (structured JSON array)
- `--size`
- `--type <OBJECT_TYPE>`

### `stackctl attach`

Attach local standard input/output/error streams to a running container.

Flags:

- `--service <NAME>`
- `--no-stdin`
- `--sig-proxy`
- `--detach-keys <KEYS>`

### `stackctl cp <SOURCE> <DESTINATION>`

Copy files/folders between host and container.

`SOURCE` and `DESTINATION` can be host paths, `service:/path`, or
`container:/path`.

Flags:

- `-L, --follow-link`
- `-a, --archive`

### `stackctl kill`

Force-stop running container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--signal <SIGNAL>`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl pause`

Pause all processes in container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl unpause`

Unpause all processes in container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl wait`

Block until container(s) stop and print exit status.

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--condition <CONDITION>`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl events`

Stream Docker daemon events (Stackctl container scope by default).

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--since <VALUE>`
- `--until <VALUE>`
- `--format <FORMAT>`
- `--json` (newline-delimited JSON objects)
- `--all` (disable Stackctl-only event scoping)
- `--allow-empty`
- `--filter <KEY=VALUE>` (repeatable)

### `stackctl port [PRIVATE_PORT]`

List port mappings for container(s).

Flags:

- `--service <NAME>`
  - Repeatable: `--service app --service worker`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--format <FORMAT>` (`table` default, `json` supported)
- `--json` (structured JSON array)
- Optional positional `PRIVATE_PORT` (for example `80/tcp`)

### `stackctl prune`

Remove stopped Stackctl service containers (or all with `--all`).

Flags:

- `--service <NAME>`
  - Repeatable: `--service db --service cache`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))
- `--all` (global Docker prune scope)
- `-f, --force` (required with `--all`)
- `--filter <KEY=VALUE>` (global mode only)

### `stackctl pull`

Pull service images.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--parallel <N>` (default: `auto` = min(4, CPU cores))

### `stackctl exec [-- <COMMAND...>]`

Run a command inside a service container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing command is optional after flags.
- If no command is provided, Stackctl opens an interactive shell session.

### `stackctl app-create`

Bootstrap Laravel runtime tasks.

Flags:

- `--service <NAME>`
- `--no-migrate`
- `--seed`
- `--no-storage-link`

### `stackctl artisan -- <COMMAND...>`

Run `php artisan` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Artisan command/args.

### `stackctl composer -- <COMMAND...>`

Run `composer` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Composer command/args.

### `stackctl phpstan -- <COMMAND...>`

Run `phpstan` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing PHPStan command/args.

### `stackctl ecs -- <COMMAND...>`

Run `ecs` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing ECS command/args.

### `stackctl php-cs-fixer -- <COMMAND...>`

Run `php-cs-fixer` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing PHP CS Fixer command/args.

### `stackctl psalm -- <COMMAND...>`

Run `psalm` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Psalm command/args.

### `stackctl pint -- <COMMAND...>`

Run `pint` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Pint command/args.

### `stackctl pest -- <COMMAND...>`

Run `pest` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Pest command/args.

### `stackctl phpunit -- <COMMAND...>`

Run `phpunit` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing PHPUnit command/args.

### `stackctl rector -- <COMMAND...>`

Run `rector` inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--tty`
- `--no-tty`
- Trailing Rector command/args.

### `stackctl node -- <COMMAND...>`

Run Node package manager commands inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--package-manager <npm|pnpm|yarn>` (optional override)
- `--version-manager <system|fnm|nvm|volta>` (optional override)
- `--node-version <VERSION>` (optional override)
- `--tty`
- `--no-tty`
- Trailing package-manager command/args.

Node package-manager resolution order:

- CLI overrides
- `[service.javascript]` in `.stackctl.toml`
- Project files:
  `.nvmrc`, `.node-version`, `package.json.packageManager`,
  `package.json.volta.node`, and `package.json.engines.node`
- Stackctl defaults (`system` version manager)

Config example:

```toml
[[service]]
preset = "laravel"
name = "app"

[service.javascript]
runtime = "node"
package_manager = "pnpm"
version_manager = "fnm"
version = "22"
```

### `stackctl deno -- <COMMAND...>`

Run Deno inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--deno-version <VERSION>` (optional override)
- `--tty`
- `--no-tty`
- Trailing Deno command/args.

Deno resolution order:

- CLI `--deno-version`
- `[service.javascript]` in `.stackctl.toml`
- Project files: `deno.json`, `deno.jsonc`, or `deno.lock`
- Stackctl default Deno installer version

Config example:

```toml
[[service]]
preset = "laravel"
name = "app"

[service.javascript]
runtime = "deno"
version = "2.2.3"
```

### `stackctl bun -- <COMMAND...>`

Run Bun inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--bun-version <VERSION>` (optional override)
- `--tty`
- `--no-tty`
- Trailing Bun command/args.

Bun resolution order:

- CLI `--bun-version`
- `[service.javascript]` in `.stackctl.toml`
- Project files: `bun.lock`, `bun.lockb`, or `package.json.packageManager`
- Stackctl default Bun installer version

Config example:

```toml
[[service]]
preset = "laravel"
name = "app"

[service.javascript]
runtime = "bun"
version = "1.2.5"
```

### `stackctl task deps <bump|audit|normalize|install>`

Run opinionated dependency workflows inside the app container.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--composer`
- `--node`
- `--bun`
- `--deno`
- `--all` (runs all workflows; conflicts with `--composer`, `--node`, `--bun`, and `--deno`)
- `--package-manager <npm|pnpm|yarn>` (optional override)
- `--version-manager <system|fnm|nvm|volta>` (optional override)
- `--node-version <VERSION>` (optional override)
- `--tty`
- `--no-tty`

Notes:

- At least one target flag is required: `--composer`, `--node`, `--bun`,
  `--deno`, or `--all`.
- `bump` runs:
  - Composer: `composer bump --dev-only`, `composer bump --no-dev-only`,
    `composer update --ignore-platform-reqs`, then `composer normalize`
  - Node: manager-specific upgrade flow against `package.json`
  - Bun: `bun update --latest`
  - Deno: `deno outdated --update --latest`
- `audit` runs:
  - Composer: `composer audit`
  - Node: manager-specific audit flow
  - Bun: `bun audit`
  - Deno: currently skipped with a warning because Stackctl does not define a
    Deno dependency-audit equivalent yet
- `normalize` runs:
  - Composer: `composer normalize`
  - Node: manager-specific lockfile normalization flow
  - Bun: `bun install`
  - Deno: currently skipped with a warning because Stackctl does not define a
    Deno dependency-normalize equivalent yet
- `install` runs:
  - Composer: `composer install`
  - Node: manager-specific install flow
  - Bun: `bun install`
  - Deno: currently skipped with a warning because Stackctl does not define a
    Deno dependency-install equivalent yet
- Node workflow targets infer the package manager from
  `package.json.packageManager` first, then lockfiles, when
  `--package-manager` is omitted.
- `--all` runs Composer, Node, Bun, and Deno workflows; missing manifests
  and unsupported Deno actions are skipped with a warning.
- Non-system Node version managers require a concrete Node version from
  `--node-version`, `[service.javascript].version`, or project files such as
  `.nvmrc` or `.node-version`.
- Missing `composer.json`, `package.json`, or Deno project files are
  skipped with a warning.

### `stackctl ls`

List configured services.

Flags:

- `--format <FORMAT>` (default: `table`)
  - `json`: JSON array of service names
  - other values/default (`table`): one service name per line
- `--kind <KIND>`
- `--driver <DRIVER>`

### `stackctl swarm -- <COMMAND...>`

Run a Stackctl command across workspace swarm targets.

Flags:

- `--only <name1,name2,...>`
- `--no-deps`
- `-f, --force` (conflicts with `--no-deps`)
- `--parallel <N>` (default: `auto` = min(4, CPU cores))
- `--fail-fast` (parallel-safe fail-fast; remaining targets are cancelled)
- `--port-strategy <random|stable>` (default: `random`)
- `--port-seed <SEED>`
- `--env-output`
- Trailing command is required (examples: `up`, `down`, `ps --format json`).

### `stackctl completions <SHELL>`

Generate shell completion scripts.

`<SHELL>` is one of clap-complete supported targets (for example `bash`,
`zsh`, `fish`, `powershell`, `elvish`).

### `stackctl serve`

Start and expose an app service through local HTTPS routing.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service` and `--kind`)
- `--recreate`
- `--detached`
- `--env-output`
- `--trust-container-ca`

### `stackctl open`

Print or open serve URLs and health summary.

Flags:

- `--service <NAME>`
- `--kind <KIND>`
- `--profile <NAME>` (conflicts with `--service`, `--kind`, and `--all`)
- `--all` (conflicts with `--service`)
- `--health-path <PATH>`
- `--no-browser`
- `--json`

### `stackctl share <SUBCOMMAND>`

Expose an app service through a supported tunnel provider.

- `stackctl share start (--provider <cloudflare|expose|tailscale> | --cloudflare | --expose | --tailscale) [--service <NAME>] [--detached] [--timeout <SECONDS>] [--json]`
- `stackctl share status [--service <NAME>] [--provider <cloudflare|expose|tailscale> | --cloudflare | --expose | --tailscale] [--json]`
- `stackctl share stop [--all] [--service <NAME>] [--provider <cloudflare|expose|tailscale> | --cloudflare | --expose | --tailscale] [--json]`

Notes:

- `share start` requires provider binaries on `PATH` (`cloudflared`, `expose`, or `tailscale`).
- `--detached` keeps the provider process running in background.
- Session state is persisted under `~/.config/stackctl/share/`.

### `stackctl env-scrub`

Scrub sensitive `.env` values and replace with local-safe placeholders.

Flags:

- `--env-file <PATH>`
