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

RabbitMQ vhost recovery exports and restores exact definitions only after both
the selected recovery point and the current safety snapshot prove the vhost has
no queued messages. Restore deletes only the exact vhost, imports its verified
definitions, and confirms that the vhost exists before reporting success.
Non-empty vhosts fail closed because RabbitMQ requires an offline node-data
backup to preserve messages; Stackctl does not drain and republish messages as
if that were an equivalent snapshot.

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

## V7 migration

Migration inventories v7 config, host routes/trust, containers, images,
volumes, logical data, credentials, custom runtime features, and generated
environment. It validates the v8 plan, backs up data, provisions target
resources, restores and verifies data, starts the app, applies environment and
routes, verifies readiness, records reversible cutover, and removes old
resources only after confirmation.

The first phase is a read-only, secret-free typed inventory. A dedicated Engine
capability lists both running and stopped containers carrying only the legacy
`com.stackctl.managed=true` marker. The inventory then requires exact legacy
service, kind, and container-name labels and records the Engine-observed image
identity and mount set alongside configured intent. Duplicate matches,
conflicting labels, missing image identity, volume drift, unexpected mounts,
host binds, anonymous mounts, absent containers, and legacy Swarm targets block
automatic migration with retained source state. Environment and credential
values are never copied into this diagnostic model; only key or field presence
is recorded. Host discovery also reads the generated project `.env`, system
hosts file, legacy Caddy route state, and known public Caddy CA locations as
bounded regular non-symlink files, rejecting concurrent changes. The inventory
retains only `.env` path, size, modification time, and key names; only routes
matching the project's configured domains are retained from global files; and
public CA certificates receive a SHA-256 revision. Exact environment bytes are
not hashed into diagnostic evidence because that could disclose an offline
verifier for weak secrets. Inventory acceptance therefore re-reads the exact
bounded file, requires its size, modification time, and key set to still match,
and copies the bytes into a randomized private rollback envelope. The envelope
and manifest are stored under user-only directories and files; SQLite retains
only the protected reference, envelope checksum, and size. Randomization keeps
that checksum from becoming a verifier for guessed secret values. Repeated
acceptance re-verifies the existing artifact instead of silently replacing
append-only evidence. Schema-16 records remain readable after upgrade; if one
predates protected capture, acceptance permits only a one-way enrichment of
its empty rollback fields while retaining its original inventory and acceptance
time.

Logical-data credentials are reconstructed only when an accepted migration is
being composed. Stackctl reopens the canonical project's exact `.stackctl.toml`
as a bounded regular non-symlink file, requires its SHA-256 revision to equal
the accepted source revision before and after expansion, and verifies the
configured service kind, driver, image, credential-field set, and configured
versus Engine-observed named volumes against accepted evidence. Secret values
exist only in driver-specific redacted credential types and are never added to
the inventory or execution journal. Any config change requires a fresh
inventory and acceptance instead of guessing or silently updating credentials.

Adapter selection consumes only that immutable accepted record. It assigns
each service both its normal v8 deployment strategy and exactly one migration
adapter: a logical database, tenant prefix, bucket, vhost, named-volume
archive, project-workload recreation, stateless recreation, or ephemeral
recreation. Named volumes are archived only when no logical adapter owns the
data transition. The same plan explicitly selects gateway snapshot/cutover,
an installation-scoped legacy Caddy CA transition, and protected
generated-environment handling. The trust adapter must retain the legacy CA
while any accepted project still depends on its rollback path; project
migration never removes shared host trust independently.
Its deterministic revision includes the accepted evidence revision, every
service and volume decision, and every exact route. Unknown drivers,
unsupported mounts, duplicate identities, routes without a routable target,
missing required public CA evidence, and accepted environments without a
protected rollback artifact fail closed before execution. Live cutover remains
a separate later phase.

