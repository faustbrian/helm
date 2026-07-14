# Installation

Stackctl v8 supports macOS and Linux hosts with Docker Desktop or Docker
Engine. It is a clean install: pre-v8 state and project files are not migrated
or adopted.

## Build from source

```bash
cargo install --path . --locked
stackctl --version
```

Release binaries and their SBOM, provenance, signatures, and architecture
evidence remain release-blocking artifacts until published in the v8 evidence
record.

## Configure the control plane

Create strict `.stackctl.yaml` projects under one or more watched roots, then
run:

```bash
stackctl daemon trust install
stackctl daemon service install --dir ~/Developer
stackctl daemon service status
stackctl daemon status
```

The login service owns project discovery and reconciliation. Routine project
commands communicate with it over user-only local IPC; they do not invoke a
Docker or Podman CLI.

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
