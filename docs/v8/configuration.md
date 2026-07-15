# V8 Configuration and Identity

## Canonical project file

V8 reads only `.stackctl.yaml`. User configuration is YAML, daemon state is
SQLite, and machine-facing artifacts use JSON or typed wire formats. TOML is
not parsed, converted, or used as a fallback.

Services use a Compose-familiar mapping keyed by identity:

```yaml
schema_version: 8
project: bill

services:
  app:
    preset: laravel
    version: "8.5"
    php_extensions:
      - intl
      - redis
    composer_image: composer@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    node_image: node@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
    depends_on:
      - db
      - cache

  db:
    preset: postgres
    version: "17"
    database: bill

  cache:
    preset: valkey
    version: "8"

  mailpit:
    preset: mailpit
```

The common schema describes desired capabilities and logical resources. It does
not expose container names or CLI arguments. Advanced container settings live
in validated typed extensions.

## Strict parsing

Parsing rejects duplicate keys, unknown fields, multiple documents,
unsupported tags, non-string versions, and ambiguous scalar coercion. Schema
version is validated before expansion. Diagnostics identify file and field
path. A versioned JSON Schema is published for editors. Map order does not
affect the plan; dependencies are topologically ordered with explicit cycle
diagnostics.

The v8 editor contract is bundled at
`schemas/stackctl-project-v8.schema.json` and identifies itself as
`https://stackctl.dev/schemas/project/v8.json`. It is available without a
project, daemon, or container engine through `stackctl config schema`.

`stackctl config validate [PATH]` parses and resolves the complete v8 desired
state without writing the project file or contacting the daemon or container
engine. Without a path it reads `.stackctl.yaml` in the current directory.

## Immutable artifact lock

Mutable image declarations and built-in preset identities are resolved through
the project-local `.stackctl.lock.yaml` before Engine planning. The lock is
strict YAML with its own schema version and maps exact service identities to
the source declaration and an immutable sha256 digest:

```yaml
schema_version: 1
catalog_revision: 2026-07-15.3
images:
  app:
    source: preset:laravel:8.5
    resolved: dunglas/frankenphp@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  db:
    source: preset:postgres:17
    resolved: postgres@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
```

The source field is an exact freshness check. Explicit images use their exact
configured value; presets use `preset:{name}` or
`preset:{name}:{version}`. A changed source, unknown service, mutable resolved
value, unsupported field, duplicate key, tag, or additional YAML document
fails the complete registry before mutation. Stackctl never repairs a stale
lock or guesses a replacement.

Built-in presets resolve through an explicitly revisioned catalog of versioned
vendor tags. The catalog never uses the unbounded `latest` alias. Mailpit
replaces the unsupported MailHog preset. Soketi remains available as a
dedicated routable project service with a pinned multi-architecture artifact.

Discovery reads the lock only beside `.stackctl.yaml`, with the same byte bound,
UTF-8 requirement, and symbolic-link prohibition as the project file. Projects
without mutable artifacts may omit it. Mutable or preset artifacts must be
resolved to immutable identities before their Engine resources can be planned.

`stackctl lock images` publishes `.stackctl.lock.yaml` atomically. Mutable
explicit images are resolved by the singleton daemon through its selected
Engine and the result is validated again before publication. Already immutable
explicit images do not require an Engine lookup. `stackctl lock verify` and
`stackctl lock diff` operate only on the exact strict-YAML configuration path.
Preset-only generation uses a revisioned built-in image catalog. The lock
records that catalog revision, so changing a preset's registry source or
default version invalidates existing locks instead of silently changing the
runtime artifact. An unknown preset version fails explicitly. Horizon, queue
workers, queues, and schedulers inherit their application artifact and never
receive redundant lock entries.

`php_extensions` is available only on the `laravel`, `frankenphp`, and `reverb`
application presets. Their built-in artifact is the public FrankenPHP image,
resolved to an immutable digest through the project lock. Extension names must
use the installer's lowercase package syntax. The daemon derives one locally
cached content-addressed image from the locked base and sorted extension set,
allows registry and package access only for that build, installs the declared
modules with the public image's `install-php-extensions` executable, and
verifies each module through PHP before starting the app. Projects with the
same base digest, platform, extensions, and tool images reuse that derived
runtime. Workers use the same built image, while schedulers execute inside the
exact application container.
Images without one of these presets cannot declare extensions implicitly; they
must contain their requirements already.

Application and worker source is mounted at and executed from `/workspace`.
Without an explicit `command`, Laravel and FrankenPHP presets serve
`/workspace/public` as plain HTTP on the daemon-owned internal port; Reverb
starts its Artisan server on that same private port. The shared gateway alone
owns host TLS and routing.

Application services may declare `composer_image`, `node_image`, and
`bun_image`. Each value must already be an exact registry reference of the form
`repository@sha256:<64 hex characters>`; Stackctl does not resolve mutable tool
tags or download an installer during unattended reconciliation. The daemon
derives one content-addressed Linux runtime from the locked application base,
the target platform, normalized PHP extensions, and all declared tool-image
digests. It makes every input available through the selected Engine before an
offline build, then starts the application from that derived image. Workers
inherit the resulting image. The singleton daemon invokes scheduler commands
inside that application container once per wall-clock minute, so a project does
not require an otherwise idle scheduler container.

