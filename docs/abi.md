# Ferry wire format

This describes the Go↔Rust wire format implemented by `runtime/` (Go) and
`rust/ferry-go` (Rust), the decisions behind it, and the ones made as
simplest-sane-choice calls while building it rather than asked up front.
Ferry's design started from `noir-go`'s existing WASM bridge (see that
repo's own `docs/abi.md` for the original, Noir-shaped version); this
document describes what it turned into once generalized and actually built.

## 1. Module shape

A guest is a single Rust binary, cross-compiled to `wasm32-wasip1`,
zstd-compressed, and embedded into a Go package via `//go:embed` (see
`examples/greet/greet.go`). The host (`runtime.Runtime`) instantiates WASI
preview1 once, compiles the guest module once per `Module` (memoized with
`sync.Once` -- see `Module.compile`), and gets a **fresh module instance for
every call**, closed when the call returns (`runtime.Call`, decision below).

## 2. Exported guest functions

Every guest links `ferry-go`, which exports:

| Export | Signature | Purpose |
|---|---|---|
| `alloc` | `(size: usize) -> *mut u8` | Bump-allocate `size` bytes of guest memory. |
| `dealloc` | `(ptr: *mut u8, size: usize)` | Reclaim a buffer from `alloc`. |

...plus whatever call-specific exports the guest defines, each shaped
`extern "C" fn(out: *mut WasmBuf, in_ptr: *mut u8, in_len: usize)` and
built on `ferry_go::export`:

```rust
#[no_mangle]
pub extern "C" fn greet(out: *mut WasmBuf, in_ptr: *mut u8, in_len: usize) {
    ferry_go::export(out, in_ptr, in_len, |input: GreetIn| -> Result<GreetOut, String> {
        Ok(GreetOut { message: format!("hello, {}", input.name) })
    });
}
```

**Verified, not assumed:** `alloc`/`dealloc` are defined in the `ferry-go`
*dependency* crate, not the guest binary's own source, and still show up as
wasm exports in the final binary with no forwarding shims needed. Confirmed
by compiling `examples/greet` and listing `CompiledModule.ExportedFunctions()`
via wazero -- Rust's `#[no_mangle]` symbols get kept and exported through the
whole dependency chain on this target, not just in the crate being linked
into a binary.

`WasmBuf` is the out-parameter convention for returning a variable-length
buffer -- a pointer to an 8-byte `{ptr: u32, len: u32}` struct the caller
allocates and the callee fills in.

## 3. Call convention

One generic implementation on each side instead of one copy per export
(`noir-go` hand-duplicated this per call; see its `bridge.go`):

- Rust: `ferry_go::export(out, in_ptr, in_len, closure)` -- one function,
  used by every guest export.
- Go: `runtime.Call[In, Out any](ctx, mod, export, in)` -- one generic
  function, used by every call site.

`Call`'s five steps: `alloc(len(input))`, write input, `alloc(8)` for the
`WasmBuf` header, call the export, read the header and copy the result bytes
out of guest memory, `dealloc` everything the guest allocated (input buffer,
header, result buffer) via `defer`.

## 4. Envelope and errors

`{v, k, p}`, unchanged in shape from `noir-go`:

```rust
struct Envelope {
    #[serde(rename = "v")] version: u16,
    #[serde(rename = "k")] ok: bool,
    #[serde(rename = "p", with = "serde_bytes")] payload: Vec<u8>,
}
```

```go
type wireEnvelope struct {
    Version uint16 `msgpack:"v"`
    Ok      bool   `msgpack:"k"`
    Payload []byte `msgpack:"p"`
}
```

On failure, `payload` decodes to a **structured** error (this is the one
real shape change from `noir-go`, which only ever had a bare `message`
string):

```rust
#[serde(rename_all = "snake_case")]
enum ErrorKind { Panic, Decode, Guest }

struct WireError { kind: ErrorKind, message: String }
```

```go
type ErrorKind string
const (
    KindPanic  ErrorKind = "panic"
    KindDecode ErrorKind = "decode"
    KindGuest  ErrorKind = "guest"
)
```

`runtime.Call` always returns a `*runtime.Error{Kind, Message, Stderr}` on
failure, whether that came from a decoded `WireError` or was synthesized by
the host itself (see §6).

`Envelope.payload` and any byte-vector fields inside call-specific payloads
should use `#[serde(with = "serde_bytes")]` on the Rust side. Without it,
rmp-serde encodes a `Vec<u8>` as a msgpack array of integers rather than a
msgpack `bin` value, which is how `noir-go`'s `acir_bytes` ended up decoded
on the Go side as `[]int` instead of `[]byte` (see its `wire.go`). Ferry's
own envelope doesn't make that mistake; guest authors still need to remember
it for their own payload types, since `serde_bytes` has to be opted into
per-field.

## 5. msgpack only, struct-map mode

Both the envelope and every payload are msgpack, struct-map mode (field
names, not positions) on the Rust side (`rmp_serde::Serializer::...with_struct_map()`).
Go's default `vmihailenco/msgpack` struct encoding is already map-by-field-name
with no extra configuration needed -- confirmed by reading that library's
source (`types.go`: array mode is opt-in via an `as_array` tag option, not
the default) rather than assuming it from `noir-go`'s receive-only usage.
This is pinned as a permanent Ferry convention, not left per-call-site: every
guest crate and host call site can assume it without re-deciding.

## 6. The panic path doesn't go through the envelope -- yet

This is the most consequential thing discovered while building this, and it
changes what "catch panics" actually means in practice:

**The prebuilt `wasm32-wasip1` std ships `panic = "abort"` only.** There is
no unwinding support without building std from source (`-Z build-std` on
nightly). This was verified directly: `RUSTFLAGS="-C panic=unwind" cargo
build --target wasm32-wasip1` fails with `the crate panic_unwind does not
have the panic strategy unwind`, and a guest closure that panics inside
`ferry_go::export`'s `catch_unwind` aborts the whole module instance (wasm
trap, `wasmtime`/`wazero` report it as an instruction trap) instead of
returning control to the `Ok(Err(...))` branch.

