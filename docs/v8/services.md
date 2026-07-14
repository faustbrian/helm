# V8 Service Sharing Strategies

## Compatibility identity

A shared instance is keyed by engine, major version, immutable image digest,
extension/plugin/module profile, immutable server settings, persistence mode,
authentication/isolation capability, and platform architecture where relevant.
A different key produces another container, not a native fallback. Project
logical resources reconcile independently and idempotently.

## Preset matrix

The `sharing key` for every shared row starts with the complete compatibility
identity above. The row names the additional implementation-specific boundary.
Dedicated rows use project plus service identity and never join a compatibility
pool. "Engine state only" means the current contract can prove that the process
is running but does not yet claim protocol-level readiness.

| Preset | Default scope and sharing key | Logical isolation | Backup and upgrade boundary | Dedicated when |
| --- | --- | --- | --- | --- |
| PostgreSQL | Shared by major/profile | Database and restricted role/password | Logical dump/restore; new instance for major upgrade | Extensions, locale, auth, or immutable settings differ |
| MySQL | Shared by engine/major/profile | Schema and restricted user/password | Logical dump/restore; new instance for major/plugin change | Plugins, SQL mode, charset defaults, or settings differ |
| MariaDB | Shared separately by major/profile | Schema and restricted user/password | Logical dump/restore; never reinterpret a MySQL volume | MySQL-only assumptions or incompatible profile |
| MongoDB | Shared when auth isolation is proven | Database-scoped user/role | Logical dump/restore; new instance for major upgrade | Topology, plugin, or auth requirements differ |
| SQL Server | Shared by edition/version/profile | Database plus restricted login/user | Native logical backup/restore | Collation, feature, edition, or isolation differs |
| Redis | Shared by major/module profile | ACL user/password and key prefix; global commands denied | Persistence and supported logical export | Commands cannot be safely ACL-scoped |
| Valkey | Shared by major/module profile | ACL user/password and key prefix; global commands denied | Persistence and supported logical export | Commands cannot be safely ACL-scoped |
| Dragonfly | Shared only after parity is proven | Project credential and prefix where supported | Engine snapshot/restore | Isolation is weaker than requested |
| Memcached | Dedicated by default | Prefix is convention, not security isolation | No authoritative persistent backup | Unless weak sharing is explicitly accepted |
| MinIO | Shared by compatible profile | Bucket, key/secret, bucket policy | Current-object export for unversioned buckets; version history fails closed | Global config or policy differs |
| RustFS | Dedicated until the external admin-client lifecycle is proven | Whole project instance | Bucket export; new instance for storage-format boundary | Default; share after bucket, identity, policy, and recovery acceptance tests pass |
| Garage | Shared only after policy behavior is proven | Bucket and scoped key where supported | Bucket export plus metadata backup | Isolation cannot be proven |
| LocalStack | Dedicated by default | Whole emulated account/container | Explicit export where supported | Default; share only after complete namespacing |
| OpenSearch | Shared only with tested security | Project indexes and restricted role/user | Snapshot/restore; major/plugin boundary | Security, plugins, or settings differ |
| Elasticsearch | Shared only with tested security | Project indexes and restricted role/user | Snapshot/restore; major/plugin boundary | License, security, plugins, or settings differ |
| Meilisearch | Shared only with tested key isolation | Project indexes and restricted key | Dump/snapshot; major boundary | Master access or global operations are required |
| Typesense | Shared only with tested scoped keys | Project collections and scoped key | Snapshot/export; major boundary | Global configuration or key scope is insufficient |
| Application/FrankenPHP | Project | Dedicated runtime; image may be shared | Source/data handled separately; image fingerprint boundary | Always project-scoped |
| Reverb | Project | Project process and credentials | No persistent service data by default | Always project-scoped |
| Horizon/workers | Project | Project process using project credentials | Queue data belongs to cache service | Always project-scoped |
| Scheduler | Project execution | Timed container exec or supervised process | No independent persistent data | Always project-scoped; avoid idle container when exec works |
| Dusk/Selenium | Ephemeral | Whole browser container | Disposable | Always per test/project run |
| Gotenberg | Shared by exact image/config | Stateless HTTP | No service data | Fonts, policy, or config differs |
| Mailpit | Shared | Authenticated SMTP username tag and deterministic route | Optional message export | Attribution or access isolation differs |
| MailHog | Unsupported; use Mailpit | None | None | No maintained multi-architecture artifact contract is available |
| RabbitMQ | Shared by major/plugin profile | Vhost, user/password, permissions | Broker-wide quiesced maintenance window; credential-free scoped topology plus durable persistent classic-queue message-store backup and network-isolated safety-backed restore; non-durable, non-persistent, quorum, and stream messages fail closed | Plugins, policies, topology, isolation, maintenance tolerance, or required recovery type differs |
| Soketi | Dedicated routable project service | Stable project app ID/key/secret | Stateless; no service volume | Always project-scoped until cross-project isolation is proven |