Project system libraries are part of the immutable application base image.
Stackctl does not accept arbitrary package names and run a distribution package
manager during reconciliation, because that would make results depend on
mutable repositories. A project requiring additional libraries must publish a
digest-pinned custom application base containing them; Composer, Node, or Bun
may then be layered from their separately pinned images as above.

## Project identity and routes

The project name is explicit `project` when present; otherwise it is the exact
directory basename. Project and service names must already be valid lowercase
DNS labels:

```text
[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?
```

Stackctl never lowercases, slugifies, trims, truncates, hashes, counts, or
otherwise repairs a name. Invalid input fails before mutation.

Routes always use `{project}-{service}.stackctl.localhost`. The combined first
label must not exceed 63 bytes. The app has no exception:

```text
bill-app.stackctl.localhost
bill-mailpit.stackctl.localhost
bill-1-app.stackctl.localhost
```

## Registry collision validation

The complete registry is validated before mutation. Canonical duplicate paths
are deduplicated. One project identity cannot belong to multiple distinct
canonical paths, including when the projects contain only non-routable
services. That conflict reports every path and requires a unique directory or
explicit `project` value. Distinct paths producing the same project/service
domain are also a conflict. Route diagnostics report the domain, names, and all
canonical paths; Stackctl never creates a fallback.

If persisted state proves an existing owner, its persistent resources remain
untouched and the newcomer does not activate. If ownership cannot be proven,
neither plan activates. The conflict remains visible until the user changes a
directory or explicit project name.

## Managed environment

Stackctl does not rewrite arbitrary user `.env` values. The daemon owns a
versioned environment record and injects it into application containers. An
optional user-only export supports IDEs and inspection.

Managed values include internal endpoints, database/schema and role, cache ACL
identity and prefix, object-store bucket and identity, and resolved route URLs.
Credentials remain stable until deliberate rotation and never appear in
arguments, logs, labels, routes, image layers, or diagnostics.

### Multiple databases and generated environment mapping

Each database service receives a distinct logical schema and credential inside
the shared compatible database instance. When one application needs more than
one database, use `environment_mapping` on the additional service so its
generated keys do not collide with the primary `DB_*` values:

```yaml
services:
  app:
    preset: laravel

  shipit:
    preset: mysql
    version: "8"

  billing:
    preset: mysql
    version: "8"
    environment_mapping:
      DB_HOST: DB_INVOICING_HOST
      DB_PORT: DB_INVOICING_PORT
      DB_DATABASE: DB_INVOICING_DATABASE
      DB_USERNAME: DB_INVOICING_USERNAME
      DB_PASSWORD: DB_INVOICING_PASSWORD
```

Mapping is explicit and strict. Stackctl does not rename a colliding key by
guessing; invalid keys, duplicate targets, and unresolved collisions fail the
project plan.

## Explicit project workflows

Named workflows capture repeatable, user-invoked development operations in
the same strict YAML without turning them into unattended daemon hooks. For an
API project that restores two database dumps, migrates only the primary
Laravel connection, and then opens the app:

```yaml
workflows:
  sandbox:
    steps:
      - type: database_restore
        service: shipit
        file: database/dumps/sandbox.sql.zip
        archive_entry: sandbox.sql
        reset: true
        migrate:
          service: app
          connection: mysql

      - type: database_restore
        service: billing
        file: database/dumps/billing_staging.sql
        reset: true

      - type: open
        service: app
```

Run it from the project with:

```bash
stackctl config validate
stackctl run sandbox
```

Steps are sequential and stop on failure. Dump paths must be traversal-free,
project-relative regular files. `archive_entry` selects one exact non-empty
file from a ZIP without requiring host `unzip`. `reset: true` drops and
re-provisions only that project's selected logical schema before streaming the
dump. A `migrate` block runs `artisan migrate --database=<connection>` inside
the selected Linux application service only after the restore succeeds.

Workflows are never executed during discovery, login, reboot, reconciliation,
or file watching. Invoking `stackctl run` is the destructive authorization
boundary.

## Project trust

Declarative configurations in trusted watched roots reconcile automatically.
The current strict schema does not expose privileged mode, host networking,
Engine socket mounts, devices, Linux capabilities, unsafe ports, binds outside
the project, untrusted registries, host hooks, or destructive migration; those
fields fail as unknown before planning. Any future schema that introduces a
privilege expansion must represent it as `awaiting_approval` before mutation.
Repository hooks run inside the application container; automatic discovery
never executes them on the host. Both daemon discovery and thin-CLI project
resolution reject symbolic-link configuration files.

## Configuration boundary

A project enters the registry only through an exact `.stackctl.yaml` file that
passes strict v8 validation. Discovery and CLI lookup ignore every other file;
there is no conversion, compatibility lookup, or alternate configuration path.
