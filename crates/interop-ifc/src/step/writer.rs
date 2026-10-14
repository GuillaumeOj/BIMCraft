use std::collections::BTreeSet;
use std::fmt::{self, Write as _};
use std::io::{self, Write};

use super::lexer::{
    DATA, END_MAGIC, ENDSEC, HEADER, MAGIC, is_binary, is_enumeration, is_keyword, is_string_byte,
};
use super::value::{Instance, InstanceBody, Parameter, Record};

/// Streaming writer for ISO 10303-21 (STEP Part 21) exchange files, the mirror of
/// [`StepReader`](super::StepReader).
///
/// [`new`](Self::new) writes the header and opens the data section,
/// [`write_instance`](Self::write_instance) appends one instance, and [`finish`](Self::finish)
/// closes the file. The output is deterministic: one statement per line, no spaces, instances and
/// the records of complex instances in the order given, and each real written in the shortest form
/// that reads back to the same value. Reading the output back yields the values written.
///
/// Every value is checked before anything is written, so the output always follows the grammar
/// the reader accepts, and an instance that fails the checks leaves the output unchanged. The
/// writer does not apply the reader's [`Limits`](super::Limits): a value nested deeper, or a
/// string or instance longer, than the limits a reader uses is written but rejected on reading.
///
/// Each instance reaches `out` in one `write_all` call: wrap a file or socket in an
/// [`io::BufWriter`].
///
/// ```
/// use interop_ifc::step::{Instance, InstanceBody, Parameter, Record, StepWriter};
///
/// let schema = Record {
///     name: "FILE_SCHEMA".to_owned(),
///     parameters: vec![Parameter::List(vec![Parameter::String("IFC4X3_ADD2".to_owned())])],
/// };
/// let mut writer = StepWriter::new(Vec::new(), &[schema])?;
/// writer.write_instance(&Instance {
///     id: 1,
///     body: InstanceBody::Simple(Record {
///         name: "IFCCARTESIANPOINT".to_owned(),
///         parameters: vec![Parameter::List(vec![Parameter::Real(0.0), Parameter::Real(1.5)])],
///     }),
/// })?;
/// let bytes = writer.finish()?;
/// assert_eq!(
///     String::from_utf8(bytes).unwrap(),
///     "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC4X3_ADD2'));\nENDSEC;\nDATA;\n\
///      #1=IFCCARTESIANPOINT((0.0,1.5));\nENDSEC;\nEND-ISO-10303-21;\n"
/// );
/// # Ok::<(), interop_ifc::step::WriteError>(())
/// ```
pub struct StepWriter<W: Write> {
    out: W,
    /// Names of the instances written so far, to reject duplicates.
    ids: BTreeSet<u64>,
    /// The statement being written, sent to `out` only once it is complete and valid.
    statement: String,
}

impl<W: Write> StepWriter<W> {
    /// Writes the file header with `header` as its entries, such as `FILE_DESCRIPTION`,
    /// `FILE_NAME` and `FILE_SCHEMA`, then opens the data section.
    ///
    /// # Errors
    ///
    /// Returns a [`WriteError`] if an entry cannot be written as Part 21 or `out` fails.
    pub fn new(out: W, header: &[Record]) -> Result<Self, WriteError> {
        let mut this = Self {
            out,
            ids: BTreeSet::new(),
            statement: String::new(),
        };
        this.end_statement(MAGIC);
        this.end_statement(HEADER);
        for entry in header {
            // The reader takes an entry named ENDSEC for the end of the header.
            if entry.name == ENDSEC {
                return Err(WriteError::InvalidName);
            }
            this.record(entry)?;
            this.end_statement("");
        }
        this.end_statement(ENDSEC);
        this.end_statement(DATA);
        this.flush_statement()?;
        Ok(this)
    }

    /// Appends `instance` to the data section.
    ///
    /// # Errors
    ///
    /// Returns a [`WriteError`] if the instance cannot be written as Part 21, reuses the name of
    /// an instance already written, or `out` fails. An instance rejected by the checks writes
    /// nothing; after an output failure, part of the instance may have been written and the file
    /// is incomplete, so discard it.
    pub fn write_instance(&mut self, instance: &Instance) -> Result<(), WriteError> {
        if !self.ids.insert(instance.id) {
            return Err(WriteError::DuplicateInstance(instance.id));
        }
        self.statement.push('#');
        push_display(&mut self.statement, instance.id);
        self.statement.push('=');
        let written = match &instance.body {
            InstanceBody::Simple(record) => self.record(record),
            InstanceBody::Complex(records) => self.complex(records),
        };
        if let Err(error) = written {
            self.statement.clear();
            self.ids.remove(&instance.id);
            return Err(error);
        }
        self.end_statement("");
        self.flush_statement()
    }

