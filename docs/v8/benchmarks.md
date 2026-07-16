# V8 Benchmark Protocol

## Compared scenarios

Measure on the same machine and Engine configuration:

1. Engine/VM idle baseline without Stackctl containers.
2. One representative per-project-stack baseline.
3. Forty baseline projects with per-project infrastructure.
4. One v8 project.
5. Forty v8 projects sharing compatible infrastructure.
6. Forty v8 projects split across two PostgreSQL majors and two application
   runtime fingerprints.

The representative project uses an application, PostgreSQL, Valkey, object
storage, mail capture, one worker, and the gateway.

## Measurements

- host and Engine VM memory/CPU;
- per-container memory/CPU;
- persistent and image/cache disk usage;
- object and published-port counts;
- login-to-ready and engine-wake-to-ready time;
- config-write-to-converged latency;
- gateway latency and throughput;
- daemon, gateway, service, engine, and sleep/wake recovery;
- image-layer reuse and build duration.

Record OS, architecture, Engine/version, limits, filesystem sharing mode,
Stackctl revision, image digests, sample interval, host/VM collector identity,
one shared benchmark run ID, and raw output.

## Success thresholds

- Forty compatible projects use one gateway and one instance of each compatible
  infrastructure fingerprint.
- V8 removes at least 90% of duplicate infrastructure containers in the
  representative 40-project scenario.
- V8 idle memory excluding the fixed Engine/VM baseline is at least 60% lower
  than the 40-project per-project-stack baseline.
- Warm reconciliation completes within two seconds after a config event and a
  repeated pass performs no runtime mutation.
- A service or gateway crash returns to ready within 30 seconds.
- Engine restart and laptop wake converge without a user command.
- Default plans publish no project application port; only gateway and explicit
  shared-service access ports are exposed.

Thresholds may be tightened by evidence, not weakened merely to pass.

## Discovery regression benchmark

Project discovery has a separate release-mode synthetic benchmark because it
must remain fast before the Engine is contacted. It creates 1-, 10-, and
40-project roots plus 4,000 ordinary files, excludes fixture creation from the
measurement, warms each fixture three times, and reports 20 samples with p50,
p95, and maximum latency.

The controlled-host p95 budgets are 50 ms, 75 ms, and 125 ms respectively.
Ubuntu hosted CI uses a documented 4x noise allowance while retaining the same
fixture and sample count. Deterministic unit tests separately enforce the
two-level frontier, project-boundary stop, directory count, pruning, symlink,
and ordinary-file retention rules; timing is not hidden inside those tests.

Run and retain the raw record with:

```sh
./scripts/benchmark-v8-discovery.sh \
  docs/v8/benchmarks/<platform>-<revision>-discovery
```

## Evidence

Keep the harness under `scripts/` and result summaries under
`docs/v8/benchmarks/`. Results distinguish the unavoidable macOS Engine VM or
Linux Engine baseline from Stackctl workload consumption.

Build the release candidate, prepare and fully reconcile one scenario, then run:

```sh
STACKCTL_BENCHMARK_ENGINE='Docker Desktop' \
STACKCTL_BENCHMARK_RUN_ID='<host>-<date>-<revision>' \
STACKCTL_BENCHMARK_COLLECTOR='exact collector and version' \
STACKCTL_BENCHMARK_ENGINE_VERSION='exact-version' \
STACKCTL_BENCHMARK_ENGINE_BACKEND='Linux VM identity' \
STACKCTL_BENCHMARK_ENGINE_LIMITS='cpu=...,memory=...' \
STACKCTL_BENCHMARK_FILESYSTEM='exact sharing mode' \
STACKCTL_BENCHMARK_HOST_METRICS_FILE='external-host-metrics.txt' \
./scripts/benchmark-v8.sh v8-forty-compatible \
  docs/v8/benchmarks/<platform>-<revision>-v8-forty-compatible
```

Capture each non-v8 baseline with the same independent host/Engine collector
and include its exact runtime inventory:

