# stackctl

Stackctl v8 is a clean-slate local development control plane for macOS and
Linux. One per-user daemon reconciles strict `.stackctl.yaml` projects into
Linux containers through the Engine API. It owns shared infrastructure, local
HTTPS routing, credentials, recovery, and lifecycle state without installing
PHP, databases, Caddy, or nginx on the host.

## Start

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

Validate it, install trust, and install the login service for one or more
watched roots:

```bash
stackctl config validate .stackctl.yaml
stackctl daemon trust install
stackctl daemon service install --dir ~/Developer
```

The daemon discovers valid projects automatically. Inspect them with
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
