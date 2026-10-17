//! The panic-safe wrapper every exported guest function should be built on.
//! Turns "deserialize input, run user logic, serialize output" into a single
//! call that handles decode failures and ordinary guest errors as a
//! structured envelope.
//!
//! Panics are a partial exception: the prebuilt `wasm32-wasip1` std ships
//! with `panic = "abort"` only (there's no unwinding support without
//! building std from source), so `catch_unwind` here cannot actually catch
//! anything *today* -- a panic aborts the whole module instance before
//! unwinding ever reaches it. This is still written as if it worked because
//! (a) it costs nothing and starts working for free if upstream ever ships
//! unwind support for this target, and (b) the panic hook below prints the
//! captured message to stderr as a fallback, which the Go host captures
//! separately and surfaces on the trap error it gets back from wazero. See
//! docs/abi.md for how the host turns that trap into `*ferry.Error{Kind:
//! KindPanic}`, and why calls must each get a fresh module instance.

use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::sync::Once;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::envelope::{write_error, write_success, ErrorKind};
use crate::WasmBuf;

thread_local! {
    // Captures the formatted panic message via a custom hook, since the
    // payload `catch_unwind` hands back is only reliably a message when the
    // panic was raised with a `&str`/`String` (e.g. `panic!`, `unwrap`).
    // Code that panics with `panic_any` and a non-string payload would
    // otherwise come back as a useless "no message" error; the hook sees
    // the formatted message regardless of payload type.
    static LAST_PANIC: RefCell<Option<String>> = RefCell::new(None);
}

static HOOK_INSTALLED: Once = Once::new();

fn install_panic_hook() {
    HOOK_INSTALLED.call_once(|| {
        panic::set_hook(Box::new(|info: &PanicHookInfo<'_>| {
            let msg = info.to_string();
            // The real escape hatch under panic=abort: stderr is flushed
            // immediately, so this survives even though the process aborts
            // right after and nothing downstream of catch_unwind ever runs.
            eprintln!("{msg}");
            LAST_PANIC.with(|cell| {
                *cell.borrow_mut() = Some(msg);
            });
        }));
    });
}

/// Best-effort fallback for when the hook didn't run (e.g. it was replaced
/// by something else in-process) -- downcast the raw panic payload.
fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic occurred inside guest (no message)".to_string()
    }
}

enum CallError {
    Decode(String),
    Guest(String),
}

/// Runs `f` against the msgpack-decoded input at `in_ptr[..in_len]` and
/// writes a success or error envelope to `out`. This is the body every
/// `extern "C" fn(out, in_ptr, in_len)` export should delegate to:
///
/// ```ignore
/// #[no_mangle]
/// pub extern "C" fn greet(out: *mut WasmBuf, in_ptr: *mut u8, in_len: usize) {
///     ferry_go::export(out, in_ptr, in_len, |input: GreetIn| -> Result<GreetOut, String> {
///         Ok(GreetOut { message: format!("hello, {}", input.name) })
///     });
/// }
/// ```
pub fn export<In, Out, F>(out: *mut WasmBuf, in_ptr: *mut u8, in_len: usize, f: F)
where
    In: DeserializeOwned,
    Out: Serialize,
    F: FnOnce(In) -> Result<Out, String>,
{
    install_panic_hook();
    LAST_PANIC.with(|cell| *cell.borrow_mut() = None);

    if in_ptr.is_null() {
        write_error(out, ErrorKind::Decode, "input buffer is empty".to_string());
        return;
    }
    let data: &[u8] = unsafe { std::slice::from_raw_parts(in_ptr, in_len) };

    let caught = panic::catch_unwind(AssertUnwindSafe(|| -> Result<Out, CallError> {
        let input: In = rmp_serde::from_slice(data)
            .map_err(|e| CallError::Decode(format!("decode input: {e}")))?;
        f(input).map_err(CallError::Guest)
    }));

    match caught {
        Ok(Ok(value)) => write_success(out, &value),
        Ok(Err(CallError::Decode(msg))) => write_error(out, ErrorKind::Decode, msg),
        Ok(Err(CallError::Guest(msg))) => write_error(out, ErrorKind::Guest, msg),
        Err(payload) => {
            let msg = LAST_PANIC
                .with(|cell| cell.borrow_mut().take())
                .unwrap_or_else(|| panic_payload_message(payload));
            write_error(out, ErrorKind::Panic, msg);
        }
    }
}
