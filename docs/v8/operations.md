# V8 Operational Model

## Gateway and localhost TLS

Default routes use `{project}-{service}.stackctl.localhost`, relying on the
special-use localhost namespace instead of host entries. Setup performs a
loopback resolution self-test before creating or changing daemon state. Empty
or non-loopback answers fail with the exact probe hostname and address. The
user must repair the host resolver and retry; an explicit custom-domain
fallback remains unimplemented and is not selected automatically.

One gateway publishes loopback 80/443 and receives a complete route set from
the daemon. Caddy is acceptable initially only as a pinned, invisible container
implementation. The daemon applies one complete serialized configuration,
retains the last good revision, and verifies both config and traffic readiness.
The admin endpoint listens only on `localhost:2019` inside the gateway network
namespace. Stackctl reaches it through an ownership-checked Engine exec and
streams the complete native JSON document over stdin; no admin port or Unix
socket is published or mounted onto the host. Disposable `/config`, `/data`,
and `/tmp` state uses bounded tmpfs mounts so the image cannot leave anonymous
volumes behind.

Stackctl owns the CA and a renewable wildcard leaf for
`*.stackctl.localhost`. Key material is user-private and mounted read-only into
the gateway. Application containers use internal plain HTTP and never expose or
ask users to trust their own CAs.

`stackctl daemon trust rotate` creates an immutable replacement generation,
installs and verifies its exact OS trust identity, and atomically selects the
new generation while retaining the previous trusted identity. It then requests
daemon reconciliation and waits for the ready gateway to publish the exact
replacement generation before removing the previous trust entry. A failed
install, verification, gateway activation, or removal atomically reselects the
prior generation and restores its trust state. If filesystem rollback fails,
both identities remain trusted and the full recovery error is reported.
Routine leaf renewal preserves the current CA identity and uses the same atomic
active generation pointer.

Certificate-store mutations use a user-private advisory lock shared by the
daemon and CLI. Trust commands also share a separate rotation lock: rotation
holds it exclusively across gateway reconciliation but releases the store lock
so the daemon can read the replacement generation. This prevents concurrent
trust changes without deadlocking gateway activation.

## Supply chain and upgrades

Built-in images use immutable digests. Application runtime images are
content-addressed by the locked base digest, target Linux platform, normalized
PHP extension set, exact Composer, Node, and Bun image digests, and immutable
ownership metadata. Equivalent projects reuse the verified Engine build cache.
Every referenced image is made available through the typed `ImageResolver`
before the daemon submits a network-disabled Engine build. Workers are rebound
to the exact derived image used by their application. Scheduler commands use a
bounded, non-shell Engine exec inside the exact owned application container.
The daemon dispatches only the current wall-clock minute, never replays missed
minutes after downtime, and skips a minute if that scheduler is still running.

Additional system libraries belong in a digest-pinned custom application base.
Normal reconciliation never runs a host package manager or performs an online
`apt`, `apk`, or equivalent install from mutable package repositories. Stackctl
therefore treats the custom base digest as the complete system-library contract
rather than pretending arbitrary package names are reproducible inputs.

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

Stackctl publishes its PHP base from a manifest-pinned FrankenPHP base for
amd64 and arm64. Its Dockerfile frontend, Debian package snapshot, and PECL
extension versions are explicit inputs rather than floating build-time sources.
Publication prepares the supported extension catalog, emits an SBOM and
maximum-mode provenance, and keyless-signs the resulting OCI manifest. The
built-in preset source is resolved to an immutable digest through the normal
lock workflow before project reconciliation.