So `ErrorKind::Panic` is **never actually produced by the guest today** --
`export()`'s `catch_unwind` is written as if it worked (harmless, and it
starts working for free if upstream ever ships unwind support for this
target) but the abort happens first. Instead:

- `ferry_go::export` installs a panic hook that `eprintln!`s the formatted
  panic message before the abort happens. Stderr is flushed immediately, so
  this is the only way the message survives.
- `runtime.Call` treats *any* error from the underlying export call
  (i.e. a trap, not a decoded envelope) as `&Error{Kind: KindPanic, Message:
  <call error>, Stderr: <captured stderr>}`.
- The Go test in `examples/greet/greet_test.go` (`TestGreetPanic`) asserts
  on `Stderr` containing the panic message, not `Message` -- that's the only
  place it's actually observable right now.

**This is also why "fresh instance per call" isn't just a memory-growth
decision (§7) -- it's a crash-isolation requirement.** A panic kills the
whole instance outright; reusing an instance after a trap would mean
calling into an already-dead module.

## 7. Decisions

### Fresh module instance per call (settled)

`runtime.Call` compiles a `Module` once and instantiates it fresh for every
call, closing the instance when the call returns. Two independent reasons,
not one:

1. The guest's bump allocator (`ferry-go/src/alloc.rs`) never returns memory
   to anything -- it leaks within the instance's own linear memory until the
   instance is dropped. A long-lived "reactor" instance reused across many
   calls would grow unboundedly.
2. A guest panic traps the whole instance (§6) with no way to recover it.
   Even if the allocator were fixed to actually reclaim, a crashed instance
   can't safely serve another call.

Building toward a real reactor (persistent instance, a free-list allocator
that can actually reclaim, and -- eventually -- real unwind support) is
future work if instantiation overhead ever matters for a high-call-volume
use case. Nothing here forecloses it; `Module` could grow a second
`CallReactor` path later without changing `Call`'s contract.

### Error envelope: `{kind, message}`, not just `message`

