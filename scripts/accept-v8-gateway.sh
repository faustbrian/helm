#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

readonly GATEWAY_IMAGE="caddy@sha256:af5fdcd76f2db5e4e974ee92f96ee8c0fc3edb55bd4ba5032547cbf3f65e486d"
readonly HTTP_PORT="${STACKCTL_GATEWAY_ACCEPTANCE_HTTP_PORT:-18080}"
readonly HTTPS_PORT="${STACKCTL_GATEWAY_ACCEPTANCE_HTTPS_PORT:-18443}"
readonly RECORD="${1:-target/gateway-acceptance-record.txt}"
readonly ID="$$"
readonly NETWORK="stackctl-gateway-acceptance-${ID}"
readonly UPSTREAM="stackctl-gateway-upstream-${ID}"
readonly GATEWAY="stackctl-gateway-acceptance-${ID}"
ROOT="$(mktemp -d)"
readonly ROOT
readonly CERTIFICATES="${ROOT}/tls"
readonly HOST_FIXTURE="${ROOT}/gateway-fixture-host"
readonly LINUX_FIXTURE="${ROOT}/gateway-fixture-linux"

cleanup() {
    if [[ -n "${continuity_pid:-}" ]]; then
        kill "${continuity_pid}" >/dev/null 2>&1 || true
        wait "${continuity_pid}" >/dev/null 2>&1 || true
    fi
    docker rm --force "${GATEWAY}" >/dev/null 2>&1 || true
    docker rm --force "${UPSTREAM}" >/dev/null 2>&1 || true
    docker network rm "${NETWORK}" >/dev/null 2>&1 || true
    rm -rf "${ROOT}"
}

on_exit() {
    status=$?
    trap - EXIT INT TERM
    if ((status != 0)); then
        docker logs "${GATEWAY}" >&2 2>/dev/null || true
        docker logs "${UPSTREAM}" >&2 2>/dev/null || true
        if [[ "${STACKCTL_GATEWAY_ACCEPTANCE_KEEP_FAILED:-0}" == "1" ]]; then
            printf 'preserving failed gateway acceptance resources under id %s at %s\n' \
                "${ID}" "${ROOT}" >&2
            exit "${status}"
        fi
    fi
    cleanup
    exit "${status}"
}
trap on_exit EXIT INT TERM

for command in docker go; do
    if ! command -v "${command}" >/dev/null 2>&1; then
        printf 'gateway acceptance requires %s\n' "${command}" >&2
        exit 1
    fi
done

mkdir -p "$(dirname "${RECORD}")"
go build -trimpath -o "${HOST_FIXTURE}" ./acceptance/gateway/main.go

case "$(docker info --format '{{.Architecture}}')" in
    amd64|x86_64) engine_architecture="amd64" ;;
    arm64|aarch64) engine_architecture="arm64" ;;
    *)
        printf 'unsupported Engine architecture for gateway acceptance\n' >&2
        exit 1
        ;;
esac
CGO_ENABLED=0 GOOS=linux GOARCH="${engine_architecture}" \
    go build -trimpath -o "${LINUX_FIXTURE}" ./acceptance/gateway/main.go
"${HOST_FIXTURE}" cert "${CERTIFICATES}"

cat >"${ROOT}/config.json" <<'JSON'
{
  "admin": {
    "listen": "localhost:2019",
    "config": {"persist": false}
  },
  "apps": {
    "tls": {
      "certificates": {
        "load_files": [{
          "certificate": "/etc/stackctl/tls/cert.pem",
          "key": "/etc/stackctl/tls/key.pem",
          "format": "pem"
        }]
      }
    },
    "http": {
      "grace_period": "30s",
      "servers": {
        "stackctl_http": {
          "listen": [":80"],
          "protocols": ["h1"],
          "routes": [{
            "match": [{"host": ["bill-app.stackctl.localhost"]}],
            "handle": [{
              "handler": "static_response",
              "status_code": 308,
              "headers": {"Location": ["https://{http.request.host}{http.request.uri}"]}
            }],
            "terminal": true
          }]
        },
        "stackctl_https": {
          "listen": [":443"],
          "protocols": ["h1", "h2"],
          "routes": [{
            "match": [{"host": ["bill-app.stackctl.localhost"]}],
            "handle": [{
              "handler": "headers",
              "response": {"set": {"X-Stackctl-Revision": ["acceptance-initial"]}}
            }, {
              "handler": "reverse_proxy",
              "upstreams": [{"dial": "upstream:8080"}],
              "stream_close_delay": "5m"
            }],
            "terminal": true
          }],
          "tls_connection_policies": [{}]
        }
      }
    }
  }
}
JSON

