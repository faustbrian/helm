# V8 Command Reference

Stackctl v8 exposes only the singleton-daemon and strict-YAML command surface.
There is no compatibility fallback for pre-v8 commands or configuration.

## Global options

- `--config <PATH>` selects an exact `.stackctl.yaml` file.
- `--project-root <DIR>` starts project discovery from a directory.
- `-q, --quiet` suppresses non-essential success output.
- `--no-color` disables color.
- `--dry-run` is accepted only where the selected operation supports it.
- `--non-interactive` prevents browser opening and interactive behavior.

Engine selection and Docker tuning are installation state owned by the daemon,
not per-invocation project flags.

## Configuration and artifacts

```text
stackctl config schema
stackctl config validate [PATH]
stackctl lock images
stackctl lock verify
stackctl lock diff
```

Configuration is strict YAML with `schema_version: 8`. Unknown fields,
duplicate keys, invalid identities, collisions, unsupported tags, and multiple
documents fail before mutation. Artifact locks are project-local YAML and bind
configured sources to immutable sha256 digests. During discovery, the daemon
creates a missing artifact lock automatically before it permits project Engine
mutation. It never replaces an existing lock implicitly; use `lock diff` to
inspect drift and `lock images` for a deliberate refresh. The explicit command
uses the authoritative daemon when available and the default Engine socket only
when no daemon endpoint exists.

## Daemon and installation

```text
stackctl setup --dir <DIR>... [--interval <SECONDS>]
stackctl daemon watch --dir <DIR>... [--once] [--interval <SECONDS>]
stackctl daemon service install --dir <DIR>... [--interval <SECONDS>]
stackctl daemon service status
stackctl daemon service restart
stackctl daemon service print --dir <DIR>... [--interval <SECONDS>]
stackctl daemon service uninstall --keep-data
stackctl daemon service uninstall --delete-data --confirm-delete-data
stackctl daemon status
stackctl daemon reconcile
stackctl daemon benchmark
stackctl daemon trust install|status|remove|rotate
```

`setup` is the normal one-time path. It preflights canonical watched roots and
`.localhost` resolution, then installs singleton CA trust and the login service
as one rollback-aware transaction. The daemon resolves missing locks and setup
waits for complete operational convergence.
`daemon service restart` preserves the installed definition, restarts through
the selected service manager, and succeeds only after full operational
readiness. The other nested daemon commands remain explicit administrative and
diagnostic operations.

`daemon adopt`, `backup`, `backups`, `restore`, `prune`, and `migration`
provide the explicit retained-data and reversible-migration workflows. Their
nested `--help` output is the authoritative argument reference.

## Project inspection

```text
stackctl status [--format table|json]
stackctl url [--service <NAME>] [--format table|json]
stackctl open [--service <NAME>|--all] [--no-browser] [--json]
stackctl logs [--service <NAME>...] [--all] [--follow] [--tail <N>] [--prefix]
stackctl env generate --output <PATH>
stackctl run <WORKFLOW>
```

Status, routes, logs, and managed environment values come from authoritative
daemon state. Selectors use exact service names; v8 has no kind, driver,
profile, random-domain, or host-Caddy selectors.

## Project commands

```text
stackctl exec [--service <NAME>] <COMMAND>...
stackctl artisan [--service <NAME>] [--browser] [ARGS]...
stackctl composer [--service <NAME>] [ARGS]...
stackctl node [--service <NAME>] [--package-manager npm|pnpm|yarn] [ARGS]...
stackctl bun [--service <NAME>] [ARGS]...
stackctl deno [--service <NAME>] [ARGS]...
stackctl phpstan|ecs|php-cs-fixer|psalm|pint|pest|phpunit|rector
```

These commands execute through typed daemon IPC inside the project's Linux
runtime container. They do not run repository scripts or language runtimes on
the host. Runtime versions belong in declarative project configuration rather
than command-line version-manager flags.

## Named workflows

Every workflow explicitly declares its trigger semantics. `mode: manual` is
the default and runs only through `stackctl run <WORKFLOW>`. `mode: automatic`
runs only through the daemon after complete Engine convergence.
It runs once per exact workflow and input revision. The CLI rejects explicit
invocation of an automatic workflow. Automatic workflows cannot contain an
interactive `open` step. In both modes, steps run in order and stop at the
first failure.

The initial workflow surface supports MySQL and MariaDB dump restores from a
project-local `.sql` file or one exact entry in a project-local `.zip`, an
optional database reset, an optional Laravel migration against an exact
connection, and opening an exact route. Example:

```text
stackctl run sandbox
```

The daemon streams the dump into the selected owned logical database. It does
not require `mysql`, `mariadb`, `unzip`, PHP, or Laravel on the host. Automatic
execution uses a durable content-addressed operation identity, so unchanged
workflows do not rerun after rescans, daemon restarts, login, or reboot.