    /// Closes the data section and the file, flushes the output and returns it.
    ///
    /// # Errors
    ///
    /// Returns a [`WriteError`] if `out` fails.
    pub fn finish(mut self) -> Result<W, WriteError> {
        self.end_statement(ENDSEC);
        self.end_statement(END_MAGIC);
        self.flush_statement()?;
        self.out.flush()?;
        Ok(self.out)
    }

    /// Appends `tail`, such as a section keyword, and the `;` and line break ending a statement.
    fn end_statement(&mut self, tail: &str) {
        self.statement.push_str(tail);
        self.statement.push_str(";\n");
    }

    fn flush_statement(&mut self) -> Result<(), WriteError> {
        let written = self.out.write_all(self.statement.as_bytes());
        self.statement.clear();
        Ok(written?)
    }

    fn complex(&mut self, records: &[Record]) -> Result<(), WriteError> {
        if records.is_empty() {
            return Err(WriteError::EmptyComplexInstance);
        }
        self.statement.push('(');
        for record in records {
            self.record(record)?;
        }
        self.statement.push(')');
        Ok(())
    }

    fn record(&mut self, record: &Record) -> Result<(), WriteError> {
        self.keyword(&record.name)?;
        self.parameters(&record.parameters)
    }

    /// Writes a parameter list with its parentheses.
    ///
    /// Nested lists and typed parameters are tracked on an explicit stack rather than by
    /// recursion, so any nesting can be written with a flat call stack.
    fn parameters(&mut self, parameters: &[Parameter]) -> Result<(), WriteError> {
        let mut pending = Vec::new();
        self.statement.push('(');
        push_items(&mut pending, parameters);
        while let Some(step) = pending.pop() {
            match step {
                Step::Text(text) => self.statement.push_str(text),
                Step::Value(value) => self.value(value, &mut pending)?,
            }
        }
        Ok(())
    }

    /// Writes `value`, or for a list or typed parameter its opening, scheduling the rest on
    /// `pending`.
    fn value<'a>(
        &mut self,
        value: &'a Parameter,
        pending: &mut Vec<Step<'a>>,
    ) -> Result<(), WriteError> {
        let text = &mut self.statement;
        match value {
            Parameter::Null => text.push('$'),
            Parameter::Derived => text.push('*'),
            Parameter::Integer(value) => push_display(text, value),
            Parameter::Real(value) => push_real(text, *value)?,
            Parameter::String(value) => push_string(text, value)?,
            Parameter::Enumeration(name) => {
                if !is_enumeration(name) {
                    return Err(WriteError::InvalidEnumeration);
                }
                text.push('.');
                text.push_str(name);
                text.push('.');
            }
            Parameter::Binary(digits) => {
                if !is_binary(digits) {
                    return Err(WriteError::InvalidBinary);
                }
                text.push('"');
                text.push_str(digits);
                text.push('"');
            }
            Parameter::Reference(id) => {
                text.push('#');
                push_display(text, id);
            }
            Parameter::List(items) => {
                text.push('(');
                push_items(pending, items);
            }
            Parameter::Typed { type_name, value } => {
                self.keyword(type_name)?;
                self.statement.push('(');
                pending.push(Step::Text(")"));
                pending.push(Step::Value(value));
            }
        }
        Ok(())
    }

    /// Writes an entity or type name, checking that it reads back as one keyword.
    fn keyword(&mut self, name: &str) -> Result<(), WriteError> {
        if !is_keyword(name) {
            return Err(WriteError::InvalidName);
        }
        self.statement.push_str(name);
        Ok(())
    }
}

/// What remains to be written of a parameter list, popped from the end.
enum Step<'a> {
    Value(&'a Parameter),
    Text(&'static str),
}

/// Schedules `items` and the closing parenthesis of their list, the first item on top.
fn push_items<'a>(pending: &mut Vec<Step<'a>>, items: &'a [Parameter]) {
    pending.push(Step::Text(")"));
    for (index, item) in items.iter().enumerate().rev() {
        pending.push(Step::Value(item));
        if index > 0 {
            pending.push(Step::Text(","));
        }
    }
}

