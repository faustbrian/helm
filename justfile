run: fmt
    cargo run --release

run-dev: fmt
    cargo run

install:
    cargo install --path .

build: fmt
    cargo build --release

build-dev: fmt
    cargo build

lint:
    ./scripts/audit-v8-lint-policy.sh
    rustup run nightly cargo fmt --check
    cargo clippy --all-targets --all-features

audit-v8-lint-policy:
    ./scripts/audit-v8-lint-policy.sh

audit-v8-host-dependencies:
    ./scripts/audit-v8-host-dependencies.sh

accept-v8-gateway output="target/gateway-acceptance-record.txt":
    ./scripts/accept-v8-gateway.sh {{output}}

benchmark-v8 scenario output samples="12" interval="5":
    ./scripts/benchmark-v8.sh {{scenario}} {{output}} {{samples}} {{interval}}

fmt:
    rustup run nightly cargo fmt
