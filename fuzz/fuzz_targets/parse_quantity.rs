//! Feeds arbitrary text to the quantity parser of one dimension per input, including the
//! degrees, minutes and seconds form of angles. Parsing must never panic and only accepts
//! finite values. The first byte of an input picks the dimension (an index into `KINDS`), the
//! rest is the text; the seeds follow that format.

#![no_main]
// A native test tool, not a core crate: libFuzzer's harness writes crash files, which the ARCH-02
// lints of clippy.toml would flag inside `fuzz_target!`.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use core_model::units::{
    AngleUnit, AreaUnit, ForceUnit, LengthUnit, MassUnit, MomentUnit, Quantity, StressUnit, Unit,
    VolumeUnit,
};
use libfuzzer_sys::fuzz_target;

fn parse<U: Unit>(text: &str) {
    if let Ok(quantity) = Quantity::<U>::parse(text, U::SI) {
        assert!(quantity.si().is_finite());
    }
}

// The same list as `each_kind!` in core-model, which is private to that crate's tests. One
// dimension per input: they share the number and symbol code, so parsing each input eight
// times would only slow the fuzzer down.
const KINDS: [fn(&str); 8] = [
    parse::<LengthUnit>,
    parse::<AreaUnit>,
    parse::<VolumeUnit>,
    parse::<AngleUnit>,
    parse::<ForceUnit>,
    parse::<MomentUnit>,
    parse::<StressUnit>,
    parse::<MassUnit>,
];

fuzz_target!(|input: (u8, &str)| {
    let (kind, text) = input;
    KINDS[usize::from(kind) % KINDS.len()](text);
});
