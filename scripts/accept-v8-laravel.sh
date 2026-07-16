#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if (( $# != 2 )); then
  printf 'usage: %s <stackctl-binary> <new-output-directory>\n' "$0" >&2
  exit 64
fi

readonly STACKCTL_BINARY_INPUT="$1"
if [[ ! -x "$STACKCTL_BINARY_INPUT" ]]; then
  printf 'Stackctl binary is not executable: %s\n' "$STACKCTL_BINARY_INPUT" >&2
  exit 66
fi
STACKCTL_BINARY="$(cd "$(dirname "$STACKCTL_BINARY_INPUT")" && pwd)/$(basename "$STACKCTL_BINARY_INPUT")"
readonly STACKCTL_BINARY
readonly OUTPUT_DIRECTORY_INPUT="$2"
readonly LARAVEL_VERSION="13.8.0"
readonly LARAVEL_COMMIT="e196bfdfc96903f2e10219749fcbca7c0aefe99f"
readonly PHP_IMAGE="dunglas/frankenphp@sha256:99a8142b702d9387682b3c845ae21f41ac611961aa20237357e73c93acee6ad2"
readonly COMPOSER_IMAGE="composer@sha256:5946476338742b200bb9ff88f8be56275ddae4b3949c72305cb0dbf10cfcb760"

if [[ -e "$OUTPUT_DIRECTORY_INPUT" ]]; then
  printf 'acceptance output already exists: %s\n' "$OUTPUT_DIRECTORY_INPUT" >&2
  exit 73
fi
if [[ "$(uname -s)" != "Linux" ]]; then
  printf 'Laravel clean-room acceptance requires a native Linux Engine host\n' >&2
  exit 69
fi
for executable in curl docker git jq sha256sum; do
  if ! command -v "$executable" >/dev/null; then
    printf 'required acceptance tool is unavailable: %s\n' "$executable" >&2
    exit 69
  fi
done

mkdir -p "$OUTPUT_DIRECTORY_INPUT"
OUTPUT_DIRECTORY="$(cd "$OUTPUT_DIRECTORY_INPUT" && pwd)"
readonly OUTPUT_DIRECTORY
ACCEPTANCE_ROOT="$(mktemp -d)"
readonly ACCEPTANCE_ROOT
readonly ACCEPTANCE_HOME="$ACCEPTANCE_ROOT/home"
readonly PROJECT_DIRECTORY="$ACCEPTANCE_ROOT/laravel-acceptance"
readonly METADATA="$OUTPUT_DIRECTORY/metadata.txt"
readonly DAEMON_LOG="$OUTPUT_DIRECTORY/daemon.log"
readonly STATUS_ATTEMPTS="$OUTPUT_DIRECTORY/status-attempts.txt"
daemon_pid=''
installation_id=''
application_container_id=''
cleanup_authorized='false'

start_daemon() {
  HOME="$ACCEPTANCE_HOME" STACKCTL_ENGINE_SOCKET="${STACKCTL_ENGINE_SOCKET:-/var/run/docker.sock}" \
    "$STACKCTL_BINARY" daemon watch --interval 1 --dir "$PROJECT_DIRECTORY" \
    >> "$DAEMON_LOG" 2>&1 &
  daemon_pid=$!
}

stop_daemon() {
  if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" 2>/dev/null; then
    kill -TERM "$daemon_pid"
    wait "$daemon_pid" || true
  fi
  daemon_pid=''
}

discover_installation_id() {
  local gateway_id
  gateway_id="$(docker ps -aq \
    --filter label=dev.stackctl.kind=project_application \
    --filter label=dev.stackctl.project=acceptance | sed -n '1p')"
  if [[ -z "$gateway_id" ]]; then
    gateway_id="$(docker ps -aq \
      --filter label=dev.stackctl.kind=gateway | sed -n '1p')"
  fi
  if [[ -z "$gateway_id" ]]; then
    gateway_id="$(docker network ls -q \
      --filter label=dev.stackctl.managed=true | sed -n '1p')"
    if [[ -n "$gateway_id" ]]; then
      installation_id="$(docker network inspect \
        --format '{{ index .Labels "dev.stackctl.installation" }}' \
        "$gateway_id")"
      return
    fi
  fi
  if [[ -n "$gateway_id" ]]; then
    installation_id="$(docker inspect \
      --format '{{ index .Config.Labels "dev.stackctl.installation" }}' \
      "$gateway_id")"
  fi
}

