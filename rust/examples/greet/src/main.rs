//! A minimal ferry-go guest exercising all three call outcomes Ferry's Go
//! runtime has to handle: success, a structured guest error, and a panic.

use ferry_go::{export, WasmBuf};
use serde::{Deserialize, Serialize};

fn main() {}

#[derive(Deserialize)]
struct GreetIn {
    name: String,
}

#[derive(Serialize)]
struct GreetOut {
    message: String,
}

#[no_mangle]
pub extern "C" fn greet(out: *mut WasmBuf, in_ptr: *mut u8, in_len: usize) {
    export(
        out,
        in_ptr,
        in_len,
        |input: GreetIn| -> Result<GreetOut, String> {
            Ok(GreetOut {
                message: format!("hello, {}", input.name),
            })
        },
    );
}

#[derive(Deserialize)]
struct FailIn {
    reason: String,
}

#[derive(Serialize)]
struct FailOut {}

#[no_mangle]
pub extern "C" fn fail(out: *mut WasmBuf, in_ptr: *mut u8, in_len: usize) {
    export(
        out,
        in_ptr,
        in_len,
        |input: FailIn| -> Result<FailOut, String> { Err(input.reason) },
    );
}

#[derive(Deserialize)]
struct PanicIn {
    message: String,
}

#[derive(Serialize)]
struct PanicOut {}

#[no_mangle]
pub extern "C" fn panic(out: *mut WasmBuf, in_ptr: *mut u8, in_len: usize) {
    export(
        out,
        in_ptr,
        in_len,
        |input: PanicIn| -> Result<PanicOut, String> {
            panic!("{}", input.message);
        },
    );
}
