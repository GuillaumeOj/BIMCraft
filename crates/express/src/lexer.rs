//! Splits EXPRESS source into tokens (ISO 10303-11 §7).

use std::iter::Peekable;
use std::str::CharIndices;

use crate::error::{Error, ErrorKind, Position};

/// A lexical token.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    /// An identifier or a keyword, as written; keywords are matched without regard to case.
    Ident(String),
    Integer(i64),
    Real(f64),
    /// A simple or encoded string literal, decoded.
    String(String),
    /// A binary literal: its `0` and `1` digits, without the `%`.
    Binary(String),
    Symbol(Symbol),
    End,
}

/// Punctuation and operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Symbol {
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Semicolon,
    Colon,
    Dot,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    Assign,
    InstanceEqual,
    InstanceNotEqual,
    Plus,
    Minus,
    Star,
    Slash,
    Power,
    Concat,
    Pipe,
    Backslash,
    Question,
    QueryFrom,
}

impl Symbol {
    pub(crate) const fn text(self) -> &'static str {
        match self {
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::LeftBracket => "[",
            Self::RightBracket => "]",
            Self::LeftBrace => "{",
            Self::RightBrace => "}",
            Self::Comma => ",",
            Self::Semicolon => ";",
            Self::Colon => ":",
            Self::Dot => ".",
            Self::Equal => "=",
            Self::NotEqual => "<>",
            Self::Less => "<",
            Self::Greater => ">",
            Self::LessEqual => "<=",
            Self::GreaterEqual => ">=",
            Self::Assign => ":=",
            Self::InstanceEqual => ":=:",
            Self::InstanceNotEqual => ":<>:",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Power => "**",
            Self::Concat => "||",
            Self::Pipe => "|",
            Self::Backslash => "\\",
            Self::Question => "?",
            Self::QueryFrom => "<*",
        }
    }
}

impl Token {
    /// A description for error messages.
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Ident(name) => format!("`{name}`"),
            Self::Integer(_) => "an integer".into(),
            Self::Real(_) => "a real".into(),
            Self::String(_) => "a string".into(),
            Self::Binary(_) => "a binary".into(),
            Self::Symbol(symbol) => format!("`{}`", symbol.text()),
            Self::End => "end of input".into(),
        }
    }
}

/// A token with where it starts.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Spanned {
    pub(crate) token: Token,
    pub(crate) position: Position,
}

/// Splits `source` into tokens, skipping white space and remarks. The last token is
/// [`Token::End`].
pub(crate) fn tokenize(source: &str) -> Result<Vec<Spanned>, Error> {
    let mut lexer = Lexer {
        chars: source.char_indices().peekable(),
        source,
        position: Position::START,
    };
    let mut tokens = Vec::new();
    loop {
        lexer.skip_blanks()?;
        let position = lexer.position;
        let token = lexer.token(position)?;
        let end = token == Token::End;
        tokens.push(Spanned { token, position });
        if end {
            return Ok(tokens);
        }
    }
}

struct Lexer<'a> {
    chars: Peekable<CharIndices<'a>>,
    source: &'a str,
    position: Position,
}

