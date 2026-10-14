use std::io::{BufRead, ErrorKind};

use super::error::{Position, StepError, StepErrorKind};

/// A lexical token of a Part 21 file.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Token {
    /// An entity, type or section keyword, such as `IFCWALL` or `ISO-10303-21`.
    Keyword(String),
    InstanceName(u64),
    Integer(i64),
    Real(f64),
    String(String),
    Enumeration(String),
    Binary(String),
    Null,
    Derived,
    OpenParen,
    CloseParen,
    Comma,
    Semicolon,
    Equals,
    End,
}

impl Token {
    /// A description for error messages, without the token's content (SEC-03).
    pub(super) fn describe(&self) -> &'static str {
        match self {
            Self::Keyword(_) => "a keyword",
            Self::InstanceName(_) => "an instance name",
            Self::Integer(_) => "an integer",
            Self::Real(_) => "a real",
            Self::String(_) => "a string",
            Self::Enumeration(_) => "an enumeration",
            Self::Binary(_) => "a binary",
            Self::Null => "`$`",
            Self::Derived => "`*`",
            Self::OpenParen => "`(`",
            Self::CloseParen => "`)`",
            Self::Comma => "`,`",
            Self::Semicolon => "`;`",
            Self::Equals => "`=`",
            Self::End => "end of input",
        }
    }
}

/// A token with where it starts.
#[derive(Debug)]
pub(super) struct Spanned {
    pub(super) token: Token,
    pub(super) position: Position,
    /// Byte offset of the token's first byte.
    pub(super) offset: u64,
}

/// Splits a byte stream into tokens, reading one byte at a time from the buffer.
pub(super) struct Lexer<R> {
    reader: R,
    position: Position,
    offset: u64,
    max_token_len: usize,
}

impl<R: BufRead> Lexer<R> {
    pub(super) fn new(reader: R, max_token_len: usize) -> Self {
        Self {
            reader,
            position: Position::START,
            offset: 0,
            max_token_len,
        }
    }

    /// Reads the next token, skipping whitespace and comments.
    pub(super) fn next_token(&mut self) -> Result<Spanned, StepError> {
        self.skip_whitespace()?;
        let position = self.position;
        let offset = self.offset;
        let token = self.token(position)?;
        Ok(Spanned {
            token,
            position,
            offset,
        })
    }

    fn token(&mut self, start: Position) -> Result<Token, StepError> {
        let Some(byte) = self.peek()? else {
            return Ok(Token::End);
        };
        let punctuation = match byte {
            b'(' => Token::OpenParen,
            b')' => Token::CloseParen,
            b',' => Token::Comma,
            b';' => Token::Semicolon,
            b'=' => Token::Equals,
            b'$' => Token::Null,
            b'*' => Token::Derived,
            b'\'' => return self.string(start),
            b'"' => return self.binary(start),
            b'.' => return self.enumeration(start),
            b'#' => return self.instance_name(start),
            b'+' | b'-' | b'0'..=b'9' => return self.number(start),
            b'A'..=b'Z' | b'a'..=b'z' | b'_' | b'!' => return self.keyword(byte, start),
            _ => return Err(self.error(StepErrorKind::UnexpectedByte(byte))),
        };
        self.bump(byte);
        Ok(punctuation)
    }

    fn skip_whitespace(&mut self) -> Result<(), StepError> {
        while let Some(byte) = self.peek()? {
            match byte {
                b' ' | b'\t' | b'\r' | b'\n' => self.bump(byte),
                b'/' => self.comment()?,
                _ => break,
            }
        }
        Ok(())
    }

    fn comment(&mut self) -> Result<(), StepError> {
        let start = self.position;
        self.bump(b'/');
        if self.peek()? != Some(b'*') {
            return Err(StepError::new(StepErrorKind::UnexpectedByte(b'/'), start));
        }
        self.bump(b'*');
        let mut after_star = false;
        loop {
            let byte = self.expect_byte("`*/` closing the comment")?;
            self.bump(byte);
            if after_star && byte == b'/' {
                return Ok(());
            }
            after_star = byte == b'*';
        }
    }

