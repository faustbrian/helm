# Installation

## Requirements

- Docker Desktop (or Docker Engine)
- Rust toolchain with Cargo (for install methods below)

## Install From Main Branch (Recommended)

```bash
cargo install --git git@github.com:faustbrian/helm.git --bin helm --branch main --locked
```

## Install From Git Tag

```bash
cargo install --git git@github.com:faustbrian/helm.git --bin helm --tag v1.0.0 --locked
```

## Install From Local Source

From the repository root:

```bash
cargo install --path . --locked
```

## Verify Installation

```bash
helm --version
```

Expected output includes `helm 1.0.0`.

## Upgrade

Re-run the install command for the target ref/tag.

## Shell Completions (Optional)

```bash
helm completions zsh > ~/.zsh/completions/_helm
```