remove_owned_engine_resources() {
  if [[ -z "$installation_id" ]]; then
    discover_installation_id
  fi
  if [[ -z "$installation_id" ]]; then
    printf 'No Stackctl installation identity was observed during cleanup\n'
    return 0
  fi

  local resource
  while IFS= read -r resource; do
    [[ -z "$resource" ]] || docker rm -f "$resource"
  done < <(docker ps -aq --filter "label=dev.stackctl.installation=$installation_id")
  while IFS= read -r resource; do
    [[ -z "$resource" ]] || docker image rm -f "$resource"
  done < <(docker image ls -q \
    --filter "label=dev.stackctl.installation=$installation_id" | sort -u)
  while IFS= read -r resource; do
    [[ -z "$resource" ]] || docker volume rm -f "$resource"
  done < <(docker volume ls -q --filter "label=dev.stackctl.installation=$installation_id")
  while IFS= read -r resource; do
    [[ -z "$resource" ]] || docker network rm "$resource"
  done < <(docker network ls -q --filter "label=dev.stackctl.installation=$installation_id")

  local remaining
  remaining="$(docker ps -aq \
    --filter "label=dev.stackctl.installation=$installation_id")" || return
  [[ -z "$remaining" ]] || return 1
  remaining="$(docker image ls -q \
    --filter "label=dev.stackctl.installation=$installation_id")" || return
  [[ -z "$remaining" ]] || return 1
  remaining="$(docker volume ls -q \
    --filter "label=dev.stackctl.installation=$installation_id")" || return
  [[ -z "$remaining" ]] || return 1
  remaining="$(docker network ls -q \
    --filter "label=dev.stackctl.installation=$installation_id")" || return
  [[ -z "$remaining" ]] || return 1
}

finish() {
  local result=$?
  trap - EXIT
  set +e
  stop_daemon
  local cleanup_result=0
  if [[ "$cleanup_authorized" == 'true' ]]; then
    {
      remove_owned_engine_resources
    } > "$OUTPUT_DIRECTORY/cleanup.txt" 2>&1
    cleanup_result=$?
  else
    printf 'Cleanup was not authorized because the clean-Engine preflight did not pass\n' \
      > "$OUTPUT_DIRECTORY/cleanup.txt"
  fi
  rm -rf "$ACCEPTANCE_ROOT"
  if (( result == 0 && cleanup_result != 0 )); then
    result=$cleanup_result
  fi
  if (( result == 0 )); then
    printf 'completed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$METADATA"
  else
    printf 'failed_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$METADATA"
  fi
  exit "$result"
}
trap finish EXIT

mkdir -p "$ACCEPTANCE_HOME" "$PROJECT_DIRECTORY"
chmod 700 "$ACCEPTANCE_HOME"
if [[ -n "$(docker ps -aq --filter label=dev.stackctl.managed=true)" ]] \
  || [[ -n "$(docker image ls -q --filter label=dev.stackctl.managed=true)" ]] \
  || [[ -n "$(docker volume ls -q --filter label=dev.stackctl.managed=true)" ]] \
  || [[ -n "$(docker network ls -q --filter label=dev.stackctl.managed=true)" ]]; then
  printf 'Laravel clean-room acceptance requires an Engine without existing Stackctl resources\n' >&2
  exit 1
fi
cleanup_authorized='true'
{
  printf 'started_at_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'stackctl_revision=%s\n' "$(git rev-parse HEAD)"
  printf 'stackctl_binary=%s\n' "$STACKCTL_BINARY"
  printf 'host=%s\n' "$(uname -a)"
  printf 'engine=%s\n' "$(docker version --format '{{.Server.Version}}')"
  printf 'laravel_version=%s\n' "$LARAVEL_VERSION"
  printf 'laravel_commit=%s\n' "$LARAVEL_COMMIT"
  printf 'php_image=%s\n' "$PHP_IMAGE"
  printf 'composer_image=%s\n' "$COMPOSER_IMAGE"
} > "$METADATA"

