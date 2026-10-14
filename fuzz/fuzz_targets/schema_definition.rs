//! Any bytes, read as an entity file and as a property set file, are either accepted and turned
//! into Rust, or rejected with errors; nothing panics, overflows the stack or runs away.

#![no_main]

use libfuzzer_sys::fuzz_target;
use schema_gen::{RustOptions, Source, emit_rust, parse_sources};

const NAMESPACE: &str = "format = 1\n[namespace]\nname = \"core\"\nversion = 1\n";

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    for path in ["entities/fuzz.toml", "property-sets/fuzz.toml"] {
        let sources = [
            Source {
                path: "namespace.toml".into(),
                contents: NAMESPACE.into(),
            },
            Source {
                path: path.into(),
                contents: text.into(),
            },
        ];
        if let Ok(namespace) = parse_sources(&sources) {
            let options = RustOptions {
                id_type: "Id".into(),
            };
            let _ = emit_rust(&namespace, &options);
        }
    }
});
