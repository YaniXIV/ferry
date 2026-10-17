//! The `{v, k, p}` wire envelope every exported guest function writes its
//! result through, and the structured error payload carried inside it when
//! a call didn't succeed.

use serde::{Deserialize, Serialize};
use std::ptr;

use crate::WasmBuf;

/// Schema version stamped on every envelope. Bump this if the envelope
/// *shape* changes (not for payload-level changes inside `p`). Kept in sync
/// with `runtime.EnvelopeVersion` on the Go side by `fixtures/envelope.msgpack`
/// (see testdata in both repos' test suites).
pub const ENVELOPE_VERSION: u16 = 1;

/// Every exported guest function's result, msgpack-encoded with struct-map
/// field names so the Go side can decode it without a schema compiler.
#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    /// Schema version -- see [`ENVELOPE_VERSION`].
    #[serde(rename = "v")]
    pub version: u16,
    /// True if `payload` decodes to the call's success type; false if it
    /// decodes to a [`WireError`].
    #[serde(rename = "k")]
    pub ok: bool,
    /// Msgpack bytes of the concrete success or error payload.
    #[serde(rename = "p", with = "serde_bytes")]
    pub payload: Vec<u8>,
}

/// Why a call failed, carried structurally so callers can branch on it
/// instead of pattern-matching the message string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The guest closure passed to `export()` panicked.
    Panic,
    /// The input bytes didn't decode into the expected type.
    Decode,
    /// The guest closure ran and returned `Err`.
    Guest,
}

/// The envelope payload when `ok` is false.
#[derive(Debug, Serialize, Deserialize)]
pub struct WireError {
    pub kind: ErrorKind,
    pub message: String,
}

/// Msgpack-encodes `value` using struct-map mode (field names, not field
/// indices) so Go's tag-based decoder can read it without IDL codegen.
fn to_msgpack<T: Serialize>(value: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    let mut buf = Vec::new();
    value.serialize(&mut rmp_serde::Serializer::new(&mut buf).with_struct_map())?;
    Ok(buf)
}

/// Allocates guest memory for `buf` and writes its pointer/length into `out`.
/// Returns `false` (and zeroes `out`) if allocation fails -- the one failure
/// mode that can't itself be reported through the envelope.
fn write_buf(out: *mut WasmBuf, buf: Vec<u8>) -> bool {
    let len = buf.len();
    if len == 0 {
        WasmBuf::zero(out);
        return false;
    }
    let ptr = crate::alloc(len);
    if ptr.is_null() {
        WasmBuf::zero(out);
        return false;
    }
    unsafe {
        ptr::copy_nonoverlapping(buf.as_ptr(), ptr, len);
        *out = WasmBuf {
            ptr: ptr as u32,
            len: len as u32,
        };
    }
    true
}

fn write_envelope(out: *mut WasmBuf, ok: bool, payload: Vec<u8>) {
    let envelope = Envelope {
        version: ENVELOPE_VERSION,
        ok,
        payload,
    };
    match to_msgpack(&envelope) {
        Ok(buf) => {
            write_buf(out, buf);
        }
        Err(_) => WasmBuf::zero(out),
    }
}

/// Writes a successful envelope wrapping `value`.
pub fn write_success<T: Serialize>(out: *mut WasmBuf, value: &T) {
    match to_msgpack(value) {
        Ok(payload) => write_envelope(out, true, payload),
        Err(e) => write_error(
            out,
            ErrorKind::Decode,
            format!("failed to encode result: {e}"),
        ),
    }
}

/// Writes a failed envelope carrying a structured [`WireError`].
pub fn write_error(out: *mut WasmBuf, kind: ErrorKind, message: String) {
    let err = WireError { kind, message };
    match to_msgpack(&err) {
        Ok(payload) => write_envelope(out, false, payload),
        Err(_) => WasmBuf::zero(out),
    }
}