/// Writes a real in the shortest form that reads back to the same value.
///
/// Rust's `Debug` form is the shortest round-trip one, but it may omit the decimal point and uses
/// a lowercase exponent (`1e-7`); Part 21 requires the point and writes `1.E-7`.
fn push_real(text: &mut String, value: f64) -> Result<(), WriteError> {
    if !value.is_finite() {
        return Err(WriteError::NonFiniteReal);
    }
    let start = text.len();
    push_display(text, format_args!("{value:?}"));
    let exponent = text[start..].find('e').map(|index| start + index);
    if let Some(index) = exponent {
        text.replace_range(index..=index, "E");
    }
    let mantissa_end = exponent.unwrap_or(text.len());
    if !text[start..mantissa_end].contains('.') {
        text.insert(mantissa_end, '.');
    }
    Ok(())
}

/// Appends `value` as [`fmt::Display`] writes it, without a temporary string.
fn push_display(text: &mut String, value: impl fmt::Display) {
    // Writing to a `String` cannot fail.
    let _ = write!(text, "{value}");
}

/// Writes a string between quotes, doubling the quotes it contains. Encoding directives such as
/// `\X2\` are written as they are, since the reader keeps them as written (INT-03).
fn push_string(text: &mut String, value: &str) -> Result<(), WriteError> {
    if !value.bytes().all(is_string_byte) {
        return Err(WriteError::InvalidString);
    }
    text.push('\'');
    for (index, part) in value.split('\'').enumerate() {
        if index > 0 {
            text.push_str("''");
        }
        text.push_str(part);
    }
    text.push('\'');
    Ok(())
}

/// Why writing a STEP file failed.
///
/// Messages never quote the content being written: it can hold personal data (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    /// The underlying writer failed.
    #[error("writing the output failed ({0})")]
    Io(io::ErrorKind),
    /// An entity or type name that is not a Part 21 keyword, or a header entry named `ENDSEC`.
    #[error("invalid entity or type name")]
    InvalidName,
    /// A string with a character outside printable ASCII; encode it with `\X2\` directives.
    #[error("string with a character outside printable ASCII")]
    InvalidString,
    /// An empty enumeration value, or one with a character other than a letter, digit or `_`.
    #[error("invalid enumeration value")]
    InvalidEnumeration,
    /// A binary literal that is empty, has a digit that is not uppercase hexadecimal, or starts
    /// with a digit above 3.
    #[error("malformed binary literal")]
    InvalidBinary,
    /// An infinite or NaN real, which Part 21 cannot represent.
    #[error("real that is not finite")]
    NonFiniteReal,
    /// A complex instance without any record.
    #[error("complex instance without any record")]
    EmptyComplexInstance,
    /// Two instances with the same name.
    #[error("instance #{0} written more than once")]
    DuplicateInstance(u64),
}

impl From<io::Error> for WriteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

#[cfg(test)]
mod tests {
    use std::mem;

    use proptest::prelude::*;

    use super::*;
    use crate::step::test_support::{END_PART, record, text, typed};
    use crate::step::{Limits, StepReader};

    const START: &str = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n";

    fn simple(id: u64, name: &str, parameters: Vec<Parameter>) -> Instance {
        Instance {
            id,
            body: InstanceBody::Simple(record(name, parameters)),
        }
    }

    fn write(header: &[Record], instances: &[Instance]) -> Result<String, WriteError> {
        let mut writer = StepWriter::new(Vec::new(), header)?;
        for instance in instances {
            writer.write_instance(instance)?;
        }
        Ok(String::from_utf8(writer.finish()?).unwrap())
    }

    /// The data line written for one `X` instance with `parameters`.
    fn data_line(parameters: Vec<Parameter>) -> Result<String, WriteError> {
        let output = write(&[], &[simple(1, "X", parameters)])?;
        Ok(output
            .strip_prefix(START)
            .and_then(|rest| rest.strip_suffix(END_PART))
            .unwrap()
            .to_owned())
    }

    fn read(input: &str) -> (Vec<Record>, Vec<Instance>) {
        let reader = StepReader::new(input.as_bytes(), Limits::default()).unwrap();
        let header = reader.header().to_vec();
        (header, reader.collect::<Result<_, _>>().unwrap())
    }

