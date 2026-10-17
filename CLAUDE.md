# Ferry

Ferry lets Go programs call Rust code with no CGo and no Rust toolchain
for end users. Rust is compiled to wasm32-wasip1, embedded in a Go package
(zstd-compressed, //go:embed), and run with wazero.

## Current phase
Step 1: hand-written proof of concept in examples/greet. No macros, no
codegen, no CLI yet. Keep it minimal and readable.

## Rules
- Pure Go only. Never use CGo.
- Rust side targets wasm32-wasip1.
- Prefer small, explicit code over abstraction until patterns repeat.
- Explain any unsafe Rust or WASM memory handling in comments.