Network-disabled project builds only enable locally available extension modules
and verify them through PHP. Composer, Node, and Bun are copied from separately
digest-pinned image stages. Project builds do not download or inject installer
scripts and never execute mutable remote installer pipelines such as `curl | sh`
or `curl | php`. Other downloaded tools require a pinned source and checksum or
signature.

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
| `security` | Explicit macOS CA trust setup/rotation/removal | Invoked only by `stackctl daemon trust`; a non-zero status leaves the prior active generation selected and returns the exact adapter error. |
| `sudo`, `update-ca-certificates`, `rm` | Explicit Debian-family CA trust setup/rotation/removal | Invoked only by the trust adapter; privilege denial or a non-zero update aborts the trust transition with no reconciliation fallback. |
| `launchctl` | Explicit macOS login-service install/status/removal | Invoked only by `stackctl daemon service`; failure is reported and does not affect project reconciliation. |
| `systemctl` | Explicit Linux user-service install/status/removal | Invoked only by `stackctl daemon service`; failure is reported and does not affect project reconciliation. |
| `open`, `xdg-open` | Explicit interactive `stackctl open` browser handoff | Invoked only after daemon-authoritative route and readiness checks; `--no-browser` and `--non-interactive` avoid the boundary, and opener failure is returned directly. |

The selected Docker-compatible Engine is contacted over its Unix socket;
Stackctl does not invoke a `docker` or `podman` executable in the v8 runtime.
Caddy is an immutable workload-plane image, not a host executable.

`stackctl setup --dir <DIR>...` is the normal one-time installation path. It
canonicalizes every distinct watched root and verifies `.localhost` loopback
resolution before changing host state. It then installs singleton CA trust and
the login service. If service installation fails, setup removes trust only when
that invocation added it; existing trust is retained. A trust rollback failure
is reported together with the service failure.

Initial CA trust installation also remains transactional through exact trust
verification and active certificate-generation selection. A failure after new
OS trust is added removes that exact identity and verifies it absent. Existing
trust is never removed by this rollback, and rollback failure is reported with
the original finalization error.
If an OS trust command returns failure after partially installing the exact CA,
Stackctl detects that identity, removes it, and verifies it absent before
returning the original failure. An ambiguous or failed rollback is returned as
part of the error rather than accepted as clean host state.
The inverse operation has the same guarantee: if an OS untrust command removes
the exact CA but reports failure, Stackctl reinstalls and verifies that CA before
returning the original error. A command that fails without changing trust is
left untouched.

On Debian-family hosts, the privileged managed-root file and the
`update-ca-certificates` refresh are one transaction. Failed installation
refreshes remove the partial root and refresh again; failed removal refreshes
restore the exact root and refresh again. Partial privileged state is never
reported as a completed trust transition.

`stackctl daemon service status` checks the selected service manager in
addition to the definition file. A stale launchd plist or systemd user unit is
reported as installed but not running, with an explicit reinstall command; it
is never presented as a healthy login-time daemon. A manager-active process
must also answer one bounded, correlated IPC probe; an unresponsive singleton
fails status with the same explicit reinstall guidance.

Service installation snapshots an existing regular definition before atomic
replacement. After manager activation and the immediate running-state check,
Stackctl requires a bounded, correlated IPC `Ping`/`Pong` from the singleton.
If activation, the process check, or protocol readiness fails, a fresh install
removes its partial definition and manager state. An update restores and
reactivates the exact previous definition when it had been running, then
requires that restored singleton to answer IPC too. Failure to complete or
verify that rollback is reported together with the original installation error
instead of leaving an apparently successful setup.
Every installation path canonicalizes its watched roots first. Missing paths,
non-directories, and duplicate canonical roots fail before the definition or
service manager is changed.

Login services do not create unbounded duplicate stream files. Linux routes
stdout and stderr through journald; launchd routes those duplicate streams to
`/dev/null` while Stackctl retains explicit persistent events in its own log
sink. That sink keeps at most seven distinct clock days and, for each day, one
10 MiB active segment plus one 10 MiB previous segment. An individual entry
larger than the segment bound is not persisted.

Service uninstall tolerates manager commands that report an already-absent
unit, then verifies the resulting state before deleting the host definition.
launchd must report the service stopped. systemd must report it stopped and
disabled. Failed or ambiguous verification leaves the definition in place so
the cleanup can be retried without losing its exact target.

## Retention, backup, and deletion

Removing or invalidating config follows:

```text
active -> orphaned -> stopped/credential-disabled -> retained
       -> adopted | restored | explicitly pruned
```

