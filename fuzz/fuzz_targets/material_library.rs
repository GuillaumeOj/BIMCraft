//! Any bytes, read as a material library source from a user or a plugin, are either added whole
//! to the library or refused with an error that leaves it unchanged; nothing panics or runs away.

#![no_main]

use libfuzzer_sys::fuzz_target;
use material::Library;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mut library = Library::default();
    if library.extend("fuzz", text).is_err() {
        assert!(library.is_empty());
    } else {
        for (key, material) in library.iter() {
            assert_eq!(material.library_key(), Some(key));
        }
    }
});
