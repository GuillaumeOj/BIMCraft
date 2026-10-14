use std::collections::BTreeSet;
use std::io::BufRead;
use std::iter::FusedIterator;
use std::mem;

use super::error::{Position, StepError, StepErrorKind};
use super::lexer::{
    DATA, END_MAGIC, ENDSEC, HEADER, Lexer, MAGIC, Spanned, Token, unexpected_token,
};
use super::limits::Limits;
use super::value::{Instance, InstanceBody, Parameter, Record};

/// Reads a Part 21 file as a stream of instances.
///
/// [`new`](Self::new) reads everything up to the first instance; iterating yields the instances
/// in file order. Iteration stops after the end of the file or after the first error, which is
/// yielded once.
///
/// ```
/// use interop_ifc::step::{InstanceBody, Limits, StepReader};
///
/// let file = "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4X3_ADD2'));ENDSEC;\
///             DATA;#1=IFCWALL($);ENDSEC;END-ISO-10303-21;";
/// let mut reader = StepReader::new(file.as_bytes(), Limits::default())?;
/// assert_eq!(reader.header()[0].name, "FILE_SCHEMA");
/// let wall = reader.next().transpose()?.expect("one instance");
/// assert!(matches!(wall.body, InstanceBody::Simple(ref record) if record.name == "IFCWALL"));
/// assert!(reader.next().is_none());
/// # Ok::<(), interop_ifc::step::StepError>(())
/// ```
pub struct StepReader<R> {
    lexer: Lexer<R>,
    limits: Limits,
    header: Vec<Record>,
    /// Names of the instances read so far, to reject duplicates. Bounded by `max_instances`.
    ids: BTreeSet<u64>,
    /// Byte offset where the current statement (instance, header entry, section marker) starts.
    statement_start: u64,
    /// Whether the next token starts a statement: statements end with `;`.
    at_statement_start: bool,
    /// [`Limits::max_instance_len`] as an offset difference.
    max_instance_len: u64,
    finished: bool,
}

impl<R: BufRead> StepReader<R> {
    /// Reads the file header and the start of the data section from `reader`.
    ///
    /// # Errors
    ///
    /// Returns a [`StepError`] if the input does not start with a well-formed Part 21 header
    /// followed by `DATA;`, exceeds `limits`, or cannot be read.
    pub fn new(reader: R, limits: Limits) -> Result<Self, StepError> {
        let mut this = Self {
            lexer: Lexer::new(reader, limits.max_token_len),
            limits,
            header: Vec::new(),
            ids: BTreeSet::new(),
            statement_start: 0,
            at_statement_start: true,
            max_instance_len: u64::try_from(limits.max_instance_len).unwrap_or(u64::MAX),
            finished: false,
        };
        this.keyword_statement(MAGIC)?;
        this.keyword_statement(HEADER)?;
        loop {
            let spanned = this.token()?;
            match spanned.token {
                Token::Keyword(name) if name == ENDSEC => break,
                Token::Keyword(name) => {
                    this.check_instance_count(spanned.position)?;
                    let record = this.record(name)?;
                    this.expect(&Token::Semicolon)?;
                    this.header.push(record);
                }
                token => {
                    return Err(unexpected_token(
                        "a header entry or ENDSEC",
                        &token,
                        spanned.position,
                    ));
                }
            }
        }
        this.expect(&Token::Semicolon)?;
        this.keyword_statement(DATA)?;
        Ok(this)
    }

    /// The header entries, such as `FILE_DESCRIPTION`, `FILE_NAME` and `FILE_SCHEMA`, in file
    /// order.
    #[must_use]
    pub fn header(&self) -> &[Record] {
        &self.header
    }

    fn next_instance(&mut self) -> Result<Option<Instance>, StepError> {
        loop {
            let spanned = self.token()?;
            match spanned.token {
                Token::InstanceName(id) => return self.instance(id, spanned.position).map(Some),
                Token::Keyword(name) if name == ENDSEC => {
                    if self.end_of_section()? {
                        return Ok(None);
                    }
                }
                token => {
                    return Err(unexpected_token(
                        "an instance or ENDSEC",
                        &token,
                        spanned.position,
                    ));
                }
            }
        }
    }