As part of successful inventory acceptance, Stackctl persists a schema-18
execution record keyed by the canonical project path and accepted evidence
revision. Repeated acceptance must reload the identical record before reporting
success, so a crash between append-only inventory storage and execution-plan
storage is repaired by an exact replay rather than leaving unjournaled work. Its
immutable checkpoint set contains every selected service and volume adapter
plus the route, installation-trust, and generated-environment adapters. Each
checkpoint states whether it requires a recovery artifact and advances only
through verified recovery and target evidence. The project can enter
`prepared` only when every target is verified; no route, environment, or
application cutover is admitted from a partially prepared set. Checkpoint
identity, recovery evidence, target identity, plan revision, and timestamps
cannot be replaced or regressed across daemon restarts. Confirmation is
terminal, while any pre-confirmation phase retains an explicit rollback path.
The preparation coordinator resolves every checkpoint through a common
adapter-strategy registry before writing its initial record. Registry entries
are exact adapter IDs, not global kind handlers, so two services using the same
database engine retain separate source, target, credential, and recovery
context. A registry is scoped to one execution and may borrow the daemon's live
typed Engine, gateway, and service providers. Logical-data adapters own a
validated clone of their accepted source and a boxed execution-scoped provider;
that provider owns its confirmation-only retirement strategy while both may
borrow the shared typed Engine. Adapters do not require global handles or
self-referential daemon storage. The registry must
contain exactly the immutable checkpoint set and
match every selected kind; missing, extra, duplicate, or mismatched bindings
fail before persistence. The coordinator completes and
journals all required recovery artifacts before target work, prioritizes those
recoverable targets, and stops at the last successful checkpoint on any
adapter error. Reconciliation resumes from that exact checkpoint rather than
repeating a verified backup or trusting unrecorded in-memory progress.
Logical-data composition resolves each accepted PostgreSQL, MySQL/MariaDB,
MongoDB, SQL Server, Redis/Valkey, MinIO, or RabbitMQ source against exactly one
prepared shared instance and project tenant. It also requires exactly one
active logical-resource record and one ownership-reconstructed target
container with the prepared compatibility metadata. Missing or ambiguous state
fails before the driver provider enters the execution registry.
Named-volume adapters use the same execution ownership rule: they retain a
validated clone of the exact accepted container and volume set and own their
archive provider for the lifetime of the registry. Source resolution starts
from immutable named-volume execution checkpoints and requires the configured
and Engine-observed volume sets in accepted inventory to agree exactly.
Selected no-op strategies are still concrete registry entries and advance
through target verification, cutover, and confirmation. They cover absent
routes, trust, generated environments, and named volumes, plus volume state
whose recovery is explicitly owned by a logical-data adapter. A no-op entry
cannot satisfy a recovery-requiring checkpoint.
The protected generated-environment strategy reopens and verifies the exact
private rollback artifact accepted for the immutable evidence revision before
it binds the active managed-environment revision as its target. Project `.env`
files remain user-owned and are neither rewritten nor deleted during prepare,
cutover, rollback, or confirmation; v8 environment publication and restoration
occur through the atomic managed-state transactions and container injection.
The installation-trust strategy re-reads every accepted legacy Caddy CA as a
bounded regular non-symlink file and stores the exact certificate set in a
private identity-bound backup before preparation can complete. Cutover ensures
the prepared Stackctl CA identity is trusted. Rollback re-verifies the backup
manifest, identity, checksum, size, and exact accepted bytes before restoring
legacy CA trust, even if the original Caddy certificate files no longer exist.
Project confirmation never removes legacy CA trust; retirement is an explicit
installation-scoped operation only after no accepted project retains a rollback
dependency.
Recreated project workloads and stateless services bind only to an active v8
`ResourceRecord` or project-scoped `LogicalResourceRecord` whose service
identity matches the immutable checkpoint. An explicitly ephemeral adapter is
the only recreation strategy allowed to prepare without a durable target.
Cutover, rollback, and confirmation then follow normal desired-state
reconciliation and retention instead of introducing a second container
lifecycle path inside migration.
Named-volume checkpoints bind a dedicated legacy migration provider to the
exact accepted service, Engine container identity, and identical configured and
observed volume-name set. The provider must quiesce and restart the source while
producing verified recovery evidence, restore and verify an exact v8 target
before the checkpoint can become prepared, and reverify that target at cutover.
Rollback verifies the retained legacy source and leaves the prepared target as
retained evidence. Only project-wide confirmation may ask the provider to
retire the exact accepted legacy source; mismatched volume evidence fails during
registration before provider mutation.
Named volumes and logical data use the same recoverable-provider lifecycle
rather than independent orchestration APIs. Logical-data registration binds the
exact accepted service, driver, Engine container identity, and complete
non-secret logical-data map. The selected adapter kind must match exactly one of
the PostgreSQL, MySQL, MongoDB, SQL Server, Redis, Valkey, MinIO, or RabbitMQ
providers. A driver-specific provider owns native backup, target provisioning,
restore, and verification, but cannot bypass the common recovery-first barrier,
rollback source verification, or confirmation-only retirement rule.
Driver providers must not pass a v7 container through the v8 `OwnedContainer`
API. The Engine exposes a separate narrow v7 command target containing the
immutable accepted container ID plus its container, service, and kind labels.
Before each attached command starts, the Docker-compatible adapter re-inspects
that ID and requires `com.stackctl.managed=true` and exact values for all three
identity labels. Drift refuses execution before the Engine creates an exec
session; later status checks remain bound to that exact container ID.
After authorization, accepted-v7 and v8-owned sessions use the same bounded
attached and streaming transport. Timeout enforcement, secret-safe stderr
draining, output limits, status polling, and exit-code handling therefore have
one implementation; only target authorization and session creation differ.
The PostgreSQL logical-data provider binds the accepted project, service,
driver, kind, container name, immutable Engine ID, database name, and identical
configured and observed named-volume set before it runs `pg_dump`. Its private
recovery manifest is additionally bound to the accepted inventory revision.
Target preparation verifies that artifact before any v8 command, then
idempotently provisions and resets the deterministic database before each
`pg_restore`, making a restart during preparation safe to replay. Catalog
verification proves the target database owner and rejects invalid indexes or
unvalidated constraints. Rollback verifies that the exact legacy database
remains reachable.

