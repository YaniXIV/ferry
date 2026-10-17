set shell := ["bash", "-euo", "pipefail", "-c"]

rust_dir := "rust"
greet_out := rust_dir / "target/wasm32-wasip1/release/greet.wasm"
greet_dest := "examples/greet/greet.wasm.zst"

# Run everything: build the greet guest, then both test suites.
default: test

# Build the greet guest crate to wasm32-wasip1, zstd-compress it, and drop
# the artifact where the Go example embeds it from.
build-greet:
    cd {{rust_dir}} && cargo build --release --target wasm32-wasip1 -p greet
    zstd -19 -q -f {{greet_out}} -o {{greet_dest}}

# Unit tests for the ferry-go crate. Must run as an actual wasm32-wasip1
# guest under wasmtime (see rust/.cargo/config.toml) -- WasmBuf's pointers
# are only meaningful in a real 32-bit address space, and wasm32-wasip1's
# prebuilt std forces panic=abort, both of which behave differently (or
# outright corrupt memory) on the host target.
test-rust:
    cd {{rust_dir}} && cargo test -p ferry-go --target wasm32-wasip1

# Go runtime + example tests. Depends on the embedded greet artifact.
test-go: build-greet
    go test ./...

# Run both test suites.
test: test-rust test-go

fmt:
    cd {{rust_dir}} && cargo fmt
    gofmt -l -w .

clean:
    cd {{rust_dir}} && cargo clean
    rm -f {{greet_dest}}