    /// Reads what follows `ENDSEC` in the data part: `true` at the end of the file, `false` when
    /// another data section starts.
    fn end_of_section(&mut self) -> Result<bool, StepError> {
        self.expect(&Token::Semicolon)?;
        let spanned = self.token()?;
        match spanned.token {
            Token::Keyword(name) if name == DATA => self.expect(&Token::Semicolon).map(|()| false),
            Token::Keyword(name) if name == END_MAGIC => {
                self.expect(&Token::Semicolon)?;
                self.expect(&Token::End).map(|()| true)
            }
            token => Err(unexpected_token(
                "DATA or END-ISO-10303-21",
                &token,
                spanned.position,
            )),
        }
    }

    /// Reads an instance after its name.
    fn instance(&mut self, id: u64, position: Position) -> Result<Instance, StepError> {
        self.check_instance_count(position)?;
        if !self.ids.insert(id) {
            return Err(StepError::new(
                StepErrorKind::DuplicateInstance(id),
                position,
            ));
        }
        self.expect(&Token::Equals)?;
        let spanned = self.token()?;
        let body = match spanned.token {
            Token::Keyword(name) => InstanceBody::Simple(self.record(name)?),
            Token::OpenParen => InstanceBody::Complex(self.complex_records()?),
            token => {
                return Err(unexpected_token(
                    "an entity name or `(`",
                    &token,
                    spanned.position,
                ));
            }
        };
        self.expect(&Token::Semicolon)?;
        Ok(Instance { id, body })
    }

    /// Fails if one more header entry or instance would exceed the limit, which they share.
    fn check_instance_count(&self, position: Position) -> Result<(), StepError> {
        let limit = self.limits.max_instances;
        if self.header.len() + self.ids.len() < limit {
            return Ok(());
        }
        Err(StepError::new(
            StepErrorKind::TooManyInstances { limit },
            position,
        ))
    }

    /// Reads the records of a complex instance after its opening parenthesis.
    fn complex_records(&mut self) -> Result<Vec<Record>, StepError> {
        let mut records = Vec::new();
        loop {
            let spanned = self.token()?;
            match spanned.token {
                Token::Keyword(name) => records.push(self.record(name)?),
                Token::CloseParen if !records.is_empty() => return Ok(records),
                token => return Err(unexpected_token("an entity name", &token, spanned.position)),
            }
        }
    }

    /// Reads a record's parameter list after its name.
    fn record(&mut self, name: String) -> Result<Record, StepError> {
        self.expect(&Token::OpenParen)?;
        let parameters = self.parameters()?;
        Ok(Record { name, parameters })
    }

    /// Reads a parameter list after its opening parenthesis, up to its closing parenthesis.
    ///
    /// Nested lists and typed parameters are tracked on an explicit stack rather than by
    /// recursion, so the nesting limit alone bounds the work and the call stack stays flat.
    fn parameters(&mut self) -> Result<Vec<Parameter>, StepError> {
        let mut outer = Vec::new();
        let mut current = Frame::default();
        let mut depth = 1;
        let mut expecting_value = true;
        loop {
            let spanned = self.token()?;
            let position = spanned.position;
            let mut value = match spanned.token {
                Token::Comma if !expecting_value => {
                    expecting_value = true;
                    continue;
                }
                Token::OpenParen if expecting_value => {
                    self.enter(&mut depth, position)?;
                    outer.push(mem::take(&mut current));
                    continue;
                }
                Token::Keyword(type_name) if expecting_value => {
                    self.enter(&mut depth, position)?;
                    self.expect(&Token::OpenParen)?;
                    current.typed.push(type_name);
                    continue;
                }
                Token::CloseParen if !expecting_value || current.is_empty() => {
                    let Some(parent) = outer.pop() else {
                        return Ok(current.items);
                    };
                    depth -= 1;
                    Parameter::List(mem::replace(&mut current, parent).items)
                }
                token if expecting_value => simple_parameter(token)
                    .map_err(|token| unexpected_token("a parameter", &token, position))?,
                token => return Err(unexpected_token("`,` or `)`", &token, position)),
            };
            while let Some(type_name) = current.typed.pop() {
                self.expect(&Token::CloseParen)?;
                depth -= 1;
                value = Parameter::Typed {
                    type_name,
                    value: Box::new(value),
                };
            }
            current.items.push(value);
            expecting_value = false;
        }
    }