Settled as specified: `WireError{kind: panic|decode|guest, message}`, decoded
into `*runtime.Error{Kind, Message, Stderr}` on the Go side. `Stderr` is not
part of the wire payload -- it's the host's own captured stderr for that
call, attached after decoding (simplest sane choice: keeps the wire shape
minimal and host-transport concerns out of the guest's payload).

**Simplest-choice note:** host-side failures that aren't a decoded
`WireError` at all -- a malformed envelope, an unsupported version, a result
that doesn't decode into the expected `Out` type -- are also reported as
`KindDecode` rather than inventing a fourth, host-only kind. The caller
doesn't need to distinguish "the guest's input decode failed" from "the
host's result decode failed"; both mean the same thing operationally
("bytes that were expected to decode into a shape didn't").

### Versioning: one constant per side, enforced by a shared fixture

`ferry_go::ENVELOPE_VERSION` (Rust) and `runtime.EnvelopeVersion` (Go) are
still two independent constants with no code-level link -- but
`testdata/envelope_ok.msgpack` and `testdata/envelope_err.msgpack` are
checked-in, byte-identical fixtures both `rust/ferry-go/tests/fixture.rs` and
`runtime/fixture_test.go` decode and assert against. If either side's
envelope shape, field tags, or version drifts, the fixture test on the
*other* side (the one that wasn't updated) fails immediately, rather than
the mismatch only surfacing when two independently-built artifacts meet at
runtime. Regenerate with `cargo run --example gen_fixtures -p ferry-go`
after an intentional shape change, on both sides.

### Public runtime API: generic typed `Call`, not raw bytes or a registry

`runtime.Call[In, Out any](ctx, mod, export, in) (Out, error)` handles
marshal, the full alloc/write/call/read/dealloc sequence, envelope decode,
and result unmarshal. This is the middle of the three options considered
(raw `[]byte -> []byte`, this, or a registry/schema `Module` with
compile-time-checked named exports): it removes essentially all the
boilerplate `noir-go` had per call site, without committing to a bigger
"Module knows its own exports' types" design before there's more than one
real guest to see whether that's actually worth it.

`Runtime.NewModule(name string, load Loader)` replaces `noir-go`'s fixed
`WasmType` enum (`Compiler`/`Prover`/`Verifier`, two of which were never
implemented) with an arbitrary string key -- a `Runtime` can host any number
of named modules, each lazily loaded and compiled once.

### Toolchain and test target

- `rust/rust-toolchain.toml` pins `channel = "stable"` with `targets =
  ["wasm32-wasip1"]` (not an exact version) -- simplest choice; rustup
  auto-installs the target when needed, and there's no reason yet to pin an
  exact compiler version.
- Both crates use edition 2021, not 2024 -- nothing here needs anything
  2024 adds; simplest choice.
- **`ferry-go`'s own tests must run as real `wasm32-wasip1` guests under
  `wasmtime`, not the host target** (`rust/.cargo/config.toml` sets
  `runner = "wasmtime run --dir=."` for that target; `just test-rust` uses
  `--target wasm32-wasip1`). This isn't a style preference: `WasmBuf` stores
  pointers in `u32`, which is lossless in a genuine 32-bit wasm address
  space but silently truncates a real 64-bit host pointer when the same code
  runs natively, corrupting the roundtrip and segfaulting. This was found
  the hard way (every host-target test crashed; isolating it took a bisection
  down to a single `u32` cast) before landing on running tests inside an
  actual wasm32 sandbox, which is also just a more honest test of the real
  deployment target.

## 8. What's genuinely new vs. ported from `noir-go`

Ported as-is: the `{v,k,p}` envelope shape, the `alloc`/`dealloc` bump
allocator, the `WasmBuf` ptr+len convention, the five-step call dance,
`panic::catch_unwind` wrapping every entry point, msgpack struct-map mode,
fresh-instance-per-call, zstd+embed loading, the 200MB decompression cap,
the 4KB stderr snippet cap.

New: structured `{kind, message}` errors instead of a bare string; a single
generic `export()`/`Call` instead of one hand-rolled pair per call; a string-
keyed `Module` instead of a fixed enum; `serde_bytes` on wire byte fields;
the cross-language fixture test; and the documented (not assumed) limits of
`catch_unwind` on this target.
