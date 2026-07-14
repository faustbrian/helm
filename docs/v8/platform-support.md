# V8 Platform Support

This matrix is release policy, not an architecture aspiration. A combination
is supported only after the complete acceptance record exists. Compilation and
unit tests are necessary evidence but never sufficient on their own.

## Current status

| Host | Architecture | Status | Current automated evidence | Release-blocking evidence |
| --- | --- | --- | --- | --- |
| Ubuntu Linux | x86_64 | Preview | Full format, lint, build, and test suite in CI | Fresh Docker Engine install; login/reboot; Unix socket permissions; systemd user service; trust install/removal; engine restart; service crash; sleep/wake where available; bind mounts; inotify; gateway traffic; backup/restore; benchmark |
| Ubuntu Linux | arm64 | Preview | Full test suite on `ubuntu-24.04-arm` in CI | Same live acceptance as Linux x86_64 plus multi-architecture image verification |
| macOS | x86_64 | Preview | Full test suite on `macos-15-intel` in CI | Fresh Docker Desktop install; login/reboot; launchd; Unix socket permissions; Keychain trust install/removal; Docker Desktop restart; laptop sleep/wake; bind mounts; FSEvents; gateway traffic; backup/restore; benchmark |
| macOS | arm64 | Preview | Full test suite on `macos-15` in CI and local arm64 development checks | Same live acceptance as macOS x86_64 plus arm64 image verification |
| Windows | x86_64 | Unsupported | Trust-store and Engine named-pipe adapter unit coverage only | Per-user named-pipe IPC, daemon ownership/ACLs, login service, filesystem recovery, WSL2/Docker Desktop recovery, trust, gateway, backup/restore, and benchmark acceptance |
| Windows | arm64 | Unsupported | No release evidence | Complete Windows x86_64 blockers plus native arm64 Engine and image acceptance |

`Preview` means the implementation is intended to work but must not be marketed
as release-supported. `Unsupported` means Stackctl must fail clearly instead of
falling back to legacy host tooling or implying partial support.

## Engine support

| Engine | Status | Reason |
| --- | --- | --- |
| Docker Engine / Docker Desktop | Preview | The v8 daemon uses the typed Docker-compatible Engine API. Live platform recovery and benchmark records are still required. |
| Podman | Unsupported | V8 persists a Docker Engine contract and has no completed socket, behavior-parity, recovery, or platform acceptance record for Podman. |

The legacy CLI accepting `--engine podman` is not v8 support. Strict v8 projects
must never enter that fallback path.

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

Failed and skipped checks remain visible. A passing run may add a support claim;
documentation or compilation alone may not.
