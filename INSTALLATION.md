# Installation

Stackctl v8 supports macOS and Linux hosts with Docker Desktop or Docker
Engine. It is a clean install: pre-v8 state and project files are not migrated
or adopted.

## Build from source

```bash
git clone https://github.com/faustbrian/stackctl.git
cd stackctl
cargo install --path . --locked
stackctl --version
```

Release binaries and their SBOM, provenance, signatures, and architecture
evidence remain release-blocking artifacts until published in the v8 evidence
record.

Docker Desktop on macOS or Docker Engine on Linux must be running. Stackctl
talks to that Engine directly; no Docker CLI, Caddy, nginx, PHP, database, or
Redis installation is required on the host.

## First working project

Create `.stackctl.yaml` in a project below the root you want Stackctl to watch:

```yaml
schema_version: 8
project: example

services:
  app:
    preset: laravel
  database:
    preset: mysql
    version: "8"
  cache:
    preset: valkey
    version: "8"
  mailpit:
    preset: mailpit
```

From that project, validate the complete desired state without changing the
host:

```bash
stackctl config validate
```

Then run the one-time control-plane setup and create the project's immutable
artifact lock:

```bash
stackctl setup --dir ~/Developer
stackctl daemon service status
stackctl daemon status
stackctl lock images
stackctl daemon reconcile
```

Wait for the project to converge, then inspect and open it:

```bash
stackctl status
stackctl url
stackctl logs --service app --tail 100
stackctl open
```

The main route is deterministic:
`https://example-app.stackctl.localhost`. Stackctl never invents a suffix to
repair a collision; duplicate project identities fail until the configuration
or directory name is made unique.

## Configure the control plane

Create strict `.stackctl.yaml` projects under one or more watched roots, then
run the one-time setup transaction:

```bash
stackctl setup --dir ~/Developer
stackctl daemon service status
stackctl daemon status
```

Setup validates and canonicalizes every watched root and verifies that
Stackctl's `.localhost` names resolve to loopback before changing host state.
It then installs CA trust and the login service. If service installation fails,
newly added trust is removed; pre-existing trust is preserved.

Discovery checks the watched root and directories at most two levels below it
for exact `.stackctl.yaml` files. Ordinary files do not consume the bounded
directory budget, and a discovered project is a traversal boundary, so caches
and generated contents inside projects are never crawled. Hidden child
directories such as `.worktrees` are excluded; pass one explicitly as a watched
root when it should participate.

The login service owns project discovery and reconciliation. Routine project
commands communicate with it over user-only local IPC; they do not invoke a
Docker or Podman CLI.

When something does not start, use `stackctl config validate`, `stackctl lock
verify`, `stackctl daemon status`, and `stackctl status` in that order. Those
commands distinguish configuration, artifact, Engine, and service-readiness
failures without relying on host services.

## Shell completions

```bash
stackctl completions zsh > ~/.zsh/completions/_stackctl
```

## Removal

Preserve all state and Engine resources:

```bash
stackctl daemon service uninstall --keep-data
```

Deletion is a separate confirmed workflow and succeeds only when the daemon
can prove exact ownership and required recovery evidence:

```bash
stackctl daemon service uninstall --delete-data --confirm-delete-data
```