git -C "$PROJECT_DIRECTORY" init --quiet
git -C "$PROJECT_DIRECTORY" remote add origin https://github.com/laravel/laravel.git
git -C "$PROJECT_DIRECTORY" fetch --quiet --depth 1 origin "$LARAVEL_COMMIT"
git -C "$PROJECT_DIRECTORY" checkout --quiet --detach FETCH_HEAD
mkdir -p "$PROJECT_DIRECTORY/database/dumps"
docker run --rm \
  --user "$(id -u):$(id -g)" \
  --env COMPOSER_HOME=/tmp/composer \
  --volume "$PROJECT_DIRECTORY:/app" \
  --workdir /app \
  "$COMPOSER_IMAGE" \
  install --no-interaction --prefer-dist \
  > "$OUTPUT_DIRECTORY/fixture.txt" 2>&1
cp "$PROJECT_DIRECTORY/.env.example" "$PROJECT_DIRECTORY/.env"
touch "$PROJECT_DIRECTORY/database/database.sqlite"
docker run --rm \
  --user "$(id -u):$(id -g)" \
  --volume "$PROJECT_DIRECTORY:/app" \
  --workdir /app \
  --entrypoint php \
  "$COMPOSER_IMAGE" artisan key:generate --force \
  >> "$OUTPUT_DIRECTORY/fixture.txt" 2>&1
docker run --rm \
  --user "$(id -u):$(id -g)" \
  --volume "$PROJECT_DIRECTORY:/app" \
  --workdir /app \
  --entrypoint php \
  "$COMPOSER_IMAGE" artisan migrate --force \
  >> "$OUTPUT_DIRECTORY/fixture.txt" 2>&1
printf 'composer_lock_sha256=%s\n' \
  "$(sha256sum "$PROJECT_DIRECTORY/composer.lock" | cut -d ' ' -f 1)" \
  >> "$METADATA"

printf '%s\n' \
  'CREATE TABLE workflow_shipit_probe (' \
  '  id INTEGER PRIMARY KEY,' \
  '  marker VARCHAR(64) NOT NULL' \
  ');' \
  "INSERT INTO workflow_shipit_probe (id, marker) VALUES (1, 'shipit-restored');" \
  > "$PROJECT_DIRECTORY/database/dumps/shipit.sql"
printf '%s\n' \
  'CREATE TABLE workflow_billing_probe (' \
  '  id INTEGER PRIMARY KEY,' \
  '  marker VARCHAR(64) NOT NULL' \
  ');' \
  "INSERT INTO workflow_billing_probe (id, marker) VALUES (1, 'billing-restored');" \
  > "$PROJECT_DIRECTORY/database/dumps/billing.sql"
# PHP variables in this generated fixture must remain literal.
# shellcheck disable=SC2016
printf '%s\n' \
  '<?php' \
  '' \
  '$expectedShipit = $argv[1] ?? "shipit-restored";' \
  '$connect = static function (string $prefix): PDO {' \
  '    $value = static fn (string $key): string => (string) getenv($prefix.$key);' \
  '    return new PDO(' \
  '        sprintf("mysql:host=%s;port=%s;dbname=%s", $value("HOST"), $value("PORT"), $value("DATABASE")),' \
  '        $value("USERNAME"),' \
  '        $value("PASSWORD"),' \
  '        [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION],' \
  '    );' \
  '};' \
  '$shipit = $connect("DB_");' \
  '$billing = $connect("DB_INVOICING_");' \
  'if ($shipit->query("SELECT marker FROM workflow_shipit_probe WHERE id = 1")->fetchColumn() !== $expectedShipit) {' \
  '    throw new RuntimeException("shipit restore or replay state is incorrect");' \
  '}' \
  'if ($billing->query("SELECT marker FROM workflow_billing_probe WHERE id = 1")->fetchColumn() !== "billing-restored") {' \
  '    throw new RuntimeException("billing restore state is incorrect");' \
  '}' \
  'if (!in_array("migrations", $shipit->query("SHOW TABLES")->fetchAll(PDO::FETCH_COLUMN), true)) {' \
  '    throw new RuntimeException("primary Laravel migration did not run");' \
  '}' \
  'echo "automatic workflow verified\n";' \
  > "$PROJECT_DIRECTORY/database/workflow-verify.php"