## Operational contract matrix

| Preset | Credential model | Endpoint model | Readiness contract | Project removal behavior |
| --- | --- | --- | --- | --- |
| PostgreSQL | Stable database owner role and password; separate managed administrator secret | Internal host and port plus project database through managed `DB_*` values | Administrator-authenticated server probe followed by idempotent role/database provisioning | Apply `NOLOGIN` to the exact project role; retain role, database, and data |
| MySQL | Stable restricted user/password and project schema; separate root secret | Internal host and port plus project schema through managed `DB_*` values | Root-authenticated probe and idempotent schema/user/grant provisioning | Delete the exact project user; retain schema and data |
| MariaDB | Same project contract as MySQL with an implementation-distinct shared instance | Internal host and port plus project schema through managed `DB_*` values | Root-authenticated probe and idempotent schema/user/grant provisioning | Delete the exact project user; retain schema and data |
| MongoDB | Stable database-scoped user/password; separate administrator secret | Internal MongoDB URI naming the project database | Administrator-authenticated ping and idempotent scoped-user provisioning | Delete the exact database user; retain database and collections |
| SQL Server | Stable login/user/password and project database; separate administrator secret | Internal host and port plus project database through managed `DB_*` values | Administrator-authenticated query and idempotent login/database/user provisioning | Disable the exact project login; retain database and user |
| Redis | Stable ACL user/password and enforced project key prefix | Internal Redis host, port, username, password, and prefix | Administrator-authenticated ping plus ACL publication/reload verification | Delete the exact ACL user; retain prefixed keys |
| Valkey | Same isolated ACL contract as Redis with an implementation-distinct shared instance | Internal Valkey host, port, username, password, and prefix | Administrator-authenticated ping plus ACL publication/reload verification | Delete the exact ACL user; retain prefixed keys |
| MinIO | Stable access key/secret scoped by bucket policy; separate root credential | Internal S3 endpoint, bucket, region, access key, and secret | Root-authenticated health plus idempotent bucket, identity, and policy provisioning | Disable the exact project identity; retain bucket and objects |
| RabbitMQ | Stable project user/password, dedicated vhost, and exact permissions | Internal AMQP host, port, vhost, username, and password | Administrator-authenticated diagnostics plus atomic definitions publication | Delete the exact project user; retain vhost, topology, and messages |
| Mailpit | Stable SMTP username/password used as the project attribution identity | Shared internal SMTP endpoint and deterministic project UI route | HTTP readiness plus authenticated SMTP configuration snapshot | Disable the project credential and remove it from active authentication; retain shared service state |
| Gotenberg | No project secret | Shared internal HTTP endpoint | HTTP health endpoint from the exact shared container | Remove the logical reference; stop the unreferenced stateless container |
| Dragonfly | User-declared service configuration; no generated tenant credential | Project-private network endpoint; no host port or gateway route | Engine state only; sharing remains unsupported | Stop/remove the disposable container; retain its project volume |
| Memcached | No credential; any namespace remains an application convention | Generated project-private host and port; no host port or gateway route | Engine state only | Stop/remove the disposable container; no persistent data is retained |
| Garage | User-declared service configuration; no generated tenant credential | Project-private network endpoint; no host port or gateway route | Engine state only; policy isolation remains unproven | Stop/remove the disposable container; retain its project volume |
| RustFS | User-declared service configuration; no generated tenant credential | Project-private network endpoint; no host port or gateway route | Engine state only until the external admin lifecycle is proven | Stop/remove the disposable container; retain its project volume |
| LocalStack | User-declared project service settings | Project-private network endpoint; no host port or implicit route | Engine state only | Stop/remove the disposable container; retain its project volume |
| OpenSearch | Stable generated demo administrator password satisfying the image policy | Generated project-private HTTPS endpoint and administrator identity; no host port or implicit route | Engine state only; demo TLS and safe shared security are not claimed | Stop/remove the disposable container, disable its credential, and retain its project volume |
| Elasticsearch | Stable generated `elastic` administrator password | Generated authenticated project-private HTTP endpoint; no host port, implicit route, or per-project CA | Engine state only; safe shared security is not claimed | Stop/remove the disposable container, disable its credential, and retain its project volume |
| Meilisearch | Stable generated master key; project-specific settings remain declarative | Generated project-private HTTP endpoint and key; no host port or implicit route | Engine state only; scoped-key sharing is not claimed | Stop/remove the disposable container, disable its credential, and retain its project volume |
| Typesense | Stable generated bootstrap API key; project-specific settings remain declarative | Project-private host, port, and protocol injected into the application; no host port or implicit route | Engine state only; scoped-key sharing is not claimed | Stop/remove the disposable container, disable its credential, and retain its project volume |
| Application/FrankenPHP | Daemon-managed service values merged with declared project environment; no secret in labels or image layers | Deterministic HTTPS route to internal plain HTTP; no project host port | Engine-observed application container health | Remove the disposable runtime; project source and infrastructure retention remain independent |
| Reverb | Project application environment and any framework-managed credentials | Deterministic HTTPS/WebSocket route to internal plain HTTP | Engine-observed application container health | Remove the disposable runtime; retained infrastructure is unaffected |
| Horizon/workers | Inherit the application image and daemon-managed project environment | No public endpoint; supervised process container on the private network | Engine process state tied to the exact application revision | Stop/remove the disposable process container |
| Scheduler | Inherit the application image and daemon-managed project environment | No endpoint; daemon-timed Engine exec in the application container | Exact command completion and daemon scheduling state | Remove future schedules when the project leaves desired state |
| Dusk/Selenium | Operation-scoped browser session; no durable project secret | Private Grid endpoint available only to the test operation | Official Selenium Grid readiness probe | Always stop/remove the operation container, including interrupted-session recovery |
| Soketi | Deterministic app ID/key plus one stable random, redaction-safe project secret | Deterministic HTTPS/WebSocket route and internal port 6001; no host port | Pinned image Node probe against `/ready` | Stop/remove the disposable service and disable its retained credential |
| MailHog | Unsupported | None | None | None; use Mailpit |

