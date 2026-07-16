# V8 Platform Support

This matrix is release policy, not an architecture aspiration. A combination
is supported only after the complete acceptance record exists. Compilation and
unit tests are necessary evidence but never sufficient on their own.
The ownership boundary for local, CI, and physical-host checks is defined in
[External verification](external-verification.md).

## Current status

| Host | Architecture | Status | Current automated evidence | Release-blocking evidence |
| --- | --- | --- | --- | --- |
| Ubuntu Linux | x86_64 | Preview | Full format, lint, release build, test, production Engine acceptance, pinned-gateway protocols, isolated-home Laravel, automatic two-database restore, daemon restart, and live Engine restart in CI | Fresh persistent-host install; login/reboot; systemd user service; trust install/rotation/removal; service crash; suspend/wake where available; bind mounts; inotify; browser traffic; uninstall; controlled benchmark |
| Ubuntu Linux | arm64 | Preview | Full test, production Engine acceptance, pinned-gateway protocols, isolated-home Laravel, automatic restore, daemon restart, live Engine restart on `ubuntu-24.04-arm`, and verified native release artifact | Same persistent-host acceptance as Linux x86_64 |
| macOS | x86_64 | Preview | Release build and full test suite on `macos-15-intel` in CI | Fresh Docker Desktop install; login/reboot; launchd; Keychain trust install/rotation/removal; Docker Desktop restart; laptop sleep/wake; bind mounts; FSEvents; gateway traffic; backup/restore; benchmark |
| macOS | arm64 | Preview | Release build and full test suite on `macos-15` in CI, verified native release artifact, and local pinned-gateway protocol, reload, and restart evidence | Same live acceptance as macOS x86_64 |

`Preview` means the implementation is intended to work but must not be marketed
as release-supported. V8 targets macOS and Linux hosts only; other operating
systems are outside the product contract.

The automated evidence above passed together for selected candidate `0ca1db6`
in
[CI run 29493625150](https://github.com/faustbrian/stackctl/actions/runs/29493625150).
All four native release artifacts and their supply-chain evidence passed in
[release run 29493629778](https://github.com/faustbrian/stackctl/actions/runs/29493629778).

## Engine support

| Engine | Status | Reason |
| --- | --- | --- |
| Docker Engine / Docker Desktop | Preview | The v8 daemon uses the typed Docker-compatible Engine API. Live platform recovery and benchmark records are still required. |
| Podman | Unsupported | V8 persists a Docker Engine contract and has no completed socket, behavior-parity, recovery, or platform acceptance record for Podman. |

V8 has no per-invocation `--engine` override or compatibility fallback.

## Evidence record requirements

Every live record must include:

- Stackctl commit and release candidate;
- clean host or VM image and OS build;
- CPU architecture;
- Engine product, version, backend, and resource limits;
- image digests and Linux platforms used;
- exact acceptance command and raw artifacts;
- login, reboot, Engine restart, daemon crash, service crash, and sleep/wake
  results;
- filesystem event, mount, permissions, line-ending, loopback binding, local
  TLS trust, and application file-watching results;
- backup, reversible restore, confirmation, and rollback results;
- benchmark result linked from `docs/v8/benchmarks/`.

The gateway acceptance harness records these source, host, Engine, toolchain,
port, and resource-limit fields alongside every protocol result so uploaded CI
artifacts remain attributable without relying on surrounding job logs.

Failed and skipped checks remain visible. A passing run may add a support claim;
documentation or compilation alone may not.
