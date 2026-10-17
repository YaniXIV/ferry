//! Exercises `export()` end to end against raw guest memory, the way the
//! Go host would.
//!
//! This has to run as an actual wasm32-wasip1 binary (see
//! `rust/.cargo/config.toml`'s `wasmtime run` test runner), not the host
//! target: `WasmBuf` stores pointers in `u32`, which is lossless in a real
//! 32-bit wasm address space but silently truncates a real 64-bit host
//! pointer, corrupting the roundtrip.

use ferry_go::{dealloc, Envelope, ErrorKind, WasmBuf};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct GreetIn {
    name: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct GreetOut {
    message: String,
}

/// Encodes `value` the same way the Go host encodes call input: plain
/// msgpack (not struct-map -- that asymmetry doesn't matter for decoding
/// since rmp-serde's derive Deserialize reads either shape).
fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    rmp_serde::to_vec(value).unwrap()
}

/// Runs `f` through `export()` against `input_bytes` and returns the
/// decoded envelope plus the output struct pointer/len so the test can
/// clean up.
fn run<In, Out, F>(input_bytes: &[u8], f: F) -> Envelope
where
    In: serde::de::DeserializeOwned,
    Out: Serialize,
    F: FnOnce(In) -> Result<Out, String>,
{
    let in_ptr = if input_bytes.is_empty() {
        std::ptr::null_mut()
    } else {
        let ptr = ferry_go::alloc(input_bytes.len());
        unsafe {
            std::ptr::copy_nonoverlapping(input_bytes.as_ptr(), ptr, input_bytes.len());
        }
        ptr
    };

    let mut out = WasmBuf { ptr: 0, len: 0 };
    ferry_go::export(&mut out as *mut WasmBuf, in_ptr, input_bytes.len(), f);

    if !input_bytes.is_empty() {
        dealloc(in_ptr, input_bytes.len());
    }

    assert_ne!(out.len, 0, "export() wrote an empty result");
    let out_bytes =
        unsafe { std::slice::from_raw_parts(out.ptr as *const u8, out.len as usize) }.to_vec();
    dealloc(out.ptr as *mut u8, out.len as usize);

    rmp_serde::from_slice(&out_bytes).expect("envelope decodes")
}

#[test]
fn success_roundtrip() {
    let input = encode(&GreetIn {
        name: "ferry".to_string(),
    });

    let envelope = run::<GreetIn, GreetOut, _>(&input, |input| {
        Ok(GreetOut {
            message: format!("hello, {}", input.name),
        })
    });

    assert_eq!(envelope.version, ferry_go::ENVELOPE_VERSION);
    assert!(envelope.ok);
    let out: GreetOut = rmp_serde::from_slice(&envelope.payload).unwrap();
    assert_eq!(
        out,
        GreetOut {
            message: "hello, ferry".to_string()
        }
    );
}

#[test]
fn guest_error_is_reported_structurally() {
    let input = encode(&GreetIn {
        name: "ferry".to_string(),
    });

    let envelope = run::<GreetIn, GreetOut, _>(&input, |_| Err("nope".to_string()));

    assert!(!envelope.ok);
    let err: ferry_go::WireError = rmp_serde::from_slice(&envelope.payload).unwrap();
    assert_eq!(err.kind, ErrorKind::Guest);
    assert_eq!(err.message, "nope");
}

// There's deliberately no in-process panic test here: wasm32-wasip1's
// prebuilt std forces panic=abort, so a real panic inside export() aborts
// this whole test binary's module instance (taking every other test in this
// file down with it) instead of unwinding back to a normal #[test] failure.
// The panic path is covered on the Go side instead (runtime/call_test.go),
// which instantiates a fresh module per call and observes the trap wazero
// reports -- the same way a real caller would. See docs/abi.md.

#[test]
fn malformed_input_is_a_decode_error() {
    // Not valid msgpack for GreetIn (or anything else: a single invalid byte).
    let input = vec![0xc1];

    let envelope = run::<GreetIn, GreetOut, _>(&input, |input| {
        Ok(GreetOut {
            message: format!("hello, {}", input.name),
        })
    });

    assert!(!envelope.ok);
    let err: ferry_go::WireError = rmp_serde::from_slice(&envelope.payload).unwrap();
    assert_eq!(err.kind, ErrorKind::Decode);
}

#[test]
fn empty_input_is_a_decode_error() {
    let envelope = run::<GreetIn, GreetOut, _>(&[], |input| {
        Ok(GreetOut {
            message: format!("hello, {}", input.name),
        })
    });

    assert!(!envelope.ok);
    let err: ferry_go::WireError = rmp_serde::from_slice(&envelope.payload).unwrap();
    assert_eq!(err.kind, ErrorKind::Decode);
}
