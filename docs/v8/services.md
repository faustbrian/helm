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
| MinIO | Shared by compatible profile | Bucket, key/secret, bucket policy | Bucket export; new instance for storage-format boundary | Global config or policy differs |
| RustFS | Shared by compatible profile | Bucket, key/secret, bucket policy | Bucket export; new instance for storage-format boundary | Global config or storage policy differs |
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
| Dusk/Selenium | Ephemeral/dedicated | Whole browser container | Disposable | Always per test/project run |
| Gotenberg | Shared by exact image/config | Stateless HTTP | No service data | Fonts, policy, or config differs |
| MailHog/Mailpit | Shared | Project attribution and deterministic route | Optional message export | Attribution or access isolation differs |
| RabbitMQ | Shared by major/plugin profile | Vhost, user/password, permissions | Definitions and queue backup policy | Plugins, policies, topology, or isolation differs |
| Soketi | Shared only after credential isolation is proven | Project app ID/key/secret | Configuration export | Global settings or isolation differs |

Every strategy also requires authenticated readiness, not merely a running
container. Provisioning a logical resource must not restart a compatible
shared instance. Changing an immutable compatibility field creates a new
instance and explicit migration, never an in-place reinterpretation.

This matrix is enforced by the closed v8 service deployment strategy resolver.
Unknown presets fail, aliases resolve identically, and every "share only after"
entry remains dedicated until its isolation acceptance tests are implemented.

Removing a project disables credentials where safe and orphans logical data. It
never deletes shared volumes or project data automatically. Deleting the last
reference may stop a shared container, but retained data requires explicit
pruning after backup policy is satisfied.
