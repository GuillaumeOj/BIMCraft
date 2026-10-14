//! Property tests for the STEP reader (ENG-02): generated files, printed with arbitrary spacing
//! and comments, read back to what was generated, and the limits, truncation and duplicate checks
//! hold for any of them. The `step_reader` fuzz target covers arbitrary bytes.

use std::ops::Range;

use super::{
    Instance, InstanceBody, Limits, Parameter, Record, StepError, StepErrorKind, StepReader,
};
use proptest::collection::{btree_map, vec};
use proptest::prelude::*;
use proptest::sample::Index;

/// Deepest nesting of generated values, counting an instance's own parameter list as level 1.
const MAX_DEPTH: u32 = 4;

/// Ends a data section and starts the next one.
const NEW_SECTION: [&str; 4] = ["ENDSEC", ";", "DATA", ";"];

/// A generated Part 21 file.
#[derive(Clone, Debug)]
struct File {
    header: Vec<Record>,
    /// The instances in file order, each flagged when a new `DATA;` section starts before it.
    instances: Vec<(Instance, bool)>,
}

impl File {
    fn instances(&self) -> Vec<Instance> {
        self.instances.iter().map(|(i, _)| i.clone()).collect()
    }

    fn depth(&self) -> usize {
        let records = self.instances.iter().flat_map(|(i, _)| match &i.body {
            InstanceBody::Simple(record) => std::slice::from_ref(record),
            InstanceBody::Complex(records) => records.as_slice(),
        });
        self.header
            .iter()
            .chain(records)
            .map(|r| 1 + r.parameters.iter().map(depth).max().unwrap_or(0))
            .max()
            .unwrap_or(0)
    }

    /// The file as text, with `separators` between tokens, cycled.
    fn print(&self, separators: &[&str]) -> String {
        let mut tokens = Vec::new();
        tokens.extend(["ISO-10303-21", ";", "HEADER", ";"].map(String::from));
        for record in &self.header {
            print_record(record, &mut tokens);
            tokens.push(";".into());
        }
        tokens.extend(NEW_SECTION.map(String::from));
        for (instance, new_section) in &self.instances {
            if *new_section {
                tokens.extend(NEW_SECTION.map(String::from));
            }
            tokens.extend([format!("#{}", instance.id), "=".into()]);
            match &instance.body {
                InstanceBody::Simple(record) => print_record(record, &mut tokens),
                InstanceBody::Complex(records) => {
                    tokens.push("(".into());
                    for record in records {
                        print_record(record, &mut tokens);
                    }
                    tokens.push(")".into());
                }
            }
            tokens.push(";".into());
        }
        tokens.extend(["ENDSEC", ";", "END-ISO-10303-21", ";"].map(String::from));

        let mut text = String::new();
        for (token, separator) in tokens.iter().zip(separators.iter().cycle()) {
            if !text.is_empty() {
                text.push_str(separator);
            }
            text.push_str(token);
        }
        text
    }
}

/// Nesting levels of lists and typed parameters in `parameter`.
fn depth(parameter: &Parameter) -> usize {
    match parameter {
        Parameter::List(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Parameter::Typed { value, .. } => 1 + depth(value),
        _ => 0,
    }
}

fn print_record(record: &Record, tokens: &mut Vec<String>) {
    tokens.push(record.name.clone());
    print_list(&record.parameters, tokens);
}

fn print_list(parameters: &[Parameter], tokens: &mut Vec<String>) {
    tokens.push("(".into());
    for (i, parameter) in parameters.iter().enumerate() {
        if i > 0 {
            tokens.push(",".into());
        }
        print_parameter(parameter, tokens);
    }
    tokens.push(")".into());
}

fn print_parameter(parameter: &Parameter, tokens: &mut Vec<String>) {
    let token = match parameter {
        Parameter::Null => "$".into(),
        Parameter::Derived => "*".into(),
        Parameter::Integer(value) => value.to_string(),
        Parameter::Real(value) => real(*value),
        Parameter::String(text) => string(text),
        Parameter::Enumeration(name) => format!(".{name}."),
        Parameter::Binary(digits) => format!("\"{digits}\""),
        Parameter::Reference(id) => format!("#{id}"),
        Parameter::Typed { type_name, value } => {
            tokens.extend([type_name.clone(), "(".into()]);
            print_parameter(value, tokens);
            ")".into()
        }
        Parameter::List(items) => return print_list(items, tokens),
    };
    tokens.push(token);
}

/// A string literal. Strings of odd length are wrapped in their middle, a line break the reader
/// must drop; each half is escaped on its own so the break never splits a doubled quote.
fn string(text: &str) -> String {
    let escape = |part: &str| part.replace('\'', "''");
    if text.len().is_multiple_of(2) {
        return format!("'{}'", escape(text));
    }
    let (start, end) = text.split_at(text.len() / 2);
    format!("'{}\r\n{}'", escape(start), escape(end))
}

/// A real in Part 21 form, which needs a `.` in the mantissa: `1e-5` becomes `1.E-5`. Rust prints
/// the shortest text that reads back to the same value.
fn real(value: f64) -> String {
    let text = format!("{value:?}").replace('e', "E");
    if text.contains('.') {
        text
    } else {
        text.replacen('E', ".E", 1)
    }
}

/// An entity or type name, user-defined (`!NAME`) or standard.
fn keyword() -> impl Strategy<Value = String> {
    "(![A-Za-z]|[A-Za-z_])[A-Za-z0-9_]{0,8}"
}

fn parameter() -> impl Strategy<Value = Parameter> {
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
    // Records add the first level, so values nest at most MAX_DEPTH - 1 levels.
    leaf.prop_recursive(MAX_DEPTH - 1, 32, 4, |inner| {
        prop_oneof![
            vec(inner.clone(), 0..4).prop_map(Parameter::List),
            (keyword(), inner).prop_map(|(type_name, value)| Parameter::Typed {
                type_name,
                value: Box::new(value),
            }),
        ]
    })
}

fn record(name: impl Strategy<Value = String>) -> impl Strategy<Value = Record> {
    (name, vec(parameter(), 0..4)).prop_map(|(name, parameters)| Record { name, parameters })
}

fn body() -> impl Strategy<Value = InstanceBody> {
    prop_oneof![
        record(keyword()).prop_map(InstanceBody::Simple),
        vec(record(keyword()), 1..4).prop_map(InstanceBody::Complex),
    ]
}

/// A file with a number of instances in `instance_count`.
fn file(instance_count: Range<usize>) -> impl Strategy<Value = File> {
    let header_name = keyword().prop_filter("ENDSEC closes the header", |name| name != "ENDSEC");
    let instances = btree_map(any::<u64>(), (body(), any::<bool>()), instance_count)
        .prop_map(|map| {
            map.into_iter()
                .map(|(id, (body, new_section))| (Instance { id, body }, new_section))
                .collect::<Vec<_>>()
        })
        .prop_shuffle();
    (vec(record(header_name), 0..4), instances)
        .prop_map(|(header, instances)| File { header, instances })
}

fn separators() -> impl Strategy<Value = Vec<&'static str>> {
    let separator = prop_oneof![
        Just(""),
        Just(" "),
        Just("\n"),
        Just("\r\n\t"),
        Just("/* a comment */"),
    ];
    vec(separator, 1..8)
}