Only project confirmation enters the PostgreSQL source-retirement strategy.
The Docker-compatible Engine re-inspects the immutable container ID and exact
legacy labels, requires its current named-volume set to equal accepted
evidence, proves every accepted volume exists, and only then stops and removes
the container. Each named volume is removed only after a scan including stopped
containers proves it unused. A replay succeeds when the container and every
accepted volume are already absent. If the container is absent while a
same-named volume remains, cleanup fails loudly: Docker-compatible named
volumes have no immutable ID, so Stackctl cannot safely distinguish an
interrupted cleanup from a newly recreated volume with the same name.

The MySQL-family logical-data provider applies the same accepted source,
recovery, verification, and retirement boundaries. Because v7 uses the
`mysql` driver for both implementations, the provider resolves MySQL versus
MariaDB only from the accepted configured-image evidence and refuses a flavor
mismatch. It invokes the matching native dump and client tools. The source
database is a positional dump argument rather than a `--databases` entry, so
the artifact contains schema contents without instructions to recreate the
legacy database name. MySQL and MariaDB do not share a portable dump option for
removing explicit object definers. Stackctl therefore scans the streamed
artifact before accepting recovery, removes the newly created recovery
directory, and fails loudly if it contains an explicit `DEFINER` assignment;
it never imports a legacy security principal into a shared v8 instance. Target
preparation drops only the deterministic v8 schema, reapplies its restricted
user and grants, streams the verified dump, and checks the resulting database
and authenticated username. The complete sequence is safe to replay after an
interrupted preparation; rollback checks the exact retained v7 database
through the separately authorized v7 command target, and confirmation
delegates to the shared exact Engine retirement capability.

The MongoDB logical-data provider likewise binds one exact accepted database,
container, named-volume set, and recovery identity. Credentials are carried in
redacted command environments rather than command arguments. `mongodump`
streams an archive scoped to the accepted source database; target preparation
reverifies it before any v8 command, drops only the deterministic v8 database,
and reapplies its deterministic restricted user. `mongorestore` explicitly
maps `source.*` to `target.*`, so the archive cannot recreate the legacy
database name. Target and retained-source checks require the expected database,
authenticated username, and successful ping. Preparation is safe to replay,
rollback retains and verifies the separately authorized source, and only
confirmation delegates its exact container and accepted named volumes to the
shared Engine retirement capability.

The SQL Server provider uses the accepted `sa` identity to create a native
`COPY_ONLY` backup with page checksums inside the separately authorized v7
container. Recovery storage is bound to the accepted evidence revision and is
reverified before the target receives a command. The owned persistent migration
target first runs `RESTORE VERIFYONLY`, then drops only the deterministic v8
database and login, restores the archive under the deterministic database name,
and reapplies the deterministic login, user, password, and role plan. This
isolated target permits SQL Server to retain the backup's physical file layout
without creating the legacy database identity or colliding with another tenant.
The complete reset, restore, and provisioning sequence is replay-safe. Both the
retained source and v8 target must return their expected database and login;
confirmation alone delegates exact accepted resource retirement to the shared
Engine capability.

The Redis and Valkey logical-data providers bind the accepted implementation,
container, logical database, named-volume set, and recovery identity before
running the matching native client. A single server-side script takes a
binary-safe snapshot of every source key's serialized value and remaining TTL.
The snapshot maps each raw source key to the deterministic v8 tenant namespace
by prepending `stackctl:<project>:<service>:`. Stackctl deliberately neither
removes nor interprets an application-level prefix already present in the raw
key: recovery is lossless and deterministic rather than a guess about framework
configuration. Target preparation reverifies the accepted-revision-bound
artifact before mutation, removes expired records, stages all remaining values
under a private temporary prefix, and then atomically replaces only the exact
tenant prefix. Replaying preparation produces the same target contents. Both
the restricted v8 ACL user and retained v7 credential must return their exact
authenticated identities and a successful ping; confirmation alone delegates
the accepted container and named volumes to exact Engine retirement.

