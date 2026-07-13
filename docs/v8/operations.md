# V8 Operational Model

## Gateway and localhost TLS

Default routes use `{project}-{service}.stackctl.localhost`, relying on the
special-use localhost namespace instead of host entries. Setup performs a
loopback resolution self-test and reports a deliberate fallback when the
platform is misconfigured.

One gateway publishes loopback 80/443 and receives a complete route set from
the daemon. Caddy is acceptable initially only as a pinned, invisible container
implementation. The daemon applies one complete serialized configuration,
retains the last good revision, and verifies both config and traffic readiness.

Stackctl owns the CA and a renewable wildcard leaf for
`*.stackctl.localhost`. Key material is user-private and mounted read-only into
the gateway. Application containers use internal plain HTTP and never expose or
ask users to trust their own CAs.

## Supply chain and upgrades

Built-in images use immutable digests. Runtime images are content-addressed by
base digest, runtime version, PHP extensions, system packages, JavaScript
runtime, and immutable configuration. Equivalent projects reuse layers.

Project-local `.stackctl.lock.yaml` records bind exact configured image or
preset sources to immutable sha256 digests. Registry planning validates and
applies those bindings before producing Engine requests; stale or malformed
locks fail closed rather than silently advancing an artifact.

Mutable registry lookup uses the typed `ImageReferenceResolver` Engine
capability and the registry distribution endpoint. It returns a validated
repository manifest digest without parsing Docker or Podman CLI output. Making
that already-pinned image locally available remains the separate
`ImageResolver` capability.

The CLI submits bounded source mappings over typed local IPC. The singleton
daemon resolves them using the persisted Engine selection and returns the exact
same key set. The CLI rejects missing, additional, or mutable results before an
atomic YAML lock publication.

Built-in generation never executes mutable remote installer pipelines such as
`curl | sh` or `curl | php`. Downloaded tools require a pinned source and
checksum or signature. Releases include SBOM and provenance.

Patch updates are explicit plans with rollback. Major runtime or data-service
upgrades create a new compatibility identity and require verified migration.
Stackctl never silently advances a major version.

## Host dependency boundary

Normal reconciliation does not require host Caddy/nginx, PHP, Composer,
Node/Bun, database/cache binaries, curl, OpenSSL, tail, ps, kill, or privileged
shell scripts. Rust libraries, Engine API calls, containerized tools, or narrow
OS APIs replace them.

Allowed boundaries are OS service/trust mechanisms, the selected container
engine, optional Git bootstrap, and explicit sharing providers. The host
dependency audit lists every remaining executable, owning feature, and failure
behavior.

`just audit-v8-host-dependencies` enforces the normal v8 source boundary in CI.
It rejects direct host process execution and imports of the legacy Docker CLI,
host web-server, database-tooling, and per-project daemon runtimes. The sole
direct process executor inside the v8 control plane is
`src/control_plane/tls/process_host_command_executor.rs`; it is confined to
explicit operating-system trust-store setup and removal. Login-service setup is
the separately allowlisted OS integration boundary. Neither path participates
in normal project reconciliation.

Container command names such as `php`, `psql`, or `redis-cli` may appear in
typed Engine requests. They execute inside owned Linux containers and are not
host executable dependencies.

The complete v8 host-executable inventory is:

| Executable | Owning feature | Invocation and failure boundary |
| --- | --- | --- |
| `security` | Explicit macOS CA trust setup/removal | Invoked only by `stackctl daemon trust`; a non-zero status leaves trust unchanged and returns the exact adapter error. |
| `certutil` | Explicit Windows Current User CA trust setup/removal | Invoked only by the trust adapter; a non-zero status aborts the requested trust change. Windows daemon runtime remains unsupported until named-pipe acceptance exists. |
| `sudo`, `update-ca-certificates`, `rm` | Explicit Debian-family CA trust setup/removal | Invoked only by the trust adapter; privilege denial or a non-zero update aborts setup/removal with no reconciliation fallback. |
| `launchctl` | Explicit macOS login-service install/status/removal | Invoked only by `stackctl daemon service`; failure is reported and does not affect project reconciliation. |
| `systemctl` | Explicit Linux user-service install/status/removal | Invoked only by `stackctl daemon service`; failure is reported and does not affect project reconciliation. |

The selected Docker-compatible Engine is contacted over its API socket or
named pipe; Stackctl does not invoke a `docker` or `podman` executable in the v8
runtime. Caddy is an immutable workload-plane image, not a host executable.

## Retention, backup, and deletion

Removing or invalidating config follows:

```text
active -> orphaned -> stopped/credential-disabled -> retained
       -> adopted | restored | explicitly pruned
```

Automatic collection is limited to proven-disposable temporary containers,
expired build cache, superseded unreferenced images, and rotated logs.
Databases, buckets, queues, volumes, and backups are never implicitly deleted.

Backups are host-visible Stackctl artifacts with project/resource identity,
source compatibility fingerprint, checksum, creation time, and restore
requirements. Migration does not switch routes or environment until restore and
readiness verification pass. Rollback material remains until confirmation.

Uninstall offers keep-data and delete-data modes. Delete-data enumerates owned
resources, verifies installation labels, checks backup policy, and requires
explicit destructive approval.

## V7 migration

Migration inventories v7 config, host routes/trust, containers, images,
volumes, logical data, credentials, custom runtime features, and generated
environment. It validates the v8 plan, backs up data, provisions target
resources, restores and verifies data, starts the app, applies environment and
routes, verifies readiness, records reversible cutover, and removes old
resources only after confirmation.

An existing volume is never attached to an incompatible image or different
engine as an implicit upgrade. Unsupported projects retain a precise diagnostic
and v7 rollback path.

## Failure and recovery

Health distinguishes engine absence, container absence/stoppage/restart,
process health, service readiness, authentication, logical-resource drift,
gateway drift, certificate expiry, name collision, approval blocking, orphaning,
and destructive replacement.

Project status reports durable lifecycle and last Engine-observed health as
separate fields. Health snapshots remain in daemon memory, are timestamped, and
publish only after a complete Engine reconciliation; they are never written to
SQLite. Missing or stale observations are `unknown`, not implicitly healthy.
Losing the selected Engine adapter immediately clears the complete live-health
snapshot before reconnect backoff begins, so disconnected state is never
reported from a recently successful pass.
Browser opening accepts `healthy` and `running_unverified` routes and otherwise
fails with the exact service state and observation time instead of issuing an
ad-hoc application HTTP probe.

Engine events provide prompt reaction while periodic scans restore correctness.
Repeated failures back off with jitter and one durable diagnostic rather than
log spam. One project or shared service failure does not block unrelated work.