/// The header and every instance, or the first error.
fn read(text: &str, limits: Limits) -> Result<(Vec<Record>, Vec<Instance>), StepError> {
    let reader = StepReader::new(text.as_bytes(), limits)?;
    let header = reader.header().to_vec();
    Ok((header, reader.collect::<Result<_, _>>()?))
}

#[test]
fn prints_reals_with_a_dot_in_the_mantissa() {
    assert_eq!(real(0.25), "0.25");
    assert_eq!(real(-1.5e300), "-1.5E300");
    assert_eq!(real(1e-7), "1.E-7");
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    #[test]
    fn reads_back_what_was_written(file in file(0..8), separators in separators()) {
        let text = file.print(&separators);
        let (header, instances) = read(&text, Limits::default()).unwrap();
        prop_assert_eq!(&header, &file.header);
        prop_assert_eq!(instances, file.instances());
    }

    #[test]
    fn rejects_exactly_the_files_nested_too_deep(file in file(0..8), limit in 1..=MAX_DEPTH as usize) {
        let limits = Limits { max_nesting_depth: limit, ..Limits::default() };
        let result = read(&file.print(&[" "]), limits);
        if file.depth() <= limit {
            prop_assert!(result.is_ok(), "{:?}", result);
        } else {
            prop_assert_eq!(result.unwrap_err().kind, StepErrorKind::TooDeep { limit });
        }
    }

    #[test]
    fn rejects_every_truncated_file(
        file in file(0..8),
        separators in separators(),
        cut in any::<Index>(),
    ) {
        let text = file.print(&separators);
        let prefix = &text[..cut.index(text.len())];
        let mut yielded = Vec::new();
        let error = match StepReader::new(prefix.as_bytes(), Limits::default()) {
            Err(error) => Some(error),
            Ok(mut reader) => {
                let error = reader.by_ref().find_map(|result| match result {
                    Ok(instance) => {
                        yielded.push(instance);
                        None
                    }
                    Err(error) => Some(error),
                });
                prop_assert!(reader.next().is_none(), "the reader goes on after an error");
                error
            }
        };
        prop_assert!(error.is_some(), "a truncated file was accepted");
        let all = file.instances();
        prop_assert!(yielded.len() <= all.len());
        prop_assert_eq!(yielded.as_slice(), &all[..yielded.len()]);
    }

    #[test]
    fn rejects_a_reused_instance_name(
        mut file in file(1..8),
        which in any::<Index>(),
        body in body(),
    ) {
        let id = which.get(&file.instances).0.id;
        let count = file.instances.len();
        file.instances.push((Instance { id, body }, false));
        let text = file.print(&[" "]);
        let mut reader = StepReader::new(text.as_bytes(), Limits::default()).unwrap();
        let yielded: Vec<_> = reader.by_ref().take(count).collect::<Result<_, _>>().unwrap();
        prop_assert_eq!(yielded.len(), count);
        prop_assert_eq!(reader.next().unwrap().unwrap_err().kind, StepErrorKind::DuplicateInstance(id));
    }
}
