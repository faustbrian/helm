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
catalog_revision: 2026-07-13.1
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

Discovery reads the lock only beside `.stackctl.yaml`, with the same byte bound,
UTF-8 requirement, and symbolic-link prohibition as the project file. Projects
without mutable artifacts may omit it. Mutable or preset artifacts must be
resolved to immutable identities before their Engine resources can be planned.

`stackctl lock images` publishes `.stackctl.lock.yaml` atomically. Mutable
explicit images are resolved by the singleton daemon through its selected
Engine and the result is validated again before publication. Already immutable
explicit images do not require an Engine lookup. `stackctl lock verify` and
`stackctl lock diff` operate on strict YAML and never load pre-v8 config paths.
Preset-only generation uses a revisioned built-in image catalog. The lock
records that catalog revision, so changing a preset's registry source or
default version invalidates existing locks instead of silently changing the
runtime artifact. An unknown preset version fails explicitly. Horizon, queue
workers, queues, and schedulers inherit their application artifact and never
receive redundant lock entries.

`php_extensions` is available only on the `laravel`, `frankenphp`, and `reverb`
application presets, whose digest-pinned base image provides the pinned
`install-php-extensions` contract. The daemon derives a content-addressed image
from the locked base and the sorted extension set before starting the app.
Workers and schedulers use that exact built image. Images without one of these
presets cannot declare extensions implicitly; they must contain their
requirements already.

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
are deduplicated. Distinct paths producing the same project/service domain are
a conflict. Diagnostics report the domain, names, and all canonical paths;
Stackctl never creates a fallback.

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

## Project trust

Declarative configurations in trusted watched roots reconcile automatically.
Privilege expansion becomes `awaiting_approval`, including privileged mode,
host networking, Engine socket mounts, devices, Linux capabilities, unsafe
ports, binds outside the project, untrusted registries, host hooks, and
destructive migration. Repository hooks run inside the application container;
automatic discovery never executes them on the host.

## Clean-install configuration

V8 does not convert an earlier Stackctl configuration. A project enters the v8
registry only through a newly authored `.stackctl.yaml` that passes strict v8
validation. Discovery reports a nearby `.stackctl.toml` as unsupported and
does not read or mutate it.
