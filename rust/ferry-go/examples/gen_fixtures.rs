//! Regenerates the checked-in envelope fixtures under testdata/ at the repo
//! root. Both ferry-go's and the Go runtime's test suites decode these same
//! bytes to catch the two languages' envelope encodings silently drifting
//! apart (a renamed field tag, a version bump on one side only, etc).
//!
//! Run with `cargo run --example gen_fixtures -p ferry-go` (host target --
//! this only needs plain file I/O, not the wasm32-wasip1 guest machinery)
//! after an intentional change to the envelope shape. Not run automatically.

use ferry_go::{Envelope, ErrorKind, WireError, ENVELOPE_VERSION};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct FixtureOut {
    message: String,
}

fn to_struct_map<T: Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    value
        .serialize(&mut rmp_serde::Serializer::new(&mut buf).with_struct_map())
        .unwrap();
    buf
}

fn testdata_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata"))
}

fn main() {
    let ok_payload = to_struct_map(&FixtureOut {
        message: "fixture".to_string(),
    });
    let ok_envelope = to_struct_map(&Envelope {
        version: ENVELOPE_VERSION,
        ok: true,
        payload: ok_payload,
    });

    let err_payload = to_struct_map(&WireError {
        kind: ErrorKind::Guest,
        message: "fixture error".to_string(),
    });
    let err_envelope = to_struct_map(&Envelope {
        version: ENVELOPE_VERSION,
        ok: false,
        payload: err_payload,
    });

    let dir = testdata_dir();
    std::fs::write(dir.join("envelope_ok.msgpack"), &ok_envelope).unwrap();
    std::fs::write(dir.join("envelope_err.msgpack"), &err_envelope).unwrap();
    println!(
        "wrote {} and {} bytes to {}",
        ok_envelope.len(),
        err_envelope.len(),
        dir.display()
    );
}
