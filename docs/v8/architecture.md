# V8 Architecture

## Repository boundary

The v8 binary exposes only the strict-YAML, singleton-daemon command surface.
The pre-v8 config, Docker CLI, per-project daemon, host Caddy, hosts-file,
random-domain, swarm, sharing, and lifecycle-handler source trees have been
removed rather than retained as a disabled compatibility runtime.

The remaining `src/daemon/` module contains only macOS launchd and Linux
systemd user-service integration for the v8 singleton. SIGINT and SIGTERM are
observed atomically at daemon iteration boundaries so service-manager shutdown
stops new work, aborts read-only log streams, drains active mutation tasks, and
releases the Unix socket and singleton lease through normal ownership teardown.
Workload orchestration lives under `src/control_plane/` and reaches containers
only through typed Engine capabilities.

## Clean-install boundary

V8 has no in-place major-version upgrade, compatibility backend, pre-v8
resource adoption, or config conversion path. Installing v8 creates a new
per-user control-plane state directory and manages only resources bearing its
own installation identity. Pre-v8 containers, volumes, configuration, routes,
certificates, and host integrations remain outside its ownership.

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
2. `DesiredProject`: validated names, services, dependencies, and policies.
3. `ResolvedProjectPlan`: exact compatibility keys, logical resources,
   endpoints, routes, and reversible/destructive operations.
4. Backend mutation requests: typed inputs to narrow effect capabilities.
5. `ObservedState`: Engine objects, service readiness, route/certificate state,
   and persisted ownership.
6. Reconciliation output: applied, blocked before mutation, orphaned, degraded,
   or failed with structured reasons.

No common model exposes Docker argument arrays, CLI output, Caddyfile syntax,
or host package-manager concepts.

## Capability boundaries

The Engine integration is composed from `ContainerLifecycle`,
`ImageReferenceResolver`, `ImageResolver`,
`ImageBuilder`, `NetworkManager`, `VolumeManager`, `CommandExecutor`,
`LogSource`, `HealthObserver`, `EventSource`, and `ResourceMetrics`.

The gateway accepts a complete route and certificate plan and applies it
atomically through an ownership-checked Engine exec to a container-private
admin endpoint. No gateway admin port or socket is exposed on the host. Project
resource provisioning is distinct from service-instance lifecycle: starting
PostgreSQL and ensuring a project database/role are separate reconciliation
steps.

## Singleton daemon and IPC

The daemon is one process per OS user. It uses filesystem events with debounce,
periodic discovery rescans, Engine events with periodic observed-state scans,
per-resource mutation locks, bounded exponential backoff with jitter,
cancellation, and independent reconciliation for unrelated projects.

An unavailable engine is recoverable observed state. Desired state remains
authoritative and all resources reconcile when the selected engine returns.

Runtime operations use versioned local IPC over a user-only Unix socket.
Requests and responses carry protocol version, request ID, typed payload/result,
and structured diagnostics. Logs and events support streaming and cancellation.
Clients do not duplicate daemon reconciliation. Offline schema output and
validation may remain local.

## Transactional state

SQLite stores the installation ID, watched roots, projects, desired/resolved
revisions, ownership, compatibility fingerprints, credentials, logical
resources, routes, observed identifiers, daemon events and operations, orphans,
retention, recovery points, and service-resource migrations.

State transitions are transactional. V8 initializes only an empty database at
its current schema and rejects any other non-empty schema before mutation. A
verified bounded recovery snapshot is created before opening existing current
state. If WAL is enabled, Stackctl pins a SQLite release containing applicable
WAL race fixes.

SQLite does not replace runtime observation. Engine labels reconstruct
ownership after state loss and are compared with persisted state.

## Resource ownership

Every managed Engine object has immutable labels for managed status,
installation ID, schema, resource kind, compatibility fingerprint, owning
project where applicable, desired revision, and retention class. Stackctl never
mutates or deletes an object whose ownership cannot be proven. Persistent names
are deterministic.

## Platform ownership

| Concern | macOS | Linux |
| --- | --- | --- |
| Login start | launchd user agent | systemd user unit |
| Engine transport | Unix socket | Unix socket |
| IPC | Unix socket | Unix socket |
| File events | native watcher | native watcher |
| Trust | Keychain | supported system/browser stores |
| Workloads | Linux engine VM | Linux containers |

Compilation is not platform evidence. Every claimed row requires a recorded
platform acceptance result before release.

The release status and evidence requirements for each concrete combination are
tracked in [Platform support](platform-support.md). The table above describes
ownership, not a support claim.