The MinIO logical-data provider binds the exact accepted bucket, credential,
container, named-volume set, and recovery identity. It first queries bucket
versioning and fails before export when version history is enabled, because a
current-object mirror cannot preserve historical versions or delete markers.
For an unversioned bucket, it streams a tar archive of the current object tree
directly from the separately authorized v7 container into private,
accepted-revision-bound recovery. Target preparation reverifies that artifact
before mutation and mirrors it with overwrite and removal into only the
pre-provisioned deterministic `stackctl-<project>-<service>` bucket. The
complete replacement is safe to replay. Exact source and target credentials
must each authenticate and stat their bound bucket; confirmation alone
delegates exact accepted container and volume retirement to the Engine.

The RabbitMQ logical-data provider binds the exact accepted container,
named-volume set, default `/` vhost, legacy application credential, and recovery
identity. It first inventories every queue and fails before export if any
message is present, because a definitions archive cannot preserve message
bodies. Definitions must contain exactly the accepted user and vhost, the
exported salted password hash must match the supplied legacy credential, and
global or cross-vhost topology is refused. Accepted empty-message topology is
rewritten to only the deterministic v8 vhost and credential before private,
accepted-revision-bound storage. Target preparation reverifies the artifact,
replaces that exact vhost, and is safe to replay. Source and target definition
exports must retain their exact scoped identities and remain empty of messages;
confirmation alone delegates exact accepted container and volume retirement to
the Engine. Projects with queued messages remain on v7 until a message-body
migration or an explicit compatibility execution path is available.

Cutover invokes the prepared strategies in deterministic dependency order and
publishes routes last. The journal advances the entire project to `cutover`
only after every idempotent operation succeeds. Route ownership, application
project intent, the complete managed environment, and the project-wide
cutover checkpoint are then committed in one SQLite transaction. A rejected
or interrupted commit exposes none of those desired-state changes; external
idempotent side effects are replayed from `prepared`, never mistaken for a
complete cutover. Desired project identity and canonical path are checked
against the immutable execution before any adapter side effect runs.
The gateway strategy never edits one route in isolation. Preparation stores a
private canonical artifact for the complete pre-cutover gateway snapshot and
binds the complete target revision. Cutover atomically replaces and verifies
the provider's full snapshot. Rollback first reopens the recovery point and
checks its identity, manifest checksum, size, and canonical route bytes before
atomically restoring and verifying the complete prior snapshot; tampering
blocks route mutation.
Rollback invokes the reverse order so new routes are withdrawn first and then
atomically restores prior route ownership, project intent, and the managed
environment while retaining target logical resources and recording one
project-wide terminal rollback. Restored identity is checked before adapter
side effects. Confirmation is accepted only from `cutover`; it retires
retained sources through the same idempotent strategies before the
irreversible `confirmed` record is written.

`stackctl daemon migration inventory [PATH]` exposes that phase deliberately;
normal watched-root discovery still rejects TOML. The singleton accepts only an
absolute project path below one of its authoritative watched roots, reads an
exact regular non-symlink `.stackctl.toml` within the normal configuration-size
limit, verifies that its bytes did not change during expansion, and uses only
the installation-selected Engine. The command prints the source revision,
complete evidence revision, configured and observed service identities, route
count, CA-capture requirement, and every blocker. A blocked inventory exits
unsuccessfully after reporting all issues, never changes the source project,
and receives no acceptance token.

A blocker-free preview returns a purpose-bound confirmation token but still
writes no state. `stackctl daemon migration accept [PATH]
--confirmation-token TOKEN` performs a fresh config, Engine, and host-artifact
inventory, recomputes the complete evidence digest, and rejects the request if
any source, container, image, mount, route, public host artifact, environment
metadata, or blocker evidence changed. When generated environment exists,
acceptance also fails before persistence if exact protected rollback capture or
immediate verification fails. Only an exact replay is appended to the SQLite
acceptance journal. Each record contains the canonical path, deterministic
project identity, source revision, full secret-free inventory, protected
environment rollback evidence when required, evidence revision, and acceptance
time. A second path
claiming an already accepted project identity fails loudly; Stackctl never
renames, hashes, or repairs it. Later migration adapters must match an exact
accepted evidence revision before acting.

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
