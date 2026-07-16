# Stackctl v8 Architecture

These documents are the normative architecture baseline for the v8
transformation. They describe the intended end state; an item documented here
is not complete until its milestone evidence and acceptance tests pass.

- [Architecture](architecture.md): current constraints, two-plane target,
  domain boundaries, daemon, IPC, state, and platform ownership.
- [Configuration](configuration.md): YAML schema, deterministic identities,
  collision handling, environment ownership, and project trust.
- [Service strategies](services.md): sharing and isolation decisions for every
  current preset.
- [Operations](operations.md): gateway, TLS, supply chain, retention, migration,
  backup, deletion, and host dependencies.
- [Milestones](milestones.md): independently verifiable implementation phases.
- [Benchmarks](benchmarks.md): resource and recovery measurement protocol.
- [Platform support](platform-support.md): claimed, preview, blocked, and
  unsupported operating-system and architecture combinations.
- [External verification](external-verification.md): the exact boundary between
  local checks, CI-owned acceptance, and physical-host release evidence.
- [Completion audit](completion-audit.md): requirement-level evidence and
  explicit release blockers.
- [Dependency security policy](dependencies.md): Rust advisory, license,
  duplicate-version, and source gates.
- [Threat model](security.md): assets, attackers, controls, and open
  release-blocking security findings.

## Product invariant

Stackctl runs one quiet native control plane and one Linux container workload
plane. It never guesses project identities, installs host development runtimes,
or requires routine user intervention.
