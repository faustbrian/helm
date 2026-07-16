# V8 threat model

This threat model is a release gate, not a claim that every listed control is
complete. Any open Critical or High finding in this document or the completion
audit blocks a v8 release recommendation.

## Assets and trust boundaries

Stackctl protects:

- the per-user CA and leaf private keys;
- generated project and shared-service credentials;
- daemon state, recovery evidence, and destructive-operation approvals;
- project source trees and persistent service data;
- deterministic route, resource, and installation ownership identities; and
- the integrity of the Stackctl binary, public images, and derived runtimes.

The native CLI and singleton daemon run with the invoking user's authority.
The Unix socket, `~/.stackctl`, login service, and OS trust store are host trust
boundaries. The Docker-compatible Engine is a separate privileged boundary:
control of its socket is effectively control of the user's workloads and
mounted source. Containers, project YAML, watched files, public images, backup
archives, and IPC requests are untrusted inputs.

## Threat actors

The audit assumes:

- a malicious repository or `.stackctl.yaml` file under a watched root;
- malicious filenames, symlinks, archives, or rapid filesystem changes;
- another local OS user without access to the Stackctl user's account;
- a compromised application, gateway, or shared-service container;
- a compromised or unexpectedly behaving public image;
- a malicious, compromised, unavailable, or partially failing Engine;
- an untrusted local process running as the same user; and
- tampered backup, state, certificate, or generated configuration files.

A same-user process is already able to read that user's project files and talk
to their Engine. Stackctl still limits accidental or confused-deputy damage by
requiring exact ownership metadata, bounded IPC, deterministic identities, and
explicit destructive authorization. It does not claim to sandbox mutually
hostile processes running as the same OS identity.

## Required controls

| Surface | Required control | Current evidence |
| --- | --- | --- |
| Filesystem | Canonical roots, no followed directory symlinks, bounded exact-marker discovery, private state, symlink refusal for sensitive artifacts, atomic publication | Discovery, state, TLS, configuration, and retention tests |
| Configuration | Strict YAML, unknown-field rejection, bounded file and collection sizes, no host shell execution, deterministic collision failure | Configuration and daemon registry tests |
| IPC | User-only directory and `0600` socket, bounded frames, timeouts and queues, typed project/resource ownership checks, secret-free responses | IPC framing, listener, dispatch, queue, and redaction tests |
| Engine ownership | Exact installation/schema/kind/project/resource labels before adoption, mutation, or deletion | Engine reconstruction, reconciliation, and deletion tests |
| Containers | No privileged mode, no new privileges, no Engine socket mount, loopback-only gateway publication, immutable resolved images | Engine request tests plus real private-application, Redis, and PostgreSQL acceptance |
| Shared services | Stable per-project credentials and service-native logical isolation; removal revokes only the selected tenant | Shared-service unit and real-Engine isolation/lifecycle tests |
| TLS | Persistent one-user CA, exact fingerprint trust, private keys, atomic rotation and rollback, wildcard route contract | TLS unit/integration tests; OS and browser behavior remains platform evidence |
| Supply chain | Locked Rust dependencies, RustSec, license/source/duplicate policy, immutable image resolution, pinned CI actions | `scripts/audit-v8-supply-chain.sh`, image policy tests, workflow audit |
| Backup/destruction | Verified hashes, traversal-safe extraction, exact recovery binding, confirmation tokens, retained-data default | Retention, migration, restore, prune, and installation-deletion tests |

## Open release-blocking risks

### SEC-01: cross-project container network reachability — High

The current daemon creates one installation-wide Docker network. Service-native
credentials protect database, cache, queue, and object-store tenants, but one
compromised application container can still initiate traffic to deterministic
application and service names belonging to other projects. V8 must prove a
network topology that prevents unnecessary application-to-application reach
while preserving gateway routing and authenticated shared-service access.

Required evidence: a real-Engine test in which project A cannot connect to
project B's application endpoint, both remain reachable through the gateway,
and both can reach only their authorized shared-service identities.

### SEC-02: project source writes from root containers — High, mitigated

Application and worker plans now select the daemon user's numeric UID:GID, and
that identity participates in replacement-driving desired revisions. Real-Engine
runtime acceptance mounts a real project directory, writes through PHP, checks
host UID/GID ownership, and executes PHP extensions, Composer, npm, Bun, and a
declared hook. Unit coverage proves workers inherit the same identity.

Remaining evidence: the clean-install Laravel acceptance must prove the web
runtime, worker, and scheduler remain healthy with framework runtime/cache paths
writable on both Linux Engine mounts and Docker Desktop filesystem sharing.

## Review rules

New container presets, host commands, IPC operations, archive formats, image
sources, published ports, bind mounts, or destructive actions must update this
model and add evidence at the lowest layer that proves the real boundary.
Mocks may verify planning but cannot close Engine, trust-store, service-manager,
filesystem-sharing, networking, or browser findings.