For RabbitMQ, daemon reconciliation removes the exact disabled project user
before an otherwise unreferenced broker is stopped. The vhost and queued
messages remain retained for explicit restore, adoption, or prune.
Redis and Valkey use the same ownership-proven lifecycle boundary to delete the
disabled ACL user through the retained administrator credential. The tenant's
key prefix and all matching data remain untouched.
PostgreSQL uses that boundary to apply `NOLOGIN` only to the exact disabled
project role. The database, role, ownership, and stored data remain intact, and
the administrator secret is passed only in the Engine command environment.
MySQL and MariaDB delete only the exact disabled tenant user through their
retained root credential. The project schema and all stored data remain intact,
and the root secret is passed only in the Engine command environment.
MongoDB likewise deletes only the exact disabled database user. Its database
and collections remain intact, and `mongosh` reads the administrator secret
only from the Engine command environment.
SQL Server applies `DISABLE` only to the exact disabled project login. Its
database, mapped user, permissions, and data remain intact, and `sqlcmd` reads
the administrator secret only from the Engine command environment.
MinIO lists identities through machine-readable administrator output and
disables only the exact enabled project user. The identity, policy attachment,
buckets, and objects remain intact. RustFS remains dedicated and does not use
MinIO IAM assumptions.

Automatic collection is limited to proven-disposable temporary containers,
expired derived build images, and rotated logs. A derived image is eligible
only after seven days when its complete labels prove current-installation build
cache ownership, it is absent from the active runtime set, and the Engine
reports that zero running or stopped containers reference it. Unknown reference
counts retain the image.
Databases, buckets, queues, volumes, and backups are never implicitly deleted.
Orphaned disposable containers have a fixed seven-day retention window. After
that window, the daemon removes one only when the complete Engine discovery and
its durable snapshot prove exact current-installation ownership; it then
atomically retires the unchanged state record. Missing objects converge state
on retry. Persistent containers, every volume, and every logical resource still
require an explicit destructive workflow with the documented backup policy.

Backups are host-visible Stackctl artifacts with project/resource identity,
source compatibility fingerprint, checksum, creation time, and restore
requirements. Migration does not switch routes or environment until restore and
readiness verification pass. Rollback material remains until confirmation.

PostgreSQL and MySQL/MariaDB logical deletion use one effect-free explicit plan:

```text
stackctl daemon prune plan <project-id> <service-id> <recovery-point-id>
```

The project must already be unregistered, the exact database-and-role or
schema-and-user logical resource must be orphaned, its credential must be
disabled, and the selected immutable recovery point must match the logical
identity and compatibility fingerprint. The daemon resolves an explicit
`postgresql_logical` or `mysql_logical` strategy, then returns a secret-free
plan and stable confirmation token bound to the installation, retained state,
orphan timestamp, credential identity, and verified artifact evidence. It
never chooses a recovery point, repairs ambiguous state, or mutates the Engine
while planning. Execution requires the returned token:

```text
stackctl daemon prune execute <project-id> <service-id> <recovery-point-id> \
  --confirmation-token <token>
```

The singleton persists the selected strategy in a secret-free bounded
operation, then regenerates the plan immediately before any Engine mutation. A
stale token, changed state, registered project, missing backup, ambiguous
container, or ownership mismatch fails before the database server is touched.
The exact shared container runs the strategy adapter: PostgreSQL terminates
matching sessions before idempotent database and role removal; MySQL/MariaDB
idempotently removes the schema and restricted user. Both use runtime-only
bootstrap secrets, and arguments, IPC, events, and durable operation state
contain no credential value. Only after the adapter succeeds does one SQLite
transaction forget the unchanged orphaned logical resource and disabled
credential. The verified recovery point is retained. The disabled managed
environment is removed only when no project logical resources or credentials
remain.

An interrupted operation is replayed when both exact state records remain,
because each enabled adapter is idempotent. If the atomic SQLite
retirement already committed, restart recovery completes the durable operation
without touching the Engine again. Partial durable retirement fails loudly for
manual inspection.

