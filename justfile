run: fmt
    RUSTFLAGS="-Awarnings" cargo run --release

run-dev: fmt
    RUSTFLAGS="-Awarnings" cargo run

install:
    cargo install --path .

build: fmt
    cargo build --release

build-dev: fmt
    cargo build

lint:
    rustup run nightly cargo fmt --check

audit-v8-host-dependencies:
    ./scripts/audit-v8-host-dependencies.sh

benchmark-v8 scenario output samples="12" interval="5":
    ./scripts/benchmark-v8.sh {{scenario}} {{output}} {{samples}} {{interval}}

fmt:
    rustup run nightly cargo fmt

parity-smoke engine="docker":
    ./scripts/runtime-parity-smoke.sh {{engine}}

parity-full engine="docker":
    ./scripts/runtime-parity-full.sh {{engine}}
