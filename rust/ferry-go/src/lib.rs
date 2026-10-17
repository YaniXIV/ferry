//! Guest-side helpers for Ferry.
//!
//! A guest crate links this in to get the alloc/dealloc exports, the
//! `{v, k, p}` wire envelope, and a panic-safe `export()` wrapper so that
//! writing a new call into the guest is just "write the closure".

mod alloc;
mod envelope;
mod export;

pub use alloc::{alloc, dealloc};
pub use envelope::{Envelope, ErrorKind, WireError, ENVELOPE_VERSION};
pub use export::export;

/// The out-parameter convention every exported guest function uses to
/// return a variable-length buffer: a pointer to this 8-byte
/// `{ptr: u32, len: u32}` struct, allocated by the caller.
#[repr(C)]
pub struct WasmBuf {
    pub ptr: u32,
    pub len: u32,
}

impl WasmBuf {
    /// Writes an all-zero header, used when we can't even allocate/serialize
    /// the envelope itself (the one failure mode `export()` can't turn into
    /// a normal error payload).
    pub fn zero(out: *mut WasmBuf) {
        unsafe {
            *out = WasmBuf { ptr: 0, len: 0 };
        }
    }
}
