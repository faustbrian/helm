# V8 Architecture

## Current repository constraints

The v7 implementation establishes useful behavior but does not provide the v8
ownership model:

- `src/daemon/supervisor.rs` runs one recovery loop per project and calls CLI
  handlers rather than reconciling a global desired graph.
- `src/daemon/state.rs` persists PID and log metadata in per-project TOML files;
  it is not transactional control-plane state.
- `src/daemon/discovery.rs` recursively rescans directories but has no
  filesystem event source.
- `src/docker/cmd.rs` treats Docker or Podman CLI processes as the runtime API.
- `src/serve/caddy/process.rs` requires a host `caddy` executable.
- `src/serve/hosts/write.rs` appends one host entry per domain through a
  privileged shell.
- `src/serve/trust/container.rs` discovers and copies Caddy CA files from
  application containers.
- `src/config/domain_names.rs` special-cases the app domain and offers a random
  naming strategy.
- `src/config/raw.rs` and `src/config/types/config_root.rs` model services as a
  list and carry container-engine and domain-strategy concerns in project
  configuration.

These paths remain migration sources while v8 is introduced. They are not the
target architecture.

## Two planes

### Host control plane

One per-user Stackctl daemon owns watched roots, the canonical project registry,
strict YAML validation, transactional SQLite state, reconciliation, Engine API
communication, certificate lifecycle, gateway planning, managed credentials,
local IPC, and login/restart/sleep recovery.

Only Stackctl and narrowly scoped operating-system integration execute on the
host. PHP, Composer, JavaScript runtimes, web servers, workers, databases,
caches, and auxiliary services do not.

### Linux workload plane

The selected container engine owns one gateway, one application runtime per
project, project processes, compatibility-keyed shared infrastructure, and
dedicated or ephemeral services where logical sharing is unsafe. Application
containers expose plain HTTP on a private Stackctl network. Only the gateway
normally publishes loopback ports 80 and 443.

## Dependency direction

```text
CLI / GUI / MCP client
        |
        v
versioned IPC contract
        |
        v
daemon application services
        |
        +--> pure desired-state validation and planning
        +--> reconciliation coordinator
        |
        v
narrow effect capabilities
        |
        +--> Engine API adapter
        +--> SQLite state store
        +--> filesystem event source
        +--> gateway configuration adapter
        +--> certificate and OS integration adapters
```

Pure configuration, identity, compatibility, and planning code must not depend
on Docker request types, SQLite, clocks, filesystems, or process execution.

## State model

V8 keeps these representations distinct:

1. `RawProjectConfig`: strict deserialization of `.stackctl.yaml`.
2. `DesiredProject`: validated names, services, capabilities, and policies.
3. `ResolvedProjectPlan`: exact compatibility keys, logical resources,
   endpoints, routes, and reversible/destructive operations.
4. Backend mutation requests: typed inputs to narrow effect capabilities.
5. `ObservedState`: Engine objects, service readiness, route/certificate state,
   and persisted ownership.
6. `ReconciliationResult`: converged, changed, awaiting approval, conflicted,
   orphaned, degraded, or failed with structured reasons.

No common model exposes Docker argument arrays, CLI output, Caddyfile syntax,
or host package-manager concepts.

## Capability boundaries

The Engine integration is composed from `ContainerLifecycle`,
`ImageReferenceResolver`, `ImageResolver`,
`ImageBuilder`, `NetworkManager`, `VolumeManager`, `CommandExecutor`,
`LogSource`, `HealthObserver`, `EventSource`, and `ResourceMetrics`.

The gateway accepts a complete route and certificate plan and applies it
atomically. Project resource provisioning is distinct from service-instance
lifecycle: starting PostgreSQL and ensuring a project database/role are
separate reconciliation steps.

## Singleton daemon and IPC

The daemon is one process per OS user. It uses filesystem events with debounce,
periodic discovery rescans, Engine events with periodic observed-state scans,
per-resource mutation locks, bounded exponential backoff with jitter,
cancellation, and independent reconciliation for unrelated projects.

An unavailable engine is recoverable observed state. Desired state remains
authoritative and all resources reconcile when the selected engine returns.

Runtime operations use versioned local IPC. macOS and Linux use a user-only Unix
socket; Windows uses a user-only named pipe. Requests and responses carry
protocol version, request ID, typed payload/result, and structured diagnostics.
Logs and events support streaming and cancellation. Clients do not duplicate
daemon reconciliation. Offline schema output and validation may remain local.

## Transactional state

SQLite stores the installation ID, watched roots, projects, desired/resolved
revisions, ownership, compatibility fingerprints, credentials, logical
resources, routes, observed identifiers, reconciliation history, approvals,
orphans, retention, and migrations.

State transitions and schema upgrades are transactional. Upgrades create a
verified backup and either commit completely or retain the prior database. If
WAL is enabled, Stackctl pins a SQLite release containing applicable WAL race
fixes.

SQLite does not replace runtime observation. Engine labels reconstruct
ownership after state loss and are compared with persisted state.

## Resource ownership

Every managed Engine object has immutable labels for managed status,
installation ID, schema, resource kind, compatibility fingerprint, owning
project where applicable, desired revision, and retention class. Stackctl never
mutates or deletes an object whose ownership cannot be proven. Persistent names
are deterministic.

## Platform ownership

| Concern | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Login start | launchd user agent | systemd user unit | per-user startup task/service |
| Engine transport | Unix socket | Unix socket | named pipe |
| IPC | Unix socket | Unix socket | named pipe |
| File events | native watcher | native watcher | native watcher |
| Trust | Keychain | supported system/browser stores | Current User certificate store |
| Workloads | Linux engine VM | Linux containers | Linux engine VM/WSL2 |

Compilation is not platform evidence. Every claimed row requires a recorded
platform acceptance result before release.

The release status and evidence requirements for each concrete combination are
tracked in [Platform support](platform-support.md). The table above describes
ownership, not a support claim.