    #[test]
    fn writes_the_header_and_instances_in_order() {
        let header = [
            record(
                "FILE_DESCRIPTION",
                vec![Parameter::List(vec![text("ViewDefinition")]), text("2;1")],
            ),
            record(
                "FILE_SCHEMA",
                vec![Parameter::List(vec![text("IFC4X3_ADD2")])],
            ),
        ];
        let instances = [
            simple(2, "IFCB", vec![Parameter::Reference(1)]),
            simple(1, "IFCA", vec![]),
        ];
        assert_eq!(
            write(&header, &instances).unwrap(),
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition'),'2;1');\n\
             FILE_SCHEMA(('IFC4X3_ADD2'));\nENDSEC;\nDATA;\n#2=IFCB(#1);\n#1=IFCA();\n\
             ENDSEC;\nEND-ISO-10303-21;\n"
        );
    }

    #[test]
    fn writes_an_empty_file() {
        assert_eq!(write(&[], &[]).unwrap(), format!("{START}{END_PART}"));
    }

    #[test]
    fn writes_every_parameter_kind() {
        let parameters = vec![
            Parameter::Null,
            Parameter::Derived,
            Parameter::Integer(-3),
            Parameter::Integer(i64::MIN),
            Parameter::Real(2.5),
            text("a"),
            Parameter::Enumeration("T".to_owned()),
            Parameter::Binary("0F".to_owned()),
            Parameter::Reference(9),
            Parameter::List(vec![]),
            Parameter::List(vec![
                Parameter::Integer(1),
                Parameter::List(vec![Parameter::Integer(2)]),
            ]),
            typed("IFCLABEL", text("w")),
            typed("A", typed("B", Parameter::List(vec![Parameter::Null]))),
        ];
        assert_eq!(
            data_line(parameters).unwrap(),
            "#1=X($,*,-3,-9223372036854775808,2.5,'a',.T.,\"0F\",#9,(),(1,(2)),IFCLABEL('w'),\
             A(B(($))));\n"
        );
    }

    #[test]
    fn writes_complex_instances_in_the_order_given() {
        let instance = Instance {
            id: 3,
            body: InstanceBody::Complex(vec![
                record("B", vec![Parameter::Integer(1)]),
                record("A", vec![]),
            ]),
        };
        assert_eq!(
            write(&[], &[instance]).unwrap(),
            format!("{START}#3=(B(1)A());\n{END_PART}")
        );
    }

    #[test]
    fn writes_reals_in_the_shortest_form_with_a_decimal_point() {
        let cases = [
            (0.2, "0.2"),
            (3.0, "3.0"),
            (-0.0, "-0.0"),
            (1e-7, "1.E-7"),
            (-1.5e-7, "-1.5E-7"),
            (1e20, "1.E20"),
            (f64::MAX, "1.7976931348623157E308"),
            (f64::MIN_POSITIVE, "2.2250738585072014E-308"),
            (5e-324, "5.E-324"),
        ];
        for (value, expected) in cases {
            assert_eq!(
                data_line(vec![Parameter::Real(value)]).unwrap(),
                format!("#1=X({expected});\n"),
                "{value:?}"
            );
            let (_, instances) = read(&format!("{START}#1=X({expected});\n{END_PART}"));
            assert_eq!(
                instances[0].body,
                InstanceBody::Simple(record("X", vec![Parameter::Real(value)]))
            );
        }
    }

    #[test]
    fn writes_strings_raw_with_doubled_quotes() {
        assert_eq!(
            data_line(vec![text(r"it's \X2\00E9\X0\ ''")]).unwrap(),
            "#1=X('it''s \\X2\\00E9\\X0\\ ''''');\n"
        );
    }

    #[test]
    fn writes_user_defined_and_section_style_keywords() {
        assert_eq!(
            data_line(vec![typed("!MY_TYPE-2", Parameter::Integer(1))]).unwrap(),
            "#1=X(!MY_TYPE-2(1));\n"
        );
    }

