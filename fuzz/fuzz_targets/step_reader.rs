//! Any bytes, read as a STEP Part 21 file, are either read or rejected with an error that names
//! a position; nothing panics, overflows the stack or runs away, and the iterator stops after its
//! first error. The input is the file itself, so the seeds are plain IFC files.
//!
//! Tight limits are read twice, from the whole input and through a one-byte buffer: the result
//! must not depend on where the underlying reader splits the bytes.

#![no_main]
// A native test tool, not a core crate: libFuzzer's harness writes crash files, which the ARCH-02
// lints of clippy.toml would flag inside `fuzz_target!`.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::io::{BufRead, BufReader};

use interop_ifc::step::{Instance, Limits, Record, StepError, StepReader};
use libfuzzer_sys::fuzz_target;

/// Small enough for fuzz inputs to reach every limit.
const TIGHT: Limits = Limits {
    max_token_len: 64,
    max_instance_len: 256,
    max_nesting_depth: 4,
    max_instances: 64,
};

type Outcome = Result<(Vec<Record>, Vec<Instance>), StepError>;

fn read(input: impl BufRead, limits: Limits) -> Outcome {
    let mut reader = StepReader::new(input, limits).inspect_err(check_position)?;
    let header = reader.header().to_vec();
    let mut instances = Vec::new();
    while let Some(next) = reader.next() {
        match next {
            Ok(instance) => instances.push(instance),
            Err(error) => {
                check_position(&error);
                assert!(reader.next().is_none(), "the reader goes on after an error");
                return Err(error);
            }
        }
    }
    Ok((header, instances))
}

fn check_position(error: &StepError) {
    let position = error.position;
    assert!(position.line >= 1 && position.column >= 1, "{error}");
}

fuzz_target!(|data: &[u8]| {
    let _ = read(data, Limits::default());
    let whole = read(data, TIGHT);
    let byte_by_byte = read(BufReader::with_capacity(1, data), TIGHT);
    assert_eq!(whole, byte_by_byte);
});