Every strategy advertised as shared requires authenticated readiness, not
merely a running container. An `Engine state only` row therefore remains
dedicated and is not evidence for future sharing. Provisioning a logical
resource must not restart a compatible shared instance. Changing an immutable
compatibility field creates a new instance and explicit migration, never an
in-place reinterpretation.

Dedicated project services use a common Engine substrate: exact project and
service ownership, deterministic container naming, immutable image and numeric
major version, Linux platform selection, the private Stackctl network, declared
command and environment, restart supervision, and no host ports. The dedicated
routable strategy adds one deterministic gateway route and generated service
credentials through the same planning boundary. This substrate does not by
itself make a stateful preset
complete; each such preset still requires its documented retained-volume,
authenticated-readiness, backup, restore, and upgrade contracts.

The dedicated volume contract currently mounts one stable
`stackctl-{project}-{service}-data` volume for Dragonfly, Garage, LocalStack,
RustFS, OpenSearch, Elasticsearch, Meilisearch, and Typesense at the preset's
canonical data directory. The volume is persistent, project-owned, and keyed by
the same exact implementation, major version, image digest, and platform
identity as its container. Existing identity drift fails before container
replacement and requires explicit migration. A project backup resolves the
exact live service and named volume again, stops the service when necessary,
streams an Engine archive into immutable checksummed recovery storage, and
restores its prior running state. Restore first records a separate verified
safety recovery point for the current contents, then removes only the exact
owned service and volume, recreates the desired empty target, uploads the
selected archive before start, and requires readiness. Destructive
authorization binds one exact verified recovery point to every volume in the
user-visible installation plan and confirmation token, re-verifies it before
cleanup, and passes only those exact volume names to Engine deletion. An
unlisted Engine-observed volume fails teardown before mutation. Memcached and
Soketi remain volume-free because their current dedicated contracts are
stateless.

