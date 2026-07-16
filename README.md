# stackctl

Stackctl v8 is a clean-slate local development control plane for macOS and
Linux. One per-user daemon reconciles strict `.stackctl.yaml` projects into
Linux containers through the Engine API. It owns shared infrastructure, local
HTTPS routing, credentials, recovery, and lifecycle state without installing
PHP, databases, Caddy, or nginx on the host.

## Quick start

Start Docker Desktop on macOS or Docker Engine on Linux, then install the
current checkout:

```bash
cargo install --path . --locked
stackctl --version
```

Create a project configuration:

```yaml
schema_version: 8
project: bill
services:
  app:
    preset: laravel
  db:
    preset: postgres
    version: "18"
```

Validate it, publish the project's immutable image lock, and perform the
one-time setup:

```bash
stackctl config validate .stackctl.yaml
stackctl lock images
stackctl setup --dir ~/Developer
stackctl daemon reconcile
stackctl status
stackctl open
```

Before setup, `stackctl lock images` uses the running Docker-compatible Engine
only to resolve mutable references into immutable digests. Setup then validates
the roots and `.localhost` resolution before host mutation, installs the
singleton CA trust, and starts the login service transactionally. The daemon
discovers the already-runnable project automatically. Inspect it with
`stackctl status`, `stackctl logs`, and `stackctl url`.

V8 does not upgrade, migrate, adopt, or execute pre-v8 project configuration.
Use a fresh installation and new `.stackctl.yaml` files.

## Documentation

- [V8 design and operations](docs/v8/README.md)
- [Installation](INSTALLATION.md)
- [Command reference](USAGE.md)
- [Release changes](CHANGELOG.md)

## License

Stackctl is licensed under MIT. See [LICENSE.md](LICENSE.md).
