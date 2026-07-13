# V8 Configuration and Identity

## Canonical project file

V8 reads only `.stackctl.yaml` during normal operation. User configuration is
YAML, daemon state is SQLite, and machine-facing artifacts use JSON or typed
wire formats. TOML is limited to the isolated v7 migration reader.

Services use a Compose-familiar mapping keyed by identity:

```yaml
schema_version: 8
project: bill

services:
  app:
    preset: laravel
    image: ghcr.io/stackctl/php:8.4
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

## V7 migration

`stackctl config migrate --to yaml` reads v7 TOML in an isolated module, emits a
candidate YAML file, compares resolved v7 meaning, and reports semantic
differences. It never deletes or overwrites the source without approval. Normal
v8 discovery rejects TOML with the exact migration command and never edits a
repository automatically.