# PHP variables in this generated fixture must remain literal.
# shellcheck disable=SC2016
printf '%s\n' \
  '<?php' \
  '' \
  '$pdo = new PDO(' \
  '    sprintf("mysql:host=%s;port=%s;dbname=%s", getenv("DB_HOST"), getenv("DB_PORT"), getenv("DB_DATABASE")),' \
  '    (string) getenv("DB_USERNAME"),' \
  '    (string) getenv("DB_PASSWORD"),' \
  '    [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION],' \
  ');' \
  '$statement = $pdo->prepare("UPDATE workflow_shipit_probe SET marker = ? WHERE id = 1");' \
  '$statement->execute(["workflow-replay-guard"]);' \
  'echo "workflow replay guard written\n";' \
  > "$PROJECT_DIRECTORY/database/workflow-replay-guard.php"

cp "$PROJECT_DIRECTORY/bootstrap/app.php" "$ACCEPTANCE_ROOT/bootstrap-app.php"
printf '%s\n' \
  '<?php' \
  '' \
  "throw new RuntimeException('Stackctl acceptance bootstrap failure');" \
  > "$PROJECT_DIRECTORY/bootstrap/app.php"
printf '%s\n' \
  '<?php' \
  '' \
  'use Illuminate\Support\Facades\Schedule;' \
  '' \
  "Schedule::call(static function (): void {" \
  "    file_put_contents(storage_path('logs/stackctl-scheduler'), 'scheduled');" \
  '})->everyMinute();' \
  > "$PROJECT_DIRECTORY/routes/console.php"
printf '%s\n' \
  'schema_version: 8' \
  'project: acceptance' \
  'services:' \
  '  shipit:' \
  '    preset: mysql' \
  '    version: "8"' \
  '  billing:' \
  '    preset: mysql' \
  '    version: "8"' \
  '    environment_mapping:' \
  '      DB_HOST: DB_INVOICING_HOST' \
  '      DB_PORT: DB_INVOICING_PORT' \
  '      DB_DATABASE: DB_INVOICING_DATABASE' \
  '      DB_USERNAME: DB_INVOICING_USERNAME' \
  '      DB_PASSWORD: DB_INVOICING_PASSWORD' \
  '  app:' \
  '    preset: laravel' \
  '    version: "8.5"' \
  "    image: $PHP_IMAGE" \
  "    composer_image: $COMPOSER_IMAGE" \
  '    php_extensions: [pdo_mysql, pdo_sqlite]' \
  '    depends_on: [shipit, billing]' \
  '  worker:' \
  '    preset: queue-worker' \
  '  scheduler:' \
  '    preset: scheduler' \
  'workflows:' \
  '  sandbox:' \
  '    mode: automatic' \
  '    steps:' \
  '      - type: database_restore' \
  '        service: shipit' \
  '        file: database/dumps/shipit.sql' \
  '        reset: true' \
  '        migrate:' \
  '          service: app' \
  '          connection: mysql' \
  '      - type: database_restore' \
  '        service: billing' \
  '        file: database/dumps/billing.sql' \
  '        reset: true' \
  > "$PROJECT_DIRECTORY/.stackctl.yaml"

HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
  --project-root "$PROJECT_DIRECTORY" config validate \
  > "$OUTPUT_DIRECTORY/config-validation.txt" 2>&1
start_daemon

lock_deadline=$((SECONDS + 120))
while [[ ! -f "$PROJECT_DIRECTORY/.stackctl.lock.yaml" ]] \
  && (( SECONDS < lock_deadline )); do
  if ! kill -0 "$daemon_pid" 2>/dev/null; then
    printf 'Stackctl daemon exited before creating the artifact lock\n' >&2
    exit 1
  fi
  sleep 1
done
if [[ ! -f "$PROJECT_DIRECTORY/.stackctl.lock.yaml" ]]; then
  printf 'Stackctl daemon did not create the artifact lock\n' >&2
  exit 1
fi
HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
  --project-root "$PROJECT_DIRECTORY" lock verify \
  > "$OUTPUT_DIRECTORY/artifact-lock.txt" 2>&1
cp "$PROJECT_DIRECTORY/.stackctl.lock.yaml" \
  "$OUTPUT_DIRECTORY/project.stackctl.lock.yaml"
printf 'artifact_lock_sha256=%s\n' \
  "$(sha256sum "$PROJECT_DIRECTORY/.stackctl.lock.yaml" | cut -d ' ' -f 1)" \
  >> "$METADATA"