The normal backup command creates MySQL/MariaDB recovery points by streaming a
consistent logical dump from the exact owned shared container into private,
immutable Stackctl storage and verifying its checksum. Restore re-verifies the
catalog identity, checksum, size, and ownership before importing into a
separately owned retained target. An authenticated schema check precedes atomic
environment cutover. Explicit confirmation retires only the exact source
schema and user; rollback restores the original environment and retains the
target. Other logical service kinds fail closed until they have service-specific
backup, restore, and deletion adapters; Stackctl does not reinterpret container
removal as data deletion.

Dedicated project-volume backup resolves the exact active physical ownership,
quiesces only its owning service, streams the named mount through the Engine
API, and restarts the service even when archive creation fails. Restore accepts
only a cataloged recovery point matching that active volume and the current
validated YAML plan. Before replacing data, the daemon creates a deterministic
`{operation-id}-pre-restore` safety recovery point for the current volume. It
then removes the exact service and volume, recreates the empty desired volume,
uploads the selected archive before start, and waits for service readiness.
Missing artifacts, checksum drift, ambiguous ownership, or a mismatched desired
plan fail before destructive restore begins. Installation delete-data includes
every dedicated volume and its selected recovery identity in the visible plan
and confirmation token. The artifact is re-verified at confirmation and again
immediately before cleanup; only the exact resulting volume-name set can pass
the Engine deletion guard. A project-owned volume observed in the Engine but
absent from durable authorization blocks the entire cleanup before mutation.

RabbitMQ vhost recovery supports empty queues and non-empty durable classic
queues containing only persistent messages. Backup suspends broker listeners,
closes clients, stops the shared broker, and archives only the exact vhost's
message-store subtree together with credential-free scoped topology. Because
RabbitMQ message storage must be copied while the node is stopped, this creates
a brief broker-wide maintenance window for every project sharing the instance.
Restore first detaches the broker from Stackctl's private network and keeps it
detached from the current safety snapshot through topology replacement,
message-store extraction, restart, and queue-count verification. It validates
every tar path and link before extraction, reapplies the current recorded
user's permissions without restoring password hashes, and reconnects the
broker under its deterministic DNS alias only after verification. Non-durable
queues, non-persistent messages, quorum queues, and streams fail closed.

`stackctl daemon service uninstall` defaults to keep-data behavior. The
equivalent explicit form is `stackctl daemon service uninstall --keep-data`.
Both stop and remove only the login service definition; SQLite state, verified
backups, trust material, and Engine resources remain intact.

The destructive spelling is deliberately separate and requires both
`--delete-data` and `--confirm-delete-data`. The CLI asks the authoritative
daemon for an exact deletion plan, confirms its content-derived token, and
waits for durable terminal state. The daemon freezes ordinary reconciliation,
re-verifies every recovery artifact immediately before mutation, serializes
logical pruning, and removes only Engine objects carrying exact installation
ownership. The CLI removes matching OS trust, stops the login service, and
deletes runtime state only after writing a terminal marker. An interruption can
resume from durable lifecycle state; a failed prune requires the user to rerun
the confirmed command so Stackctl revalidates and retries the same exact
operation. Unsupported service kinds, missing evidence, ownership drift, and
ambiguous runtime paths fail closed without removing the login service or
runtime state.

## Clean installation

V8 does not inspect, import, adopt, modify, or delete resources from an earlier
major version. A new installation creates its own state, trust, network,
gateway, and ownership-labeled resources. Existing containers and volumes must
be handled independently before or after installing v8; they are never attached
to v8 services as an implicit upgrade.
Durable project-command payloads also reject unknown fields. Removed payload
shapes, including environment-bearing command records, are not silently decoded
or revised in place.

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

The singleton keeps one installation-scoped managed-container Engine event
subscription. A bounded channel coalesces bursts into prompt full
reconciliation, and the last processed event cursor prevents a reconnect from
replaying the cursor event. Stream failure or closure reconnects with bounded
exponential backoff and jitter. Periodic complete discovery remains the
correctness fallback when events are lost or coalesced.
Repeated failures back off with jitter and one durable diagnostic rather than
log spam. One project or shared service failure does not block unrelated work.
