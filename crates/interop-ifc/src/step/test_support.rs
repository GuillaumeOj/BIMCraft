//! Builders shared by the reader and writer tests.

use super::value::{Parameter, Record};

/// The end of a file, after its last instance.
pub(super) const END_PART: &str = "ENDSEC;\nEND-ISO-10303-21;\n";

pub(super) fn record(name: &str, parameters: Vec<Parameter>) -> Record {
    Record {
        name: name.to_owned(),
        parameters,
    }
}

pub(super) fn text(value: &str) -> Parameter {
    Parameter::String(value.to_owned())
}

pub(super) fn typed(type_name: &str, value: Parameter) -> Parameter {
    Parameter::Typed {
        type_name: type_name.to_owned(),
        value: Box::new(value),
    }
}