    /// Goes one nesting level deeper, failing past the limit.
    fn enter(&self, depth: &mut usize, position: Position) -> Result<(), StepError> {
        let limit = self.limits.max_nesting_depth;
        if *depth >= limit {
            return Err(StepError::new(StepErrorKind::TooDeep { limit }, position));
        }
        *depth += 1;
        Ok(())
    }

    /// Reads a section keyword followed by `;`.
    fn keyword_statement(&mut self, keyword: &'static str) -> Result<(), StepError> {
        let spanned = self.token()?;
        match spanned.token {
            Token::Keyword(name) if name == keyword => self.expect(&Token::Semicolon),
            token => Err(unexpected_token(keyword, &token, spanned.position)),
        }
    }

    /// Reads a token, failing once its statement exceeds the instance length limit.
    fn token(&mut self) -> Result<Spanned, StepError> {
        let spanned = self.lexer.next_token()?;
        if self.at_statement_start {
            self.statement_start = spanned.offset;
        }
        self.at_statement_start = spanned.token == Token::Semicolon;
        if spanned.offset - self.statement_start > self.max_instance_len {
            let limit = self.limits.max_instance_len;
            let kind = StepErrorKind::InstanceTooLong { limit };
            return Err(StepError::new(kind, spanned.position));
        }
        Ok(spanned)
    }

    fn expect(&mut self, expected: &Token) -> Result<(), StepError> {
        let spanned = self.token()?;
        if spanned.token == *expected {
            Ok(())
        } else {
            Err(unexpected_token(
                expected.describe(),
                &spanned.token,
                spanned.position,
            ))
        }
    }
}

impl<R: BufRead> Iterator for StepReader<R> {
    type Item = Result<Instance, StepError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let next = self.next_instance().transpose();
        self.finished = !matches!(next, Some(Ok(_)));
        next
    }
}

impl<R: BufRead> FusedIterator for StepReader<R> {}

/// A parameter list being read: its items so far, and the typed parameters opened since the last
/// item, innermost last, each waiting for the value it wraps.
#[derive(Default)]
struct Frame {
    items: Vec<Parameter>,
    typed: Vec<String>,
}

impl Frame {
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.typed.is_empty()
    }
}