    fn string(&mut self, start: Position) -> Result<Token, StepError> {
        self.bump(b'\'');
        let mut text = String::new();
        loop {
            let position = self.position;
            let byte = self.expect_byte("`'` closing the string")?;
            self.bump(byte);
            match byte {
                b'\'' if self.peek()? != Some(b'\'') => return Ok(Token::String(text)),
                b'\'' => self.bump(b'\''),
                // Part 21 ignores line breaks inside strings, so long strings can be wrapped.
                b'\r' | b'\n' => continue,
                b' '..=b'~' => {}
                _ => {
                    return Err(StepError::new(
                        StepErrorKind::UnexpectedByte(byte),
                        position,
                    ));
                }
            }
            self.push(&mut text, byte, start)?;
        }
    }

    fn binary(&mut self, start: Position) -> Result<Token, StepError> {
        self.bump(b'"');
        let mut digits = String::new();
        loop {
            let byte = self.expect_byte("`\"` closing the binary")?;
            self.bump(byte);
            match byte {
                b'"' => break,
                b'0'..=b'9' | b'A'..=b'F' => self.push(&mut digits, byte, start)?,
                _ => return Err(StepError::new(StepErrorKind::InvalidBinary, start)),
            }
        }
        // The first digit counts the unused bits of the last byte, so it is between 0 and 3.
        if matches!(digits.as_bytes().first(), Some(b'0'..=b'3')) {
            Ok(Token::Binary(digits))
        } else {
            Err(StepError::new(StepErrorKind::InvalidBinary, start))
        }
    }

    fn enumeration(&mut self, start: Position) -> Result<Token, StepError> {
        self.bump(b'.');
        let mut name = String::new();
        loop {
            match self.expect_byte("`.` closing the enumeration")? {
                b'.' if !name.is_empty() => {
                    self.bump(b'.');
                    return Ok(Token::Enumeration(name));
                }
                byte @ (b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_') => {
                    self.bump(byte);
                    self.push(&mut name, byte, start)?;
                }
                byte => return Err(self.error(StepErrorKind::UnexpectedByte(byte))),
            }
        }
    }

    fn instance_name(&mut self, start: Position) -> Result<Token, StepError> {
        self.bump(b'#');
        let mut digits = String::new();
        self.require_digits(&mut digits, start, "digits after `#`")?;
        digits
            .parse()
            .map(Token::InstanceName)
            .map_err(|_| StepError::new(StepErrorKind::NumberOutOfRange, start))
    }