Soketi receives one durable, redaction-safe project secret plus deterministic
app ID and key. Stackctl injects the server-side Pusher values into project
runtimes, the public Vite values for browser clients, and the generated Soketi
values into only the dedicated service container. It publishes exactly
`{project}-{service}.stackctl.localhost` through the built-in gateway and never
binds Soketi to a host port. Declared values cannot override generated secrets.

Dusk and Selenium are never steady project services. Each browser-test command
gets a deterministic operation-scoped container using the locked immutable
image, Linux platform, private Stackctl network, official Grid readiness probe,
and 2 GiB shared-memory allocation. It publishes no host port, has no restart
policy, and is stopped and removed after command success or failure. If daemon
shutdown interrupts cleanup, the next serialized reconciliation removes the
owned disposable session before new work begins.

This matrix is enforced by the closed v8 service deployment strategy resolver.
Unknown presets fail, aliases resolve identically, and every "share only after"
entry remains dedicated until its isolation acceptance tests are implemented.

Removing a project disables credentials where safe and orphans logical data. It
never deletes shared volumes or project data automatically. Deleting the last
reference may stop a shared container, but retained data requires explicit
pruning after backup policy is satisfied.

For RabbitMQ, the daemon deletes only the exact ownership-proven disabled user
before it idles an unreferenced broker. The vhost, queues, and messages remain
retained. A stopped retained broker is started when revocation still needs to
converge, and the vhost is never deleted implicitly.

Redis and Valkey use the same shared-access strategy boundary to delete only
the disabled project ACL user. The administrator secret is supplied through the
Engine command environment, never arguments, while the tenant key prefix and
all matching values remain retained.

PostgreSQL applies `NOLOGIN` to the exact ownership-proven disabled project role
through that boundary. It retains the database and role so explicit adoption,
restore, or prune remains possible, and keeps the administrator secret out of
command arguments.

MySQL and MariaDB delete only the exact ownership-proven disabled tenant user.
Their project schema and data remain available for explicit adoption, restore,
or prune, while the root secret is supplied only through the Engine command
environment.

MongoDB deletes only the exact ownership-proven disabled database user. The
database and collections remain available for explicit adoption, restore, or
prune, and the administrator secret is supplied only through the Engine command
environment.

SQL Server disables only the exact ownership-proven project login. It retains
the database, mapped user, permissions, and data for explicit adoption,
restore, or prune, while the administrator secret is supplied only through the
Engine command environment.

MinIO disables only the exact ownership-proven enabled project identity after
parsing its machine-readable user state. The identity, policy attachment,
buckets, and objects remain available for explicit adoption, restore, or
prune. RustFS stays dedicated until its separate IAM lifecycle is proven.