docker pull "${GATEWAY_IMAGE}" >/dev/null
docker network create "${NETWORK}" >/dev/null
docker run --detach \
    --name "${UPSTREAM}" \
    --network "${NETWORK}" \
    --network-alias upstream \
    --read-only \
    --mount "type=bind,source=${LINUX_FIXTURE},target=/gateway-fixture,readonly" \
    --entrypoint /gateway-fixture \
    "${GATEWAY_IMAGE}" serve >/dev/null

start_gateway() {
    docker run --detach \
        --name "${GATEWAY}" \
        --network "${NETWORK}" \
        --read-only \
        --publish "127.0.0.1:${HTTP_PORT}:80" \
        --publish "127.0.0.1:${HTTPS_PORT}:443" \
        --mount "type=bind,source=${ROOT}/config.json,target=/etc/stackctl/config.json,readonly" \
        --mount "type=bind,source=${CERTIFICATES},target=/etc/stackctl/tls,readonly" \
        --tmpfs /config:rw,noexec,nosuid,size=16m \
        --tmpfs /data:rw,noexec,nosuid,size=16m \
        --tmpfs /tmp:rw,noexec,nosuid,size=16m \
        --entrypoint caddy \
        "${GATEWAY_IMAGE}" run --config /etc/stackctl/config.json >/dev/null
}

start_gateway
"${HOST_FIXTURE}" wait "127.0.0.1:${HTTPS_PORT}"
first_probe="$("${HOST_FIXTURE}" probe "${HTTP_PORT}" "${HTTPS_PORT}" "${CERTIFICATES}/ca.pem" acceptance-initial)"
sed 's/acceptance-initial/acceptance-reloaded/g' \
    "${ROOT}/config.json" >"${ROOT}/reloaded.json"
"${HOST_FIXTURE}" continuity "${HTTPS_PORT}" "${CERTIFICATES}/ca.pem" \
    "${ROOT}/continuity-ready" "${ROOT}/continuity-release" \
    >"${ROOT}/continuity-result.json" &
continuity_pid=$!
"${HOST_FIXTURE}" wait-file "${ROOT}/continuity-ready"
docker exec --interactive "${GATEWAY}" \
    caddy reload --config - --address localhost:2019 <"${ROOT}/reloaded.json"
touch "${ROOT}/continuity-release"
wait "${continuity_pid}"
continuity_probe="$(<"${ROOT}/continuity-result.json")"
reload_probe="$("${HOST_FIXTURE}" probe "${HTTP_PORT}" "${HTTPS_PORT}" "${CERTIFICATES}/ca.pem" acceptance-reloaded)"
docker rm --force "${GATEWAY}" >/dev/null
mv "${ROOT}/reloaded.json" "${ROOT}/config.json"
start_gateway
"${HOST_FIXTURE}" wait "127.0.0.1:${HTTPS_PORT}"
restart_probe="$("${HOST_FIXTURE}" probe "${HTTP_PORT}" "${HTTPS_PORT}" "${CERTIFICATES}/ca.pem" acceptance-reloaded)"

{
    printf 'recorded_at_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'source_revision=%s\n' "$(git rev-parse HEAD)"
    printf 'acceptance_command=%q %q\n' \
        "./scripts/accept-v8-gateway.sh" "${RECORD}"
    printf 'host_os=%s\n' "$(uname -s)"
    printf 'host_release=%s\n' "$(uname -r)"
    printf 'host_architecture=%s\n' "$(uname -m)"
    printf 'go_version=%s\n' "$(go version)"
    printf 'gateway_image=%s\n' "${GATEWAY_IMAGE}"
    printf 'engine_product=%s\n' \
        "$(docker version --format '{{.Server.Platform.Name}}')"
    printf 'engine_version=%s\n' "$(docker version --format '{{.Server.Version}}')"
    printf 'engine_operating_system=%s\n' \
        "$(docker info --format '{{.OperatingSystem}}')"
    printf 'engine_kernel_version=%s\n' \
        "$(docker info --format '{{.KernelVersion}}')"
    printf 'engine_architecture=%s\n' "${engine_architecture}"
    printf 'engine_storage_driver=%s\n' \
        "$(docker info --format '{{.Driver}}')"
    printf 'engine_cpu_count=%s\n' "$(docker info --format '{{.NCPU}}')"
    printf 'engine_memory_bytes=%s\n' \
        "$(docker info --format '{{.MemTotal}}')"
    printf 'published_http_port=%s\n' "${HTTP_PORT}"
    printf 'published_https_port=%s\n' "${HTTPS_PORT}"
    printf 'first_probe=%s\n' "${first_probe}"
    printf 'reload_probe=%s\n' "${reload_probe}"
    printf 'continuity_probe=%s\n' "${continuity_probe}"
    printf 'graceful_reload=true\n'
    printf 'restart_probe=%s\n' "${restart_probe}"
    printf 'restart=true\n'
} | tee "${RECORD}"