    #[test]
    fn rejects_values_the_reader_would_not_read_back() {
        let cases = [
            (Parameter::Real(f64::NAN), WriteError::NonFiniteReal),
            (Parameter::Real(f64::INFINITY), WriteError::NonFiniteReal),
            (text("tab\there"), WriteError::InvalidString),
            (text("line\nbreak"), WriteError::InvalidString),
            (text("na\u{ef}ve"), WriteError::InvalidString),
            (
                Parameter::Enumeration(String::new()),
                WriteError::InvalidEnumeration,
            ),
            (
                Parameter::Enumeration("A.B".to_owned()),
                WriteError::InvalidEnumeration,
            ),
            (Parameter::Binary(String::new()), WriteError::InvalidBinary),
            (
                Parameter::Binary("4F".to_owned()),
                WriteError::InvalidBinary,
            ),
            (
                Parameter::Binary("0f".to_owned()),
                WriteError::InvalidBinary,
            ),
            (typed("", Parameter::Null), WriteError::InvalidName),
            (typed("1A", Parameter::Null), WriteError::InvalidName),
            (typed("A B", Parameter::Null), WriteError::InvalidName),
            (typed("!", Parameter::Null), WriteError::InvalidName),
            (typed("!_A", Parameter::Null), WriteError::InvalidName),
            (
                Parameter::List(vec![Parameter::Integer(1), Parameter::Real(f64::NAN)]),
                WriteError::NonFiniteReal,
            ),
        ];
        for (parameter, expected) in cases {
            assert_eq!(
                data_line(vec![parameter.clone()]),
                Err(expected),
                "{parameter:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_records() {
        let cases = [
            (simple(1, "", vec![]), WriteError::InvalidName),
            (simple(1, "A;B", vec![]), WriteError::InvalidName),
            (
                Instance {
                    id: 1,
                    body: InstanceBody::Complex(vec![]),
                },
                WriteError::EmptyComplexInstance,
            ),
            (
                Instance {
                    id: 1,
                    body: InstanceBody::Complex(vec![record("A", vec![]), record("2", vec![])]),
                },
                WriteError::InvalidName,
            ),
        ];
        for (instance, expected) in cases {
            assert_eq!(write(&[], &[instance]), Err(expected));
        }
        for name in ["ENDSEC", "", "A B"] {
            assert_eq!(
                StepWriter::new(Vec::new(), &[record(name, vec![])]).err(),
                Some(WriteError::InvalidName)
            );
        }
    }

    #[test]
    fn an_invalid_instance_writes_nothing() {
        let mut writer = StepWriter::new(Vec::new(), &[]).unwrap();
        writer
            .write_instance(&simple(1, "A", vec![Parameter::Integer(1)]))
            .unwrap();
        let invalid = simple(2, "B", vec![Parameter::Integer(2), text("\u{e9}")]);
        assert_eq!(
            writer.write_instance(&invalid),
            Err(WriteError::InvalidString)
        );
        // The rejected instance's name stays free.
        writer.write_instance(&simple(2, "C", vec![])).unwrap();
        let output = String::from_utf8(writer.finish().unwrap()).unwrap();
        assert_eq!(output, format!("{START}#1=A(1);\n#2=C();\n{END_PART}"));
    }

    #[test]
    fn rejects_duplicate_instances() {
        let instances = [simple(4, "A", vec![]), simple(4, "B", vec![])];
        assert_eq!(
            write(&[], &instances),
            Err(WriteError::DuplicateInstance(4))
        );
    }

    /// An output that accepts `Some(n)` bytes and then fails, or with `None` accepts every write
    /// and fails to flush.
    struct Failing(Option<usize>);

    impl Write for Failing {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let Some(capacity) = &mut self.0 else {
                return Ok(buf.len());
            };
            if *capacity == 0 {
                return Err(io::ErrorKind::StorageFull.into());
            }
            let written = buf.len().min(*capacity);
            *capacity -= written;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            match self.0 {
                Some(_) => Ok(()),
                None => Err(io::ErrorKind::BrokenPipe.into()),
            }
        }
    }

    #[test]
    fn reports_output_failures() {
        let storage_full = Some(WriteError::Io(io::ErrorKind::StorageFull));
        let roomy = StepWriter::new(Failing(Some(usize::MAX)), &[]).unwrap();
        assert!(roomy.finish().is_ok());
        assert_eq!(StepWriter::new(Failing(Some(0)), &[]).err(), storage_full);

        let mut writer = StepWriter::new(Failing(Some(START.len())), &[]).unwrap();
        assert_eq!(
            writer.write_instance(&simple(1, "A", vec![])).err(),
            storage_full
        );
        assert_eq!(writer.finish().err(), storage_full);

        let writer = StepWriter::new(Failing(None), &[]).unwrap();
        assert_eq!(
            writer.finish().err(),
            Some(WriteError::Io(io::ErrorKind::BrokenPipe))
        );
    }

    #[test]
    fn writes_deep_nesting_without_recursion() {
        let depth = 100_000;
        let mut value = Parameter::Null;
        for level in 0..depth {
            value = if level % 2 == 0 {
                Parameter::List(vec![value])
            } else {
                typed("T", value)
            };
        }
        let instance = simple(1, "X", vec![value]);
        let mut writer = StepWriter::new(Vec::new(), &[]).unwrap();
        writer.write_instance(&instance).unwrap();
        // Dropping a value this deep would recurse; leak it before any assertion can unwind.
        mem::forget(instance);
        let output = writer.finish().unwrap();
        let line_len = output.len() - START.len() - END_PART.len();
        assert_eq!(line_len, "#1=X($);\n".len() + depth / 2 * "()T()".len());
    }

    #[test]
    fn errors_describe_themselves_without_quoting_content() {
        let cases = [
            (
                WriteError::Io(io::ErrorKind::BrokenPipe),
                "writing the output failed (broken pipe)",
            ),
            (WriteError::InvalidName, "invalid entity or type name"),
            (
                WriteError::InvalidString,
                "string with a character outside printable ASCII",
            ),
            (WriteError::InvalidEnumeration, "invalid enumeration value"),
            (WriteError::InvalidBinary, "malformed binary literal"),
            (WriteError::NonFiniteReal, "real that is not finite"),
            (
                WriteError::EmptyComplexInstance,
                "complex instance without any record",
            ),
            (
                WriteError::DuplicateInstance(12),
                "instance #12 written more than once",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    /// Names the reader reads back as one keyword.
    fn keyword() -> impl Strategy<Value = String> {
        "!?[A-Za-z][A-Za-z0-9_-]{0,8}"
    }

    /// Any parameter the writer accepts, nested a few levels deep.
    fn parameter() -> impl Strategy<Value = Parameter> {
        let leaf = prop_oneof![
            Just(Parameter::Null),
            Just(Parameter::Derived),
            any::<i64>().prop_map(Parameter::Integer),
            any::<f64>()
                .prop_filter("finite", |value| value.is_finite())
                .prop_map(Parameter::Real),
            "[ -~]{0,12}".prop_map(Parameter::String),
            "[A-Za-z0-9_]{1,8}".prop_map(Parameter::Enumeration),
            "[0-3][0-9A-F]{0,8}".prop_map(Parameter::Binary),
            any::<u64>().prop_map(Parameter::Reference),
        ];
        leaf.prop_recursive(4, 32, 6, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..6).prop_map(Parameter::List),
                (keyword(), inner).prop_map(|(type_name, value)| Parameter::Typed {
                    type_name,
                    value: Box::new(value),
                }),
            ]
        })
    }

    fn records() -> impl Strategy<Value = Vec<Record>> {
        let record = (keyword(), proptest::collection::vec(parameter(), 0..4))
            .prop_map(|(name, parameters)| Record { name, parameters });
        proptest::collection::vec(record, 1..4)
    }

    /// Instances with distinct names, simple or complex.
    fn instances() -> impl Strategy<Value = Vec<Instance>> {
        let body = prop_oneof![
            records().prop_map(|mut records| InstanceBody::Simple(records.remove(0))),
            records().prop_map(InstanceBody::Complex),
        ];
        proptest::collection::btree_map(any::<u64>(), body, 0..6).prop_map(|bodies| {
            bodies
                .into_iter()
                .map(|(id, body)| Instance { id, body })
                .collect()
        })
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn reads_back_what_it_writes(header in records(), instances in instances()) {
            // The reader takes an entry named ENDSEC for the end of the header.
            prop_assume!(header.iter().all(|entry| entry.name != ENDSEC));
            let output = write(&header, &instances).unwrap();
            let (read_header, read_instances) = read(&output);
            prop_assert_eq!(&read_header, &header);
            prop_assert_eq!(&read_instances, &instances);
            prop_assert_eq!(write(&read_header, &read_instances).unwrap(), output);
        }
    }
}