```sh
STACKCTL_BENCHMARK_ENGINE='Docker Desktop' \
STACKCTL_BENCHMARK_RUN_ID='<host>-<date>-<revision>' \
STACKCTL_BENCHMARK_COLLECTOR='exact collector and version' \
STACKCTL_BENCHMARK_ENGINE_VERSION='exact-version' \
STACKCTL_BENCHMARK_ENGINE_BACKEND='Linux VM identity' \
STACKCTL_BENCHMARK_ENGINE_LIMITS='cpu=...,memory=...' \
STACKCTL_BENCHMARK_FILESYSTEM='exact sharing mode' \
STACKCTL_BENCHMARK_HOST_METRICS_FILE='external-host-metrics.txt' \
STACKCTL_BENCHMARK_EXTERNAL_INVENTORY_FILE='external-runtime-inventory.txt' \
./scripts/benchmark-v8.sh baseline-forty \
  docs/v8/benchmarks/<platform>-<revision>-baseline-forty
```

Baseline modes are `baseline-engine-idle`, `baseline-one`, and
`baseline-forty`. They never invoke Stackctl or infer zero usage from an empty
owned-resource view. They copy the independently collected metrics and exact
runtime inventory into a new immutable evidence directory with the same Engine,
VM-limit, filesystem, revision, and host metadata used by v8 scenarios.
All six scenarios must use the same `STACKCTL_BENCHMARK_RUN_ID` and exact
`STACKCTL_BENCHMARK_COLLECTOR` value. Empty metrics and baseline inventory files
are rejected before the output directory is created.

The same harness is available through the manually dispatched
`Controlled Benchmark Evidence` GitHub workflow. The runner must be a qualified
dedicated host labeled `self-hosted` and `stackctl-benchmark`; workflow inputs
name the exact scenario and externally populated host-metrics and baseline
inventory paths. Dispatch each of the six scenarios with the same run ID. Each
raw record is published as `benchmark-<run-id>-<scenario>` for ninety days.

`stackctl daemon benchmark` requests each sample from the authoritative daemon.
The harness passes a typed evidence scenario into every request. Before
emitting JSON, Stackctl verifies that the authoritative registered project-ID
set exactly equals both application and worker ownership, the expected number
of application fingerprints, the canonical worker resource identity, the exact
shared-service implementation profile, PostgreSQL 17/18 major split, one
gateway, and the exact total container count.
The compatible and split forty-project fixtures therefore cannot be
substituted for one another, and stale or partially reconciled fixtures cannot
produce accepted evidence.

`stackctl daemon benchmark --evidence-scenario <scenario>` performs the full
topology check and requires the daemon to have completed a successful Engine
reconciliation for the latest validated desired registry, with no pending
filesystem change or Engine rescan. A blocked, failed, or pending convergence
therefore cannot emit evidence. `--expect-projects <count>` remains available
for narrower ad-hoc assertions, while omitting both options retains read-only
inspection. Every sample is written to a same-directory temporary file and
renamed only after validation and serialization succeed, so a failed sample
never appears at its final evidence path.

The daemon discovers current managed containers through the typed Engine API,
proves current-installation ownership, samples normalized CPU, memory, process,
and network counters, and includes only published ports belonging to those exact
containers. Foreign installations are excluded. Ambiguous ownership, a missing
metric, Engine unavailability, or a partial sample fails the command.

The harness never invokes or parses `docker` or `podman`. Host and Engine-VM
baseline metrics are outside the container API and must be captured by a
platform-appropriate independent tool, then supplied through
`STACKCTL_BENCHMARK_HOST_METRICS_FILE`; the harness rejects a missing artifact.
The Engine-only and per-project-stack scenarios must use the baseline modes and
that same independent collector because the v8 daemon correctly refuses to
claim ownership of foreign containers. Never interpret their absence from a v8
snapshot as zero resource use.

The harness refuses to overwrite a result directory. Commit raw samples,
metadata, external host/VM samples, and a human-readable threshold comparison.
Do not use an Engine baseline captured with different VM limits or filesystem
settings.
