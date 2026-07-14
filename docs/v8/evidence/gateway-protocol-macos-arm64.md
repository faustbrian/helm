# Gateway Protocol Acceptance: macOS arm64

Recorded on 2026-07-14 from an Apple Silicon host running macOS 27.0
(26A5368g), Docker client 29.6.1, Docker Desktop Engine 29.6.1, and Go 1.26.5.

The repository acceptance harness ran the pinned Linux arm64 gateway image,
an isolated Linux upstream, and a Stackctl-owned wildcard certificate:

```text
$ just accept-v8-gateway
recorded_at_utc=2026-07-14T19:55:26Z
gateway_image=caddy@sha256:af5fdcd76f2db5e4e974ee92f96ee8c0fc3edb55bd4ba5032547cbf3f65e486d
engine_version=29.6.1
engine_architecture=arm64
first_probe={"http1":true,"http2":true,"redirect":true,"websocket":true,"streaming":true,"large_body":true,"config_revision":"acceptance-initial"}
reload_probe={"http1":true,"http2":true,"redirect":true,"websocket":true,"streaming":true,"large_body":true,"config_revision":"acceptance-reloaded"}
continuity_probe={"streaming":true,"websocket":true}
graceful_reload=true
restart_probe={"http1":true,"http2":true,"redirect":true,"websocket":true,"streaming":true,"large_body":true,"config_revision":"acceptance-reloaded"}
restart=true
```

The harness sent the complete replacement document through a container exec to
the gateway's private admin endpoint, using the same stdin command contract as
the production ownership-checked Engine API. The changed response revision
proves the replacement became active before the gateway was destroyed and
recreated from the replacement bootstrap. A streaming response and upgraded
WebSocket were opened before the reload and successfully exchanged remaining
data afterward, proving active connections survived the replacement.

This proves the pinned gateway's protocol, atomic-reload, and restart contract
on Docker Desktop's Linux arm64 workload plane. It does not prove host CA trust,
daemon or Engine restart recovery, traffic on ports 80/443, Linux-host behavior,
or the complete macOS platform acceptance suite.
