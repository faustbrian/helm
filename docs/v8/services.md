# V8 Service Sharing Strategies

## Compatibility identity

A shared instance is keyed by engine, major version, immutable image digest,
extension/plugin/module profile, immutable server settings, persistence mode,
authentication/isolation capability, and platform architecture where relevant.
A different key produces another container, not a native fallback. Project
logical resources reconcile independently and idempotently.

## Preset matrix

| Preset | Default scope | Logical isolation | Backup and upgrade boundary | Dedicated when |
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
| MailHog | Dedicated until attribution is proven | Whole project instance | Optional message export | Default; no equivalent authenticated attribution contract is proven |
| RabbitMQ | Shared by major/plugin profile | Vhost, user/password, permissions | Scoped backup and safety-backed topology restore for empty vhosts; non-empty queues fail closed until message backup exists | Plugins, policies, topology, or isolation differs |
| Soketi | Shared only after credential isolation is proven | Project app ID/key/secret | Configuration export | Global settings or isolation differs |

Every strategy also requires authenticated readiness, not merely a running
container. Provisioning a logical resource must not restart a compatible
shared instance. Changing an immutable compatibility field creates a new
instance and explicit migration, never an in-place reinterpretation.

Dedicated project services use a common Engine substrate: exact project and
service ownership, deterministic container naming, immutable image and numeric
major version, Linux platform selection, the private Stackctl network, declared
command and environment, restart supervision, no host ports, and no implicit
gateway route. This substrate does not by itself make a stateful preset
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
unlisted Engine-observed volume fails teardown before mutation. Memcached,
MailHog, and Soketi remain volume-free because their current dedicated
contracts are stateless.

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
