# Dependency security policy

Stackctl treats `Cargo.lock` as release input. CI runs the same supply-chain
gate available locally through:

```sh
just audit-v8-supply-chain
```

The command requires `cargo-audit` 0.22.2 and `cargo-deny` 0.20.2. CI installs
those exact versions with their published lock files before running the gate.
The policy checks the four supported Rust targets: macOS and GNU/Linux on
x86_64 and arm64. Windows-only and unsupported target dependencies do not
affect Stackctl's supported-target duplicate inventory.

The gate fails on:

- a RustSec vulnerability, unmaintained advisory, or yanked dependency;
- an unreviewed duplicate crate version;
- a license outside the explicit permissive allowlist;
- a wildcard dependency requirement;
- a dependency from an unknown registry or Git source; or
- a policy warning, including a stale exception.

There is one reviewed duplicate exception. `ring` 0.17 currently requires
`getrandom` 0.2 while Stackctl and `bcrypt` use `getrandom` 0.4. The exception
names the exact older version and remains visible in `deny.toml`; an unused or
changed exception fails the gate.

No advisory is ignored. A future advisory exception must identify the exact
advisory, document why the affected behavior is unreachable or mitigated, name
an owner, and include an expiry date. It must never be added solely to restore
green CI.

## Direct dependency inventory

| Area | Crates | Purpose |
| --- | --- | --- |
| CLI and diagnostics | `anyhow`, `clap`, `clap_complete`, `colored`, `tracing`, `tracing-subscriber` | Argument parsing, typed command failure context, completion output, and bounded diagnostics |
| Engine and async I/O | `bollard`, `futures-util`, `tokio` | Typed Docker-compatible Engine API, streams, local IPC, and daemon work |
| Configuration and state | `rusqlite`, `serde`, `serde_json`, `serde_yaml_ng` | SQLite state plus strict YAML and bounded JSON protocol data |
| Filesystem and lifecycle | `notify`, `rustix`, `signal-hook` | Watched-root events, Unix process identity, and shutdown signals |
| Cryptography and identity | `base64`, `bcrypt`, `getrandom`, `hex`, `rcgen`, `sha2`, `x509-parser` | Credentials, random identities, hashing, and local certificate generation and inspection |
| Archives and time | `tar`, `time`, `zip` | Verified backup formats, certificate time checks, and declared dump inputs |

Runtime and service container image identities are a separate supply-chain
surface and are inventoried in [services.md](services.md). Resolved runtime
state must use immutable digests even when users select a documented version
tag in YAML.
