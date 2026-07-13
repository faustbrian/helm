# V8 Benchmark Protocol

## Compared scenarios

Measure on the same machine and Engine configuration:

1. Engine/VM idle baseline without Stackctl containers.
2. One representative v7 project.
3. Forty v7 projects with per-project infrastructure.
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
Stackctl revision, image digests, sample interval, and raw output.

## Success thresholds

- Forty compatible projects use one gateway and one instance of each compatible
  infrastructure fingerprint.
- V8 removes at least 90% of duplicate infrastructure containers in the
  representative 40-project scenario.
- V8 idle memory excluding the fixed Engine/VM baseline is at least 60% lower
  than the v7 40-project scenario.
- Warm reconciliation completes within two seconds after a config event and a
  repeated pass performs no runtime mutation.
- A service or gateway crash returns to ready within 30 seconds.
- Engine restart and laptop wake converge without a user command.
- Default plans publish no project application port; only gateway and explicit
  shared-service access ports are exposed.

Thresholds may be tightened by evidence, not weakened merely to pass.

## Evidence

Keep the harness under `scripts/` and result summaries under
`docs/v8/benchmarks/`. Results distinguish the unavoidable Docker Desktop/WSL
VM baseline from Stackctl workload consumption.
