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
| Containers | No privileged mode, no new privileges, no Engine socket mount, loopback-only gateway publication, immutable resolved images, and one private network per project | Engine request tests plus real private-application, cross-project network-denial, Redis, and PostgreSQL acceptance |
| Shared services | Stable per-project credentials and service-native logical isolation; removal revokes only the selected tenant | Shared-service unit and real-Engine isolation/lifecycle tests |
| TLS | Persistent one-user CA, exact fingerprint trust, private keys, atomic rotation and rollback, wildcard route contract | TLS unit/integration tests; OS and browser behavior remains platform evidence |
| Supply chain | Locked Rust dependencies, RustSec, license/source/duplicate policy, immutable image resolution, pinned CI actions | `scripts/audit-v8-supply-chain.sh`, image policy tests, workflow audit |
| Backup/destruction | Verified hashes, traversal-safe extraction, exact recovery binding, confirmation tokens, retained-data default | Retention, migration, restore, prune, and installation-deletion tests |

## Audited high-risk findings

### SEC-01: cross-project container network reachability — High, mitigated

The daemon now creates one exact owned network per project. Applications,
workers, scheduled execution, dedicated services, provisioning jobs, and
ephemeral browsers use only their project's network. The global gateway joins
each active project network, and a shared container joins only networks for
projects that currently consume that compatibility instance. Reconciliation
also detaches a shared container from active projects that no longer consume
it, and removes the empty network after a project leaves desired state. Project
network names are deterministic `stackctl-<project>` identities; they are never
guessed, normalized, or collision-repaired.

Unit coverage proves exact project network planning and owned-network
reconciliation beside the installation network. Native Linux CI and the local
real-Engine acceptance test create two HTTP applications, a gateway, and a
shared endpoint; direct project-to-project probes fail, the unauthorized
project cannot reach the shared endpoint, and the gateway reaches both apps.

### SEC-02: project source writes from root containers — High, mitigated

Application and worker plans now select the daemon user's numeric UID:GID, and
that identity participates in replacement-driving desired revisions. Real-Engine
runtime acceptance mounts a real project directory, writes through PHP, checks
host UID/GID ownership, and executes PHP extensions, Composer, npm, Bun, and a
declared hook. Unit coverage proves workers inherit the same identity.

Remaining evidence: the clean-install Laravel acceptance must prove the web
runtime, worker, and scheduler remain healthy with framework runtime/cache paths
writable on both Linux Engine mounts and Docker Desktop filesystem sharing.

### SEC-03: listener-only Laravel readiness — High, mitigated

Application health previously proved only that FrankenPHP accepted a TCP
connection. A PHP fatal or failed Laravel bootstrap could therefore leave the
container and gateway looking healthy while every framework request failed.
Laravel plans now issue a private HTTP request to `/up` and accept only a 2xx
status. The health contract participates in the replacement-driving desired
revision, while non-HTTP application presets retain their protocol-appropriate
listener check. Unit coverage proves the Laravel preset cannot regress to the
listener-only probe; native Engine Laravel acceptance remains a release gate.

## Review rules

New container presets, host commands, IPC operations, archive formats, image
sources, published ports, bind mounts, or destructive actions must update this
model and add evidence at the lowest layer that proves the real boundary.
Mocks may verify planning but cannot close Engine, trust-store, service-manager,
filesystem-sharing, networking, or browser findings.
