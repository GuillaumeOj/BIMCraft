//! Any bytes, read as an EXPRESS schema, are either parsed, resolved and turned into Rust, or
//! rejected with errors; nothing panics, overflows the stack or runs away.

#![no_main]

use express::{RustOptions, emit_rust, parse, resolve};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(schema) = parse(source) else {
        return;
    };
    if let Ok(model) = resolve(&schema) {
        let options = RustOptions {
            runtime: "rt".into(),
            doc_url: None,
        };
        let _ = emit_rust(&model, &options);
    }
});
