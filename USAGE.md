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
configured sources to immutable sha256 digests.

## Daemon and installation

```text
stackctl daemon watch --dir <DIR>... [--once] [--interval <SECONDS>]
stackctl daemon service install --dir <DIR>... [--interval <SECONDS>]
stackctl daemon service status
stackctl daemon service print --dir <DIR>... [--interval <SECONDS>]
stackctl daemon service uninstall --keep-data
stackctl daemon service uninstall --delete-data --confirm-delete-data
stackctl daemon status
stackctl daemon reconcile
stackctl daemon benchmark
stackctl daemon trust install|status|remove|rotate
```

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