    fn number(&mut self, start: Position) -> Result<Token, StepError> {
        let mut text = String::new();
        if let Some(sign @ (b'+' | b'-')) = self.peek()? {
            self.bump(sign);
            self.push(&mut text, sign, start)?;
        }
        self.require_digits(&mut text, start, "digits")?;
        if self.peek()? != Some(b'.') {
            return text
                .parse()
                .map(Token::Integer)
                .map_err(|_| StepError::new(StepErrorKind::NumberOutOfRange, start));
        }
        self.bump(b'.');
        self.push(&mut text, b'.', start)?;
        self.digits(&mut text, start)?;
        if let Some(exponent @ (b'E' | b'e')) = self.peek()? {
            self.bump(exponent);
            self.push(&mut text, exponent, start)?;
            if let Some(sign @ (b'+' | b'-')) = self.peek()? {
                self.bump(sign);
                self.push(&mut text, sign, start)?;
            }
            self.require_digits(&mut text, start, "exponent digits")?;
        }
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(Token::Real(value)),
            _ => Err(StepError::new(StepErrorKind::NumberOutOfRange, start)),
        }
    }

    fn keyword(&mut self, first: u8, start: Position) -> Result<Token, StepError> {
        self.bump(first);
        let mut name = String::new();
        self.push(&mut name, first, start)?;
        // A user-defined keyword is `!` followed by a name, which starts with a letter.
        if first == b'!' {
            match self.peek()? {
                Some(byte @ (b'A'..=b'Z' | b'a'..=b'z')) => {
                    self.bump(byte);
                    self.push(&mut name, byte, start)?;
                }
                next => return Err(self.unexpected(next, "a name after `!`")),
            }
        }
        // Section keywords such as `ISO-10303-21` contain hyphens, entity names never do.
        while let Some(byte @ (b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-')) =
            self.peek()?
        {
            self.bump(byte);
            self.push(&mut name, byte, start)?;
        }
        Ok(Token::Keyword(name))
    }

    /// Appends the decimal digits that follow, failing if there are none.
    fn require_digits(
        &mut self,
        text: &mut String,
        start: Position,
        expected: &'static str,
    ) -> Result<(), StepError> {
        if self.digits(text, start)? > 0 {
            return Ok(());
        }
        let next = self.peek()?;
        Err(self.unexpected(next, expected))
    }

    /// Appends the decimal digits that follow and returns how many there were.
    fn digits(&mut self, text: &mut String, start: Position) -> Result<usize, StepError> {
        let mut count = 0;
        while let Some(byte @ b'0'..=b'9') = self.peek()? {
            self.bump(byte);
            self.push(text, byte, start)?;
            count += 1;
        }
        Ok(count)
    }

    /// Appends an ASCII byte to a token, failing once the token would exceed its limit.
    fn push(&self, text: &mut String, byte: u8, start: Position) -> Result<(), StepError> {
        if text.len() >= self.max_token_len {
            let limit = self.max_token_len;
            return Err(StepError::new(StepErrorKind::TokenTooLong { limit }, start));
        }
        text.push(char::from(byte));
        Ok(())
    }

    /// The error for the `next` byte, or the end of the input, when it is not `expected`.
    fn unexpected(&self, next: Option<u8>, expected: &'static str) -> StepError {
        match next {
            Some(byte) => self.error(StepErrorKind::UnexpectedByte(byte)),
            None => self.end_of_input(expected),
        }
    }

    fn end_of_input(&self, expected: &'static str) -> StepError {
        self.error(StepErrorKind::UnexpectedToken {
            expected,
            found: Token::End.describe(),
        })
    }

    fn error(&self, kind: StepErrorKind) -> StepError {
        StepError::new(kind, self.position)
    }

    /// The next byte, failing at the end of the input.
    fn expect_byte(&mut self, expected: &'static str) -> Result<u8, StepError> {
        match self.peek()? {
            Some(byte) => Ok(byte),
            None => Err(self.end_of_input(expected)),
        }
    }

    fn peek(&mut self) -> Result<Option<u8>, StepError> {
        loop {
            match self.reader.fill_buf() {
                Ok(buffer) => return Ok(buffer.first().copied()),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(self.error(StepErrorKind::Io(error.kind()))),
            }
        }
    }

    /// Consumes `byte`, which [`peek`](Self::peek) just returned.
    fn bump(&mut self, byte: u8) {
        self.reader.consume(1);
        self.offset += 1;
        if byte == b'\n' {
            self.position.line += 1;
            self.position.column = 1;
        } else {
            self.position.column += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    fn tokens(input: &str) -> Result<Vec<Token>, StepError> {
        tokens_with_limit(input, usize::MAX)
    }

    fn tokens_with_limit(input: &str, max_token_len: usize) -> Result<Vec<Token>, StepError> {
        let mut lexer = Lexer::new(input.as_bytes(), max_token_len);
        let mut tokens = Vec::new();
        loop {
            match lexer.next_token()?.token {
                Token::End => return Ok(tokens),
                token => tokens.push(token),
            }
        }
    }

    fn error(input: &str) -> StepError {
        tokens(input).expect_err("input should be rejected")
    }

    fn at(line: u64, column: u64) -> Position {
        Position { line, column }
    }

    fn string(text: &str) -> Token {
        Token::String(text.to_owned())
    }

    #[test]
    fn reads_punctuation() {
        assert_eq!(
            tokens("( ) , ; = $ *").unwrap(),
            [
                Token::OpenParen,
                Token::CloseParen,
                Token::Comma,
                Token::Semicolon,
                Token::Equals,
                Token::Null,
                Token::Derived,
            ]
        );
    }

    #[test]
    fn reads_keywords_with_section_hyphens_and_user_defined_names() {
        assert_eq!(
            tokens("ISO-10303-21 IFCWALL _x1 !USER").unwrap(),
            [
                Token::Keyword("ISO-10303-21".to_owned()),
                Token::Keyword("IFCWALL".to_owned()),
                Token::Keyword("_x1".to_owned()),
                Token::Keyword("!USER".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_user_defined_keywords_without_a_name() {
        assert_eq!(
            error("! "),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b' '),
                position: at(1, 2),
            }
        );
        assert_eq!(
            error("!1"),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b'1'),
                position: at(1, 2),
            }
        );
        assert_eq!(
            error("!").kind,
            StepErrorKind::UnexpectedToken {
                expected: "a name after `!`",
                found: "end of input",
            }
        );
    }

    #[test]
    fn reads_instance_names() {
        assert_eq!(
            tokens("#0 #18446744073709551615").unwrap(),
            [Token::InstanceName(0), Token::InstanceName(u64::MAX)]
        );
    }

    #[test]
    fn rejects_malformed_instance_names() {
        assert_eq!(
            error("#18446744073709551616"),
            StepError {
                kind: StepErrorKind::NumberOutOfRange,
                position: at(1, 1),
            }
        );
        assert_eq!(
            error("#x"),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b'x'),
                position: at(1, 2),
            }
        );
        assert_eq!(
            error("#").kind,
            StepErrorKind::UnexpectedToken {
                expected: "digits after `#`",
                found: "end of input",
            }
        );
    }

    #[test]
    fn reads_integers() {
        assert_eq!(
            tokens("0 42 +7 -9223372036854775808").unwrap(),
            [
                Token::Integer(0),
                Token::Integer(42),
                Token::Integer(7),
                Token::Integer(i64::MIN),
            ]
        );
    }

    #[test]
    fn reads_reals_in_every_form() {
        assert_eq!(
            tokens("1. -2.5 3.E2 4.25e-2 +5.0E+1").unwrap(),
            [
                Token::Real(1.0),
                Token::Real(-2.5),
                Token::Real(300.0),
                Token::Real(0.0425),
                Token::Real(50.0),
            ]
        );
    }

    #[test]
    fn rejects_numbers_out_of_range() {
        for input in ["9223372036854775808", "1.E999", "-1.E999"] {
            assert_eq!(
                error(input),
                StepError {
                    kind: StepErrorKind::NumberOutOfRange,
                    position: at(1, 1),
                },
                "{input}"
            );
        }
    }

    #[test]
    fn rejects_numbers_without_digits() {
        assert_eq!(
            error("-x"),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b'x'),
                position: at(1, 2),
            }
        );
        assert_eq!(
            error("1.E;"),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b';'),
                position: at(1, 4),
            }
        );
        assert_eq!(
            error("1.E+").kind,
            StepErrorKind::UnexpectedToken {
                expected: "exponent digits",
                found: "end of input",
            }
        );
    }

    #[test]
    fn reads_strings_with_doubled_quotes_and_raw_directives() {
        assert_eq!(
            tokens(r"'' 'it''s' '\X2\00E9\X0\'").unwrap(),
            [string(""), string("it's"), string(r"\X2\00E9\X0\")]
        );
    }

    #[test]
    fn ignores_line_breaks_inside_strings() {
        assert_eq!(tokens("'ab\r\ncd'").unwrap(), [string("abcd")]);
    }

    #[test]
    fn rejects_non_printable_bytes_in_strings() {
        assert_eq!(
            error("'a\tb'"),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b'\t'),
                position: at(1, 3),
            }
        );
        assert_eq!(
            error("'abc\u{e9}'"),
            StepError {
                kind: StepErrorKind::UnexpectedByte(0xC3),
                position: at(1, 5),
            }
        );
    }

    #[test]
    fn rejects_unterminated_strings() {
        assert_eq!(
            error("'abc"),
            StepError {
                kind: StepErrorKind::UnexpectedToken {
                    expected: "`'` closing the string",
                    found: "end of input",
                },
                position: at(1, 5),
            }
        );
    }

    #[test]
    fn reads_enumerations() {
        assert_eq!(
            tokens(".T. .ELEMENT_2.").unwrap(),
            [
                Token::Enumeration("T".to_owned()),
                Token::Enumeration("ELEMENT_2".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_malformed_enumerations() {
        assert_eq!(
            error(".."),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b'.'),
                position: at(1, 2),
            }
        );
        assert_eq!(
            error(".A B."),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b' '),
                position: at(1, 3),
            }
        );
        assert_eq!(
            error(".AB").kind,
            StepErrorKind::UnexpectedToken {
                expected: "`.` closing the enumeration",
                found: "end of input",
            }
        );
    }

    #[test]
    fn reads_binaries() {
        assert_eq!(
            tokens(r#""0" "3FA09""#).unwrap(),
            [
                Token::Binary("0".to_owned()),
                Token::Binary("3FA09".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_malformed_binaries() {
        for input in [r#""""#, r#""4F""#, r#""0g""#, r#"  "0a""#] {
            assert_eq!(error(input).kind, StepErrorKind::InvalidBinary, "{input}");
        }
        assert_eq!(error(r#"  "0a""#).position, at(1, 3));
        assert_eq!(
            error(r#""0"#).kind,
            StepErrorKind::UnexpectedToken {
                expected: "`\"` closing the binary",
                found: "end of input",
            }
        );
    }

    #[test]
    fn skips_comments_and_whitespace() {
        assert_eq!(
            tokens("/* a ** comment / */ (\t/**/)\r\n/*\n*/;").unwrap(),
            [Token::OpenParen, Token::CloseParen, Token::Semicolon]
        );
    }

    #[test]
    fn rejects_malformed_comments() {
        assert_eq!(
            error(" /x"),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b'/'),
                position: at(1, 2),
            }
        );
        assert_eq!(
            error("/* never closed *").kind,
            StepErrorKind::UnexpectedToken {
                expected: "`*/` closing the comment",
                found: "end of input",
            }
        );
    }

    #[test]
    fn rejects_unexpected_bytes_between_tokens() {
        assert_eq!(
            error("(\n  @"),
            StepError {
                kind: StepErrorKind::UnexpectedByte(b'@'),
                position: at(2, 3),
            }
        );
    }

    #[test]
    fn reports_positions_and_offsets_across_lines() {
        let mut lexer = Lexer::new("A\r\n  #1".as_bytes(), usize::MAX);
        let first = lexer.next_token().unwrap();
        assert_eq!((first.position, first.offset), (at(1, 1), 0));
        let second = lexer.next_token().unwrap();
        assert_eq!(second.token, Token::InstanceName(1));
        assert_eq!((second.position, second.offset), (at(2, 3), 5));
        let end = lexer.next_token().unwrap();
        assert_eq!(
            (end.token, end.position, end.offset),
            (Token::End, at(2, 5), 7)
        );
    }

    #[test]
    fn enforces_the_token_length_limit_on_every_token_kind() {
        let accepted = [
            "'abcd'", "\"0ABC\"", ".ABCD.", "ABCD", "#1234", "1234", "-1.5",
        ];
        for input in accepted {
            assert!(tokens_with_limit(input, 4).is_ok(), "{input}");
        }
        let rejected = [
            "'abcde'",
            "\"0ABCD\"",
            ".ABCDE.",
            "ABCDE",
            "#12345",
            "12345",
            "-1.50",
            "-123.",
            "1.E+1",
            "-1234",
        ];
        for input in rejected {
            assert_eq!(
                tokens_with_limit(input, 4).unwrap_err(),
                StepError {
                    kind: StepErrorKind::TokenTooLong { limit: 4 },
                    position: at(1, 1),
                },
                "{input}"
            );
        }
    }

    #[test]
    fn describes_every_token_without_its_content() {
        let cases = [
            (Token::Keyword("SECRET".to_owned()), "a keyword"),
            (Token::InstanceName(1), "an instance name"),
            (Token::Integer(1), "an integer"),
            (Token::Real(1.0), "a real"),
            (string("secret"), "a string"),
            (Token::Enumeration("T".to_owned()), "an enumeration"),
            (Token::Binary("0".to_owned()), "a binary"),
            (Token::Null, "`$`"),
            (Token::Derived, "`*`"),
            (Token::OpenParen, "`(`"),
            (Token::CloseParen, "`)`"),
            (Token::Comma, "`,`"),
            (Token::Semicolon, "`;`"),
            (Token::Equals, "`=`"),
            (Token::End, "end of input"),
        ];
        for (token, description) in cases {
            assert_eq!(token.describe(), description);
        }
    }

    /// A reader that returns one byte per call and fails once with `kind` after `before` bytes.
    struct FailingReader {
        data: &'static [u8],
        before: usize,
        kind: Option<ErrorKind>,
    }

    impl FailingReader {
        fn buffered(data: &'static [u8], before: usize, kind: ErrorKind) -> impl BufRead {
            io::BufReader::with_capacity(
                1,
                Self {
                    data,
                    before,
                    kind: Some(kind),
                },
            )
        }
    }

    impl io::Read for FailingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.before == 0
                && let Some(kind) = self.kind.take()
            {
                return Err(kind.into());
            }
            let count = buffer.len().min(self.data.len()).min(1);
            buffer[..count].copy_from_slice(&self.data[..count]);
            self.data = &self.data[count..];
            self.before = self.before.saturating_sub(count);
            Ok(count)
        }
    }

    #[test]
    fn retries_interrupted_reads() {
        let reader = FailingReader::buffered(b"AB C", 1, ErrorKind::Interrupted);
        let mut lexer = Lexer::new(reader, usize::MAX);
        assert_eq!(
            lexer.next_token().unwrap().token,
            Token::Keyword("AB".to_owned())
        );
        assert_eq!(
            lexer.next_token().unwrap().token,
            Token::Keyword("C".to_owned())
        );
    }

    #[test]
    fn reports_read_failures_where_they_happen() {
        let reader = FailingReader::buffered(b"AB C", 3, ErrorKind::BrokenPipe);
        let mut lexer = Lexer::new(reader, usize::MAX);
        assert_eq!(
            lexer.next_token().unwrap().token,
            Token::Keyword("AB".to_owned())
        );
        assert_eq!(
            lexer.next_token().unwrap_err(),
            StepError {
                kind: StepErrorKind::Io(ErrorKind::BrokenPipe),
                position: at(1, 4),
            }
        );
    }

    #[test]
    fn reports_read_failures_while_looking_for_digits() {
        let reader = FailingReader::buffered(b"#1", 1, ErrorKind::BrokenPipe);
        let mut lexer = Lexer::new(reader, usize::MAX);
        assert_eq!(
            lexer.next_token().unwrap_err().kind,
            StepErrorKind::Io(ErrorKind::BrokenPipe)
        );
    }
}