impl Lexer<'_> {
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c)
    }

    /// The character after the next one.
    fn peek_second(&self) -> Option<char> {
        let mut ahead = self.chars.clone();
        ahead.next();
        ahead.next().map(|(_, c)| c)
    }

    fn bump(&mut self) -> Option<char> {
        let (_, c) = self.chars.next()?;
        if c == '\n' {
            self.position.line += 1;
            self.position.column = 1;
        } else {
            self.position.column += 1;
        }
        Some(c)
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn offset(&mut self) -> usize {
        self.chars.peek().map_or(self.source.len(), |&(i, _)| i)
    }

    /// Skips white space, embedded remarks `(* … *)` (which nest) and tail remarks `-- …`.
    fn skip_blanks(&mut self) -> Result<(), Error> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('(') if self.peek_second() == Some('*') => self.embedded_remark()?,
                Some('-') if self.peek_second() == Some('-') => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn embedded_remark(&mut self) -> Result<(), Error> {
        let start = self.position;
        self.bump();
        self.bump();
        let mut depth = 1_usize;
        while depth > 0 {
            match self.bump() {
                Some('(') if self.eat('*') => depth += 1,
                Some('*') if self.eat(')') => depth -= 1,
                Some(_) => {}
                None => return Err(Error::at(ErrorKind::UnterminatedRemark, start)),
            }
        }
        Ok(())
    }

    fn token(&mut self, start: Position) -> Result<Token, Error> {
        let Some(c) = self.peek() else {
            return Ok(Token::End);
        };
        match c {
            'a'..='z' | 'A'..='Z' => return Ok(self.identifier()),
            '0'..='9' => return self.number(start),
            '\'' => return self.simple_string(start),
            '"' => return self.encoded_string(start),
            '%' => return self.binary(start),
            _ => {}
        }
        self.bump();
        let symbol = match c {
            '(' => Symbol::LeftParen,
            ')' => Symbol::RightParen,
            '[' => Symbol::LeftBracket,
            ']' => Symbol::RightBracket,
            '{' => Symbol::LeftBrace,
            '}' => Symbol::RightBrace,
            ',' => Symbol::Comma,
            ';' => Symbol::Semicolon,
            '.' => Symbol::Dot,
            '=' => Symbol::Equal,
            '+' => Symbol::Plus,
            '-' => Symbol::Minus,
            '/' => Symbol::Slash,
            '\\' => Symbol::Backslash,
            '?' => Symbol::Question,
            '*' if self.eat('*') => Symbol::Power,
            '*' => Symbol::Star,
            '|' if self.eat('|') => Symbol::Concat,
            '|' => Symbol::Pipe,
            '<' if self.eat('>') => Symbol::NotEqual,
            '<' if self.eat('=') => Symbol::LessEqual,
            '<' if self.eat('*') => Symbol::QueryFrom,
            '<' => Symbol::Less,
            '>' if self.eat('=') => Symbol::GreaterEqual,
            '>' => Symbol::Greater,
            ':' => self.colon(),
            _ => return Err(Error::at(ErrorKind::UnexpectedCharacter(c), start)),
        };
        Ok(Token::Symbol(symbol))
    }

    /// `:`, `:=`, `:=:` or `:<>:`, after the first `:`.
    fn colon(&mut self) -> Symbol {
        if self.eat('=') {
            return if self.eat(':') {
                Symbol::InstanceEqual
            } else {
                Symbol::Assign
            };
        }
        let mut ahead = self.chars.clone();
        let instance_not_equal = [Some('<'), Some('>'), Some(':')]
            .iter()
            .all(|&expected| ahead.next().map(|(_, c)| c) == expected);
        if instance_not_equal {
            for _ in 0..3 {
                self.bump();
            }
            Symbol::InstanceNotEqual
        } else {
            Symbol::Colon
        }
    }

    fn identifier(&mut self) -> Token {
        let start = self.offset();
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.bump();
        }
        let end = self.offset();
        Token::Ident(self.source[start..end].to_owned())
    }

    fn digits(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
    }

    /// An integer, or a real: digits, a dot, optional digits and an optional exponent.
    fn number(&mut self, start: Position) -> Result<Token, Error> {
        let begin = self.offset();
        self.digits();
        let real = self.eat('.');
        if real {
            self.digits();
            let mut ahead = self.chars.clone();
            if matches!(ahead.next(), Some((_, 'e' | 'E'))) {
                let mut sign_or_digit = ahead.next().map(|(_, c)| c);
                if matches!(sign_or_digit, Some('+' | '-')) {
                    sign_or_digit = ahead.next().map(|(_, c)| c);
                }
                if sign_or_digit.is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                    if !self.eat('+') {
                        self.eat('-');
                    }
                    self.digits();
                }
            }
        }
        let text = &self.source[begin..self.offset()];
        let out_of_range = || Error::at(ErrorKind::NumberOutOfRange, start);
        if real {
            text.parse()
                .ok()
                .filter(|value: &f64| value.is_finite())
                .map(Token::Real)
                .ok_or_else(out_of_range)
        } else {
            text.parse().map(Token::Integer).map_err(|_| out_of_range())
        }
    }

    /// `'…'`, where `''` stands for one quote.
    fn simple_string(&mut self, start: Position) -> Result<Token, Error> {
        self.bump();
        let mut value = String::new();
        loop {
            match self.bump() {
                Some('\'') if self.eat('\'') => value.push('\''),
                Some('\'') => return Ok(Token::String(value)),
                Some(c) => value.push(c),
                None => return Err(Error::at(ErrorKind::UnterminatedString, start)),
            }
        }
    }

    /// `"…"`, groups of eight hexadecimal digits, each the ISO 10646 code of one character.
    fn encoded_string(&mut self, start: Position) -> Result<Token, Error> {
        self.bump();
        let begin = self.offset();
        loop {
            match self.bump() {
                Some('"') => break,
                Some(_) => {}
                None => return Err(Error::at(ErrorKind::UnterminatedString, start)),
            }
        }
        // The closing quote is one byte.
        let digits = &self.source[begin..self.offset() - 1];
        let invalid = || Error::at(ErrorKind::InvalidEncodedString, start);
        if !digits.len().is_multiple_of(8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let mut value = String::new();
        for group in digits.as_bytes().chunks(8) {
            let character = std::str::from_utf8(group)
                .ok()
                .and_then(|group| u32::from_str_radix(group, 16).ok())
                .and_then(char::from_u32)
                .ok_or_else(invalid)?;
            value.push(character);
        }
        Ok(Token::String(value))
    }

    /// `%` followed by binary digits.
    fn binary(&mut self, start: Position) -> Result<Token, Error> {
        self.bump();
        let begin = self.offset();
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
            self.bump();
        }
        let digits = &self.source[begin..self.offset()];
        if digits.is_empty() || !digits.chars().all(|c| c == '0' || c == '1') {
            return Err(Error::at(ErrorKind::InvalidBinary, start));
        }
        Ok(Token::Binary(digits.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn tokens(source: &str) -> Vec<Token> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|spanned| spanned.token)
            .collect()
    }

    fn error(source: &str) -> (ErrorKind, Position) {
        let error = tokenize(source).unwrap_err();
        (error.kind, error.position.unwrap())
    }

    fn ident(name: &str) -> Token {
        Token::Ident(name.into())
    }

    #[test]
    fn reads_identifiers_and_numbers() {
        assert_eq!(
            tokens("IfcWall end_entity x_1 42 1.5 1. 1.E-5 2.5e+3 3.0E2"),
            [
                ident("IfcWall"),
                ident("end_entity"),
                ident("x_1"),
                Token::Integer(42),
                Token::Real(1.5),
                Token::Real(1.0),
                Token::Real(1.0e-5),
                Token::Real(2.5e3),
                Token::Real(300.0),
                Token::End,
            ]
        );
    }

    #[test]
    fn an_exponent_needs_digits() {
        // `1.E` is the real 1 followed by the identifier `E`.
        assert_eq!(
            tokens("1.E 1.e+"),
            [
                Token::Real(1.0),
                ident("E"),
                Token::Real(1.0),
                ident("e"),
                Token::Symbol(Symbol::Plus),
                Token::End,
            ]
        );
    }

    #[test]
    fn reads_every_symbol() {
        let source = "( ) [ ] { } , ; : . = <> < > <= >= := :=: :<>: + - * / ** || | \\ ? <*";
        let symbols: Vec<&str> = tokens(source)
            .into_iter()
            .filter_map(|token| match token {
                Token::Symbol(symbol) => Some(symbol.text()),
                _ => None,
            })
            .collect();
        assert_eq!(symbols.join(" "), source);
    }

    #[test]
    fn a_colon_followed_by_less_is_not_always_instance_not_equal() {
        assert_eq!(
            tokens(":<> :<"),
            [
                Token::Symbol(Symbol::Colon),
                Token::Symbol(Symbol::NotEqual),
                Token::Symbol(Symbol::Colon),
                Token::Symbol(Symbol::Less),
                Token::End,
            ]
        );
    }

    #[test]
    fn reads_strings() {
        assert_eq!(
            tokens(r#"'it''s' '' "00000041000000E9" "#),
            [
                Token::String("it's".into()),
                Token::String(String::new()),
                Token::String("Aé".into()),
                Token::End,
            ]
        );
    }

    #[test]
    fn reads_binaries() {
        assert_eq!(tokens("%0101"), [Token::Binary("0101".into()), Token::End]);
    }

    #[test]
    fn skips_remarks() {
        let source = "a (* one (* nested *) still *) b -- tail\nc (**) d --";
        assert_eq!(
            tokens(source),
            [ident("a"), ident("b"), ident("c"), ident("d"), Token::End]
        );
    }

    #[test]
    fn tracks_positions() {
        let spanned = tokenize("a\n (*é*) b c").unwrap();
        let positions: Vec<(usize, usize)> = spanned
            .iter()
            .map(|token| (token.position.line, token.position.column))
            .collect();
        assert_eq!(positions, [(1, 1), (2, 8), (2, 10), (2, 11)]);
    }

    #[test]
    fn rejects_malformed_input() {
        let at = |line, column| Position { line, column };
        assert_eq!(
            error("a ~"),
            (ErrorKind::UnexpectedCharacter('~'), at(1, 3))
        );
        assert_eq!(
            error("x _"),
            (ErrorKind::UnexpectedCharacter('_'), at(1, 3))
        );
        assert_eq!(error("'abc"), (ErrorKind::UnterminatedString, at(1, 1)));
        assert_eq!(error("\"0000"), (ErrorKind::UnterminatedString, at(1, 1)));
        assert_eq!(
            error("\"0000\""),
            (ErrorKind::InvalidEncodedString, at(1, 1))
        );
        assert_eq!(
            error("\"0000004G\""),
            (ErrorKind::InvalidEncodedString, at(1, 1))
        );
        assert_eq!(
            error("\"0000D800\""),
            (ErrorKind::InvalidEncodedString, at(1, 1))
        );
        assert_eq!(error("%"), (ErrorKind::InvalidBinary, at(1, 1)));
        assert_eq!(error("%012"), (ErrorKind::InvalidBinary, at(1, 1)));
        assert_eq!(
            error("\n (* (* *)"),
            (ErrorKind::UnterminatedRemark, at(2, 2))
        );
        assert_eq!(
            error("99999999999999999999"),
            (ErrorKind::NumberOutOfRange, at(1, 1))
        );
        let huge = format!("1{}.", "0".repeat(400));
        assert_eq!(error(&huge), (ErrorKind::NumberOutOfRange, at(1, 1)));
    }

    #[test]
    fn describes_tokens() {
        let cases = [
            (ident("x"), "`x`"),
            (Token::Integer(1), "an integer"),
            (Token::Real(1.0), "a real"),
            (Token::String(String::new()), "a string"),
            (Token::Binary("1".into()), "a binary"),
            (Token::Symbol(Symbol::Assign), "`:=`"),
            (Token::End, "end of input"),
        ];
        for (token, description) in cases {
            assert_eq!(token.describe(), description);
        }
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Any text is either split into tokens or rejected; nothing panics.
        #[test]
        fn never_panics(source in "\\PC*") {
            let _ = tokenize(&source);
        }

        /// Integers read back as written.
        #[test]
        fn integers_round_trip(value in 0..=i64::MAX) {
            prop_assert_eq!(tokens(&value.to_string()), [Token::Integer(value), Token::End]);
        }

        /// Simple strings read back as written, quotes doubled.
        #[test]
        fn strings_round_trip(value in "[ -~\n\t]*") {
            let source = format!("'{}'", value.replace('\'', "''"));
            prop_assert_eq!(tokens(&source), [Token::String(value), Token::End]);
        }
    }
}
