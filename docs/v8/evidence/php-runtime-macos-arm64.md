# PHP Runtime Acceptance: macOS arm64

Recorded on 2026-07-14 from an Apple Silicon host running macOS 27.0
(26A5368g), Docker client 29.6.1, and Docker Desktop Engine 29.6.1.

The Stackctl PHP 8.5 definition was built for the Linux arm64 workload plane:

```text
$ docker buildx build --quiet --platform linux/arm64 --load \
    --tag stackctl-php:8.5-validation images/php/8.5
sha256:ccd37c4f0e4c46cc43a832920d18abb5e393aadbca973749e3170b9e0ffbe3c0

$ docker image inspect stackctl-php:8.5-validation \
    --format 'id={{.Id}} architecture={{.Architecture}} os={{.Os}}'
id=sha256:ccd37c4f0e4c46cc43a832920d18abb5e393aadbca973749e3170b9e0ffbe3c0 architecture=arm64 os=linux
```

An ephemeral container then enabled every selectable module from
`images/php/8.5/extensions.txt` and verified each one with
`extension_loaded()`:

```text
all supported extensions loaded
```

This is local build and module-load evidence. It does not prove GHCR
publication, signing, SBOM/provenance attachment, amd64 behavior, application
boot, or the complete macOS platform acceptance suite.