/// The parameter a single token stands for, or the token back if it does not stand for one.
fn simple_parameter(token: Token) -> Result<Parameter, Token> {
    Ok(match token {
        Token::Null => Parameter::Null,
        Token::Derived => Parameter::Derived,
        Token::Integer(value) => Parameter::Integer(value),
        Token::Real(value) => Parameter::Real(value),
        Token::String(text) => Parameter::String(text),
        Token::Enumeration(name) => Parameter::Enumeration(name),
        Token::Binary(digits) => Parameter::Binary(digits),
        Token::InstanceName(id) => Parameter::Reference(id),
        token => return Err(token),
    })
}
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::step::test_support::{END_PART, record, text, typed};

    const HEADER_PART: &str =
        "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC4X3_ADD2'));\nENDSEC;\nDATA;\n";

    fn file(data: &str) -> String {
        format!("{HEADER_PART}{data}\n{END_PART}")
    }

    fn read_with(input: &str, limits: Limits) -> Result<Vec<Instance>, StepError> {
        StepReader::new(input.as_bytes(), limits)?.collect()
    }

    /// A file with an empty header and `data` on one line, so short limits only hit the data.
    fn minimal(data: &str) -> String {
        format!("ISO-10303-21;HEADER;ENDSEC;DATA;{data}ENDSEC;END-ISO-10303-21;")
    }

    /// Column of `data`'s first byte in [`minimal`].
    const MINIMAL_DATA_COLUMN: u64 = 33;

    fn read(input: &str) -> Result<Vec<Instance>, StepError> {
        read_with(input, Limits::default())
    }

    /// Reads one data line and returns its error, which must be on line 6 of [`file`].
    fn data_error(data: &str) -> (StepErrorKind, u64) {
        let error = read(&file(data)).expect_err("data should be rejected");
        assert_eq!(error.position.line, 6, "{data}: {error}");
        (error.kind, error.position.column)
    }

    /// Reads `data` and returns the bodies of its instances.
    fn bodies(data: &str) -> Vec<InstanceBody> {
        read(&file(data))
            .unwrap()
            .into_iter()
            .map(|i| i.body)
            .collect()
    }

    /// The body of a simple `X` instance.
    fn x(parameters: Vec<Parameter>) -> InstanceBody {
        InstanceBody::Simple(record("X", parameters))
    }

    fn unexpected_kind(expected: &'static str, found: &'static str) -> StepErrorKind {
        StepErrorKind::UnexpectedToken { expected, found }
    }

    #[test]
    fn reads_the_header_and_instances_in_order() {
        let input = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition'),'2;1');\n\
                     FILE_SCHEMA(('IFC4X3_ADD2'));\nENDSEC;\nDATA;\n#1=IFCA();\n#2=IFCB(#1);\n\
                     ENDSEC;\nEND-ISO-10303-21;\n";
        let mut reader = StepReader::new(input.as_bytes(), Limits::default()).unwrap();
        assert_eq!(
            reader.header(),
            [
                record(
                    "FILE_DESCRIPTION",
                    vec![Parameter::List(vec![text("ViewDefinition")]), text("2;1")],
                ),
                record(
                    "FILE_SCHEMA",
                    vec![Parameter::List(vec![text("IFC4X3_ADD2")])],
                ),
            ]
        );
        assert_eq!(
            reader.next().unwrap().unwrap(),
            Instance {
                id: 1,
                body: InstanceBody::Simple(record("IFCA", vec![])),
            }
        );
        assert_eq!(
            reader.next().unwrap().unwrap(),
            Instance {
                id: 2,
                body: InstanceBody::Simple(record("IFCB", vec![Parameter::Reference(1)])),
            }
        );
        assert!(reader.next().is_none());
        assert!(reader.next().is_none());
    }

    #[test]
    fn reads_every_parameter_kind() {
        assert_eq!(
            bodies("#1=X($,*,-3,2.5,'a',.T.,\"0F\",#9,(),(1,(2)),IFCLABEL('w'));"),
            [x(vec![
                Parameter::Null,
                Parameter::Derived,
                Parameter::Integer(-3),
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
            ])]
        );
    }

    #[test]
    fn reads_nested_typed_parameters_and_lists() {
        assert_eq!(
            bodies("#1=X(A(B(1)),C((1.,2.)),(D(.F.),E($)));"),
            [x(vec![
                typed("A", typed("B", Parameter::Integer(1))),
                typed(
                    "C",
                    Parameter::List(vec![Parameter::Real(1.0), Parameter::Real(2.0)]),
                ),
                Parameter::List(vec![
                    typed("D", Parameter::Enumeration("F".to_owned())),
                    typed("E", Parameter::Null),
                ]),
            ])]
        );
    }

    #[test]
    fn reads_complex_instances() {
        assert_eq!(
            read(&file("#5=(A()B(1,$)C(#5));")).unwrap(),
            [Instance {
                id: 5,
                body: InstanceBody::Complex(vec![
                    record("A", vec![]),
                    record("B", vec![Parameter::Integer(1), Parameter::Null]),
                    record("C", vec![Parameter::Reference(5)]),
                ]),
            }]
        );
    }

    #[test]
    fn reads_several_data_sections() {
        let input = format!("{HEADER_PART}#1=A();ENDSEC;DATA;#2=B();{END_PART}");
        let ids: Vec<u64> = read(&input).unwrap().iter().map(|i| i.id).collect();
        assert_eq!(ids, [1, 2]);
    }

    #[test]
    fn rejects_malformed_parameter_lists() {
        let cases = [
            ("#1=X(1,);", unexpected_kind("a parameter", "`)`"), 8),
            ("#1=X(,1);", unexpected_kind("a parameter", "`,`"), 6),
            ("#1=X(1 2);", unexpected_kind("`,` or `)`", "an integer"), 8),
            ("#1=X(A());", unexpected_kind("a parameter", "`)`"), 8),
            ("#1=X(A 1);", unexpected_kind("`(`", "an integer"), 8),
            ("#1=X(A(1,2));", unexpected_kind("`)`", "`,`"), 9),
            ("#1=X(=);", unexpected_kind("a parameter", "`=`"), 6),
            ("#1=X 1;", unexpected_kind("`(`", "an integer"), 6),
        ];
        for (data, kind, column) in cases {
            assert_eq!(data_error(data), (kind, column), "{data}");
        }
    }

    #[test]
    fn rejects_malformed_instances() {
        let cases = [
            ("#1 X();", unexpected_kind("`=`", "a keyword"), 4),
            (
                "#1=1;",
                unexpected_kind("an entity name or `(`", "an integer"),
                4,
            ),
            ("#1=X() ENDSEC;", unexpected_kind("`;`", "a keyword"), 8),
            ("#1=();", unexpected_kind("an entity name", "`)`"), 5),
            (
                "#1=(A()1);",
                unexpected_kind("an entity name", "an integer"),
                8,
            ),
            (
                "X();",
                unexpected_kind("an instance or ENDSEC", "a keyword"),
                1,
            ),
        ];
        for (data, kind, column) in cases {
            assert_eq!(data_error(data), (kind, column), "{data}");
        }
    }

    #[test]
    fn rejects_duplicate_instances() {
        assert_eq!(
            data_error("#1=A(); #1=B();"),
            (StepErrorKind::DuplicateInstance(1), 9)
        );
    }

    #[test]
    fn rejects_a_malformed_header() {
        let cases = [
            ("", unexpected_kind("ISO-10303-21", "end of input")),
            ("ISO-10303-21 HEADER;", unexpected_kind("`;`", "a keyword")),
            ("ISO-10303-21;DATA;", unexpected_kind("HEADER", "a keyword")),
            (
                "ISO-10303-21;HEADER;1;",
                unexpected_kind("a header entry or ENDSEC", "an integer"),
            ),
            (
                "ISO-10303-21;HEADER;ENDSEC;ENDSEC;",
                unexpected_kind("DATA", "a keyword"),
            ),
            (
                "ISO-10303-21;HEADER;A()ENDSEC;",
                unexpected_kind("`;`", "a keyword"),
            ),
        ];
        for (input, kind) in cases {
            let error = StepReader::new(input.as_bytes(), Limits::default())
                .err()
                .expect(input);
            assert_eq!(error.kind, kind, "{input}");
        }
    }

    #[test]
    fn rejects_a_malformed_end_of_file() {
        let cases = [
            (
                "#1=A();ENDSEC END-ISO-10303-21;",
                unexpected_kind("`;`", "a keyword"),
            ),
            (
                "#1=A();ENDSEC;#2=B();",
                unexpected_kind("DATA or END-ISO-10303-21", "an instance name"),
            ),
            (
                "#1=A();ENDSEC;END-ISO-10303-21",
                unexpected_kind("`;`", "end of input"),
            ),
            (
                "#1=A();ENDSEC;END-ISO-10303-21;X",
                unexpected_kind("end of input", "a keyword"),
            ),
            (
                "#1=A();",
                unexpected_kind("an instance or ENDSEC", "end of input"),
            ),
        ];
        for (data, kind) in cases {
            let input = format!("{HEADER_PART}{data}");
            let mut reader = StepReader::new(input.as_bytes(), Limits::default()).unwrap();
            assert!(reader.next().unwrap().is_ok(), "{data}");
            assert_eq!(reader.next().unwrap().unwrap_err().kind, kind, "{data}");
            assert!(
                reader.next().is_none(),
                "{data}: iteration stops after an error"
            );
        }
    }

    #[test]
    fn reports_lexical_errors_from_the_reader() {
        assert_eq!(
            data_error("#1=X(@);"),
            (StepErrorKind::UnexpectedByte(b'@'), 6)
        );
    }

    #[test]
    fn enforces_the_token_length_limit() {
        // `END-ISO-10303-21`, the longest keyword of the file, has 16 bytes.
        let limits = Limits {
            max_token_len: 16,
            ..Limits::default()
        };
        assert!(read_with(&minimal("#1=X('abcdefghijklmnop');"), limits).is_ok());
        let error = read_with(&minimal("#1=X('abcdefghijklmnopq');"), limits).unwrap_err();
        assert_eq!(error.kind, StepErrorKind::TokenTooLong { limit: 16 });
        assert_eq!(error.position.column, MINIMAL_DATA_COLUMN + 5);
    }

    #[test]
    fn enforces_the_instance_length_limit() {
        // The limit applies to the offset where each token starts: `;` is at 16 in
        // `#1=X(12345,6789);`, as in `END-ISO-10303-21;`.
        let limits = Limits {
            max_instance_len: 16,
            ..Limits::default()
        };
        read_with(&minimal("#1=X(12345,6789);"), limits).unwrap();
        let error = read_with(&minimal("#1=X(12345,67890);"), limits).unwrap_err();
        assert_eq!(error.kind, StepErrorKind::InstanceTooLong { limit: 16 });
        assert_eq!(error.position.column, MINIMAL_DATA_COLUMN + 17);
    }

    #[test]
    fn counts_comments_in_the_instance_length() {
        let limits = Limits {
            max_instance_len: 16,
            ..Limits::default()
        };
        let error = read_with(&minimal("#1=X(/* hidden comment */);"), limits).unwrap_err();
        assert_eq!(error.kind, StepErrorKind::InstanceTooLong { limit: 16 });
    }

    #[test]
    fn enforces_the_nesting_limit_on_lists_and_typed_parameters() {
        let limits = Limits {
            max_nesting_depth: 3,
            ..Limits::default()
        };
        for data in [
            "#1=X(((1)));",
            "#1=X(A(B(1)));",
            "#1=X((A(1)));",
            "#1=X((1),(2));",
        ] {
            assert!(read_with(&file(data), limits).is_ok(), "{data}");
        }
        for (data, column) in [
            ("#1=X((((1))));", 8),
            ("#1=X(A(B(C(1))));", 10),
            ("#1=X(A(((1))));", 9),
        ] {
            let error = read_with(&file(data), limits).unwrap_err();
            assert_eq!(error.kind, StepErrorKind::TooDeep { limit: 3 }, "{data}");
            assert_eq!(error.position.column, column, "{data}");
        }
    }

    #[test]
    fn enforces_the_instance_limit() {
        let limits = Limits {
            max_instances: 2,
            ..Limits::default()
        };
        assert_eq!(
            read_with(&minimal("#1=A();#2=B();"), limits).unwrap().len(),
            2
        );
        let error = read_with(&minimal("#1=A();#2=B();#3=C();"), limits).unwrap_err();
        assert_eq!(error.kind, StepErrorKind::TooManyInstances { limit: 2 });
        assert_eq!(error.position.column, MINIMAL_DATA_COLUMN + 14);
        // `file` has one header entry, which takes a share of the same budget.
        let error = read_with(&file("#1=A();#2=B();"), limits).unwrap_err();
        assert_eq!(error.kind, StepErrorKind::TooManyInstances { limit: 2 });
        assert_eq!(error.position.column, 8);
    }

    #[test]
    fn counts_header_entries_against_the_instance_limit() {
        let limits = Limits {
            max_instances: 1,
            ..Limits::default()
        };
        let input = "ISO-10303-21;HEADER;A();B();ENDSEC;DATA;ENDSEC;END-ISO-10303-21;";
        let error = StepReader::new(input.as_bytes(), limits).err().unwrap();
        assert_eq!(error.kind, StepErrorKind::TooManyInstances { limit: 1 });
        assert_eq!(error.position.column, 25);
    }

    #[test]
    fn reads_deep_nesting_without_recursion() {
        let depth = 100_000;
        let limits = Limits {
            max_nesting_depth: depth + 1,
            ..Limits::default()
        };
        let data = format!("#1=X({}{});", "(".repeat(depth), ")".repeat(depth));
        let instances = read_with(&file(&data), limits).unwrap();
        assert_eq!(instances.len(), 1);
        // Dropping a value this deep would recurse; leak it, the test ends here.
        mem::forget(instances);
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
            let _ = StepReader::new(bytes.as_slice(), Limits::default()).map(Iterator::count);
        }

        #[test]
        fn never_panics_on_arbitrary_data(data in "[#=(),;$*.'\"A-Z0-9 +-]{0,128}") {
            let input = file(&data);
            let limits = Limits {
                max_nesting_depth: 4,
                max_instance_len: 64,
                ..Limits::default()
            };
            if let Ok(reader) = StepReader::new(input.as_bytes(), limits) {
                let _ = reader.count();
            }
        }
    }
}