resource_deadline=$((SECONDS + 300))
while (( SECONDS < resource_deadline )); do
  if ! kill -0 "$daemon_pid" 2>/dev/null; then
    printf 'Stackctl daemon exited before publishing application status\n' >&2
    exit 1
  fi
  if HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
    --project-root "$PROJECT_DIRECTORY" ps --format json \
    > "$OUTPUT_DIRECTORY/broken-status.json" 2>> "$STATUS_ATTEMPTS" \
    && jq -e '.resources[] | select(.service == "app")' \
      "$OUTPUT_DIRECTORY/broken-status.json" >/dev/null; then
    break
  fi
  sleep 2
done
if ! jq -e '.resources[] | select(.service == "app")' \
  "$OUTPUT_DIRECTORY/broken-status.json" >/dev/null; then
  printf 'Laravel application status was not published before the deadline\n' >&2
  exit 1
fi
application_container_id="$(docker ps -aq \
  --filter label=dev.stackctl.kind=project_application \
  --filter label=dev.stackctl.project=acceptance | sed -n '1p')"
if [[ -z "$application_container_id" ]]; then
  printf 'Laravel application container was not observable through exact labels\n' >&2
  exit 1
fi
printf 'application_container_id=%s\n' "$application_container_id" >> "$METADATA"

unhealthy_deadline=$((SECONDS + 30))
while (( SECONDS < unhealthy_deadline )); do
  HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
    --project-root "$PROJECT_DIRECTORY" ps --format json \
    > "$OUTPUT_DIRECTORY/broken-status.json" 2>> "$STATUS_ATTEMPTS" || true
  if jq -e \
    '.resources[] | select(.service == "app" and .health.state == "healthy")' \
    "$OUTPUT_DIRECTORY/broken-status.json" >/dev/null; then
    printf 'A fatal Laravel bootstrap was incorrectly reported as healthy\n' >&2
    exit 1
  fi
  sleep 2
done

cp "$ACCEPTANCE_ROOT/bootstrap-app.php" "$PROJECT_DIRECTORY/bootstrap/app.php"
ready_deadline=$((SECONDS + 300))
while (( SECONDS < ready_deadline )); do
  if HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" daemon status \
    >> "$STATUS_ATTEMPTS" 2>&1 \
    && HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
      --project-root "$PROJECT_DIRECTORY" ps --format json \
      > "$OUTPUT_DIRECTORY/ready-status.json" 2>> "$STATUS_ATTEMPTS" \
    && jq -e \
      '.resources[] | select(.service == "app" and .kind == "project_application" and .health.state == "healthy")' \
      "$OUTPUT_DIRECTORY/ready-status.json" >/dev/null \
    && jq -e \
      '.resources[] | select(.service == "worker" and .kind == "project_process" and .lifecycle == "active" and .health.state == "running_unverified")' \
      "$OUTPUT_DIRECTORY/ready-status.json" >/dev/null \
    && jq -e \
      '.resources[] | select(.kind == "gateway_route" and .health.state == "healthy")' \
      "$OUTPUT_DIRECTORY/ready-status.json" >/dev/null; then
    break
  fi
  if ! kill -0 "$daemon_pid" 2>/dev/null; then
    printf 'Stackctl daemon exited before Laravel became ready\n' >&2
    exit 1
  fi
  sleep 2
done
HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" daemon status \
  >> "$STATUS_ATTEMPTS" 2>&1
discover_installation_id
printf 'installation_id=%s\n' "$installation_id" >> "$METADATA"

jq -e \
  '.resources[] | select(.service == "app" and .kind == "project_application" and .health.state == "healthy")' \
  "$OUTPUT_DIRECTORY/ready-status.json" >/dev/null
jq -e \
  '.resources[] | select(.service == "worker" and .kind == "project_process" and .lifecycle == "active" and .health.state == "running_unverified")' \
  "$OUTPUT_DIRECTORY/ready-status.json" >/dev/null
jq -e \
  '.resources[] | select(.kind == "gateway_route" and .health.state == "healthy")' \
  "$OUTPUT_DIRECTORY/ready-status.json" >/dev/null
test "$(docker ps -aq \
  --filter label=dev.stackctl.kind=project_application \
  --filter label=dev.stackctl.project=acceptance | sed -n '1p')" = "$application_container_id"

