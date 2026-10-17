# ferry

Ferry lets Go programs call Rust code with no CGo and no Rust toolchain for
end users. Rust is compiled to `wasm32-wasip1`, embedded in a Go package
(zstd-compressed, `//go:embed`), and run with [wazero](https://wazero.io).

See [docs/abi.md](docs/abi.md) for the wire format and the decisions behind
it.

## Layout

- `runtime/` -- the Go host: `Runtime`, `Module`, and the generic
  `Call[In, Out any]`.
- `rust/ferry-go/` -- the guest-side crate: `alloc`/`dealloc`, the wire
  envelope, and the `export()` helper every guest export is built on.
- `rust/examples/greet/` -- a guest crate exercising all three call
  outcomes (success, a guest error, a panic).
- `examples/greet/` -- the Go package embedding that guest's compiled,
  compressed wasm, plus the Go types mirroring its calls.
- `testdata/` -- checked-in envelope fixtures both language's test suites
  decode, guarding against the wire format drifting between them.

## Quick start

Build the example guest and run both test suites:

```sh
just build-greet   # compiles rust/examples/greet to wasm32-wasip1,
                    # zstd-compresses it into examples/greet/greet.wasm.zst
just test           # cargo test (ferry-go, under wasmtime) + go test
```

Calling a guest export from Go looks like this (see
`examples/greet/greet_test.go` for the full thing):

```go
ctx := context.Background()
rt, err := runtime.NewRuntime(ctx)
if err != nil {
    log.Fatal(err)
}
defer rt.Close(ctx)

mod := rt.NewModule("greet", greet.Load)

out, err := runtime.Call[greet.GreetIn, greet.GreetOut](ctx, mod, "greet", greet.GreetIn{
    Name: "ferry",
})
if err != nil {
    var ferr *runtime.Error
    if errors.As(err, &ferr) {
        // ferr.Kind is one of runtime.KindPanic, runtime.KindDecode, runtime.KindGuest.
        log.Fatalf("%s: %s", ferr.Kind, ferr.Message)
    }
    log.Fatal(err)
}
fmt.Println(out.Message) // "hello, ferry"
```

And the guest side it's calling into (`rust/examples/greet/src/main.rs`):

```rust
use ferry_go::{export, WasmBuf};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct GreetIn { name: String }

#[derive(Serialize)]
struct GreetOut { message: String }

#[no_mangle]
pub extern "C" fn greet(out: *mut WasmBuf, in_ptr: *mut u8, in_len: usize) {
    export(out, in_ptr, in_len, |input: GreetIn| -> Result<GreetOut, String> {
        Ok(GreetOut { message: format!("hello, {}", input.name) })
    });
}
```

A new guest export is: write a `#[derive(Deserialize)]` input struct, a
`#[derive(Serialize)]` output struct, and one `export()` call -- the
alloc/dealloc plumbing, the envelope, and (best-effort; see
[docs/abi.md](docs/abi.md), §6) panic handling are already done for you.

## Requirements

- Go 1.24+
- Rust via `rustup` (the `wasm32-wasip1` target is pinned in
  `rust/rust-toolchain.toml` and auto-installed on first build)
- [`just`](https://github.com/casey/just)
- `zstd` and [`wasmtime`](https://wasmtime.dev) (the latter runs
  `ferry-go`'s own test suite as a real `wasm32-wasip1` guest -- see
  docs/abi.md for why that's required, not optional)

## Current phase

Step 1: hand-written proof of concept in `examples/greet`. No macros, no
codegen, no CLI yet. Keep it minimal and readable.

## Rules

- Pure Go only. Never use CGo.
- Rust side targets `wasm32-wasip1`.
- Prefer small, explicit code over abstraction until patterns repeat.
- Explain any unsafe Rust or WASM memory handling in comments.
