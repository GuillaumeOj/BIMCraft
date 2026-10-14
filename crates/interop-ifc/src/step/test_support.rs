//! Builders and property-test generators shared by the reader and writer tests.

use std::ops::Range;

use proptest::collection::{btree_map, vec};
use proptest::prelude::*;

use super::value::{Instance, InstanceBody, Parameter, Record};

/// The end of a file, after its last instance.
pub(super) const END_PART: &str = "ENDSEC;\nEND-ISO-10303-21;\n";

pub(crate) fn record(name: &str, parameters: Vec<Parameter>) -> Record {
    Record {
        name: name.to_owned(),
        parameters,
    }
}

pub(super) fn text(value: &str) -> Parameter {
    Parameter::String(value.to_owned())
}

pub(crate) fn typed(type_name: &str, value: Parameter) -> Parameter {
    Parameter::Typed {
        type_name: type_name.to_owned(),
        value: Box::new(value),
    }
}

/// Deepest nesting of generated parameters, not counting the record's own parameter list.
pub(super) const MAX_VALUE_DEPTH: u32 = 4;

/// An entity or type name the reader reads back as one keyword, user-defined (`!NAME`) or
/// standard.
pub(super) fn keyword() -> impl Strategy<Value = String> {
    "(![A-Za-z]|[A-Za-z_])[A-Za-z0-9_-]{0,8}"
}

/// Any parameter the reader and the writer accept, nested up to [`MAX_VALUE_DEPTH`] levels.
pub(super) fn parameter() -> impl Strategy<Value = Parameter> {
    let finite = prop::num::f64::POSITIVE
        | prop::num::f64::NEGATIVE
        | prop::num::f64::NORMAL
        | prop::num::f64::SUBNORMAL
        | prop::num::f64::ZERO;
    let leaf = prop_oneof![
        Just(Parameter::Null),
        Just(Parameter::Derived),
        any::<i64>().prop_map(Parameter::Integer),
        finite.prop_map(Parameter::Real),
        "[ -~]{0,12}".prop_map(Parameter::String),
        "[A-Za-z0-9_]{1,8}".prop_map(Parameter::Enumeration),
        "[0-3][0-9A-F]{0,8}".prop_map(Parameter::Binary),
        any::<u64>().prop_map(Parameter::Reference),
    ];
    leaf.prop_recursive(MAX_VALUE_DEPTH, 32, 4, |inner| {
        prop_oneof![
            vec(inner.clone(), 0..4).prop_map(Parameter::List),
            (keyword(), inner).prop_map(|(type_name, value)| Parameter::Typed {
                type_name,
                value: Box::new(value),
            }),
        ]
    })
}

fn any_record(name: impl Strategy<Value = String>) -> impl Strategy<Value = Record> {
    (name, vec(parameter(), 0..4)).prop_map(|(name, parameters)| Record { name, parameters })
}

/// Header entries. None is named `ENDSEC`, which the reader takes for the end of the header.
pub(super) fn header() -> impl Strategy<Value = Vec<Record>> {
    let name = keyword().prop_filter("ENDSEC closes the header", |name| name != "ENDSEC");
    vec(any_record(name), 0..4)
}

/// Instances with distinct names, simple or complex, in any order.
pub(super) fn instances(count: Range<usize>) -> impl Strategy<Value = Vec<Instance>> {
    let body = prop_oneof![
        any_record(keyword()).prop_map(InstanceBody::Simple),
        vec(any_record(keyword()), 1..4).prop_map(InstanceBody::Complex),
    ];
    btree_map(any::<u64>(), body, count)
        .prop_map(|bodies| {
            bodies
                .into_iter()
                .map(|(id, body)| Instance { id, body })
                .collect::<Vec<_>>()
        })
        .prop_shuffle()
}
