//! The guest-side half of the buffer-passing convention: a bump allocator
//! exposed as `alloc`/`dealloc` exports. The host writes input into a buffer
//! obtained from `alloc`, and frees every buffer it allocated (including the
//! guest's output buffer) via `dealloc` once it has copied the bytes out.
//!
//! This never returns memory to the host OS -- it leaks within the guest's
//! own linear memory until the module instance is dropped. That's fine
//! because Ferry instantiates a fresh module per call (see runtime/module.go)
//! and discards the whole instance afterwards, so nothing accumulates across
//! calls.

/// Allocates `size` bytes of guest linear memory and returns a pointer to it.
/// The memory is uninitialized; the caller is expected to fill it entirely
/// before reading it back.
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buf: Vec<u8> = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    // Hand ownership to the caller; `dealloc` reconstructs and drops the Vec.
    std::mem::forget(buf);
    ptr
}

/// Frees a buffer previously returned by `alloc`. `size` must be the same
/// capacity passed to the matching `alloc` call.
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        // Length 0 is correct here: we only ever wrote into the buffer via
        // raw memory access (Go's Memory().Write), never through the Vec
        // itself, so the Vec never had a tracked length to restore.
        drop(Vec::from_raw_parts(ptr, 0, size));
    }
}