CERTIFICATE_GENERATION="$(< "$ACCEPTANCE_HOME/.stackctl/tls/current")"
readonly CERTIFICATE_GENERATION
readonly CA_CERTIFICATE="$ACCEPTANCE_HOME/.stackctl/tls/$CERTIFICATE_GENERATION/ca.crt"
INITIAL_CA_SHA256="$(sha256sum "$CA_CERTIFICATE" | cut -d ' ' -f 1)"
readonly INITIAL_CA_SHA256
printf 'ca_sha256=%s\n' "$INITIAL_CA_SHA256" >> "$METADATA"
NO_PROXY="${NO_PROXY:-},.stackctl.localhost" \
  no_proxy="${no_proxy:-},.stackctl.localhost" \
  curl --fail --silent --show-error \
  --cacert "$CA_CERTIFICATE" \
  --dump-header "$OUTPUT_DIRECTORY/up-headers.txt" \
  --output "$OUTPUT_DIRECTORY/up-body.txt" \
  https://acceptance-app.stackctl.localhost/up
NO_PROXY="${NO_PROXY:-},.stackctl.localhost" \
  no_proxy="${no_proxy:-},.stackctl.localhost" \
  curl --fail --silent --show-error \
  --cacert "$CA_CERTIFICATE" \
  --output "$OUTPUT_DIRECTORY/application-body.html" \
  https://acceptance-app.stackctl.localhost/
grep -q 'Laravel' "$OUTPUT_DIRECTORY/application-body.html"

HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
  --project-root "$PROJECT_DIRECTORY" exec \
  php artisan about --only=environment --no-ansi \
  > "$OUTPUT_DIRECTORY/artisan.txt" 2>&1
grep -q 'Environment' "$OUTPUT_DIRECTORY/artisan.txt"
HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
  --project-root "$PROJECT_DIRECTORY" logs --all --tail 100 --prefix \
  > "$OUTPUT_DIRECTORY/project-logs.txt" 2>&1

scheduler_deadline=$((SECONDS + 90))
while (( SECONDS < scheduler_deadline )); do
  if [[ -f "$PROJECT_DIRECTORY/storage/logs/stackctl-scheduler" ]]; then
    break
  fi
  sleep 2
done
grep -q 'scheduled' "$PROJECT_DIRECTORY/storage/logs/stackctl-scheduler"

workflow_deadline=$((SECONDS + 300))
while (( SECONDS < workflow_deadline )); do
  if HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
    --project-root "$PROJECT_DIRECTORY" exec \
    php database/workflow-verify.php shipit-restored \
    > "$OUTPUT_DIRECTORY/automatic-workflow.txt" 2>> "$STATUS_ATTEMPTS"; then
    break
  fi
  if ! kill -0 "$daemon_pid" 2>/dev/null; then
    printf 'Stackctl daemon exited before the automatic workflow completed\n' >&2
    exit 1
  fi
  sleep 2
done
grep -q 'automatic workflow verified' "$OUTPUT_DIRECTORY/automatic-workflow.txt"
HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
  --project-root "$PROJECT_DIRECTORY" exec \
  php database/workflow-replay-guard.php \
  > "$OUTPUT_DIRECTORY/workflow-replay-guard.txt" 2>&1
grep -q 'workflow replay guard written' \
  "$OUTPUT_DIRECTORY/workflow-replay-guard.txt"

HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" daemon reconcile \
  > "$OUTPUT_DIRECTORY/reconcile.txt" 2>&1
HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
  --project-root "$PROJECT_DIRECTORY" exec \
  php database/workflow-verify.php workflow-replay-guard \
  > "$OUTPUT_DIRECTORY/reconciled-automatic-workflow.txt" 2>&1
test "$(sha256sum "$CA_CERTIFICATE" | cut -d ' ' -f 1)" = "$INITIAL_CA_SHA256"
stop_daemon
start_daemon

restart_deadline=$((SECONDS + 180))
while (( SECONDS < restart_deadline )); do
  if HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" daemon status \
    >> "$STATUS_ATTEMPTS" 2>&1 \
    && HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
      --project-root "$PROJECT_DIRECTORY" ps --format json \
      > "$OUTPUT_DIRECTORY/restarted-status.json" 2>> "$STATUS_ATTEMPTS" \
    && jq -e \
      '.resources[] | select(.service == "app" and .health.state == "healthy")' \
      "$OUTPUT_DIRECTORY/restarted-status.json" >/dev/null \
    && jq -e \
      '.resources[] | select(.kind == "gateway_route" and .health.state == "healthy")' \
      "$OUTPUT_DIRECTORY/restarted-status.json" >/dev/null; then
    break
  fi
  sleep 2
