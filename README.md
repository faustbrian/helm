# helm

`helm` is a low-config local orchestrator for Laravel workspaces.
Instead of maintaining Docker files yourself, you define a small
`.helm.toml` and Helm runs app/data services, wires env values, and coordinates
single-project and multi-project workflows with sensible defaults.

## Quick Start

1. Install Helm:

```bash
cargo install --git git@github.com:faustbrian/helm.git --bin helm --branch main --locked
```

2. Initialize config:

```bash
helm init
```

3. Start services:

```bash
helm up
```

## Documentation

- Installation and upgrade: [`INSTALLATION.md`](INSTALLATION.md)
- Full command and flag reference: [`USAGE.md`](USAGE.md)
- Release changes: [`CHANGELOG.md`](CHANGELOG.md)

For a detailed usage walkthrough, jump to [`USAGE.md`](USAGE.md).

## License

`helm` is licensed under MIT. See [`LICENSE.md`](LICENSE.md).