done
HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" daemon status \
  >> "$STATUS_ATTEMPTS" 2>&1
jq -e \
  '.resources[] | select(.service == "app" and .health.state == "healthy")' \
  "$OUTPUT_DIRECTORY/restarted-status.json" >/dev/null
jq -e \
  '.resources[] | select(.kind == "gateway_route" and .health.state == "healthy")' \
  "$OUTPUT_DIRECTORY/restarted-status.json" >/dev/null
test "$(docker ps -aq \
  --filter label=dev.stackctl.kind=project_application \
  --filter label=dev.stackctl.project=acceptance | sed -n '1p')" = "$application_container_id"
test "$(sha256sum "$CA_CERTIFICATE" | cut -d ' ' -f 1)" = "$INITIAL_CA_SHA256"
HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
  --project-root "$PROJECT_DIRECTORY" exec \
  php database/workflow-verify.php workflow-replay-guard \
  > "$OUTPUT_DIRECTORY/restarted-automatic-workflow.txt" 2>&1
if [[ "${STACKCTL_ACCEPT_ENGINE_RESTART:-false}" == 'true' ]]; then
  if ! command -v systemctl >/dev/null || ! command -v sudo >/dev/null; then
    printf 'Engine restart acceptance requires systemctl and sudo\n' >&2
    exit 69
  fi
  printf 'engine_restart_started_at_utc=%s\n' \
    "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$METADATA"
  sudo -n systemctl restart docker

  engine_deadline=$((SECONDS + 300))
  while (( SECONDS < engine_deadline )); do
    if docker info >/dev/null 2>&1 \
      && HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" daemon status \
        >> "$STATUS_ATTEMPTS" 2>&1 \
      && HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
        --project-root "$PROJECT_DIRECTORY" ps --format json \
        > "$OUTPUT_DIRECTORY/engine-restarted-status.json" \
        2>> "$STATUS_ATTEMPTS" \
      && jq -e \
        '.resources[] | select(.service == "app" and .health.state == "healthy")' \
        "$OUTPUT_DIRECTORY/engine-restarted-status.json" >/dev/null \
      && jq -e \
        '.resources[] | select(.kind == "gateway_route" and .health.state == "healthy")' \
        "$OUTPUT_DIRECTORY/engine-restarted-status.json" >/dev/null; then
      break
    fi
    if ! kill -0 "$daemon_pid" 2>/dev/null; then
      printf 'Stackctl daemon exited during Engine restart recovery\n' >&2
      exit 1
    fi
    sleep 2
  done
  HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" daemon status \
    >> "$STATUS_ATTEMPTS" 2>&1
  jq -e \
    '.resources[] | select(.service == "app" and .health.state == "healthy")' \
    "$OUTPUT_DIRECTORY/engine-restarted-status.json" >/dev/null
  jq -e \
    '.resources[] | select(.kind == "gateway_route" and .health.state == "healthy")' \
    "$OUTPUT_DIRECTORY/engine-restarted-status.json" >/dev/null
  test "$(docker ps -aq \
    --filter label=dev.stackctl.kind=project_application \
    --filter label=dev.stackctl.project=acceptance | sed -n '1p')" = "$application_container_id"
  HOME="$ACCEPTANCE_HOME" "$STACKCTL_BINARY" \
    --project-root "$PROJECT_DIRECTORY" exec \
    php database/workflow-verify.php workflow-replay-guard \
    > "$OUTPUT_DIRECTORY/engine-restarted-automatic-workflow.txt" 2>&1
  printf 'engine_restart_recovered_at_utc=%s\n' \
    "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >> "$METADATA"
else
  printf 'engine_restart=skipped\n' >> "$METADATA"
fi
NO_PROXY="${NO_PROXY:-},.stackctl.localhost" \
  no_proxy="${no_proxy:-},.stackctl.localhost" \
  curl --fail --silent --show-error \
  --cacert "$CA_CERTIFICATE" \
  --output "$OUTPUT_DIRECTORY/restarted-up-body.txt" \
  https://acceptance-app.stackctl.localhost/up

printf 'Laravel clean-room acceptance passed for installation %s\n' "$installation_id"
