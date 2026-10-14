//! Recursive-descent parser for EXPRESS (ISO 10303-11), building the [`ast`](crate::ast).
//!
//! Every construct that nests (expressions, statements, types, declarations) counts against
//! [`MAX_DEPTH`](crate::MAX_DEPTH), so hostile input cannot overflow the stack.

mod declarations;
mod expressions;
mod statements;
mod types;

use crate::ast::{Ident, Schema};
use crate::error::{Error, ErrorKind, Position};
use crate::lexer::{Spanned, Symbol, Token, tokenize};
use crate::{MAX_DEPTH, MAX_INPUT_LEN};

/// Words that cannot name anything (ISO 10303-11 §7.2), apart from built-in functions and
/// procedures, which are parsed as calls.
const RESERVED: &[&str] = &[
    "ABSTRACT",
    "AGGREGATE",
    "ALIAS",
    "AND",
    "ANDOR",
    "ARRAY",
    "AS",
    "BAG",
    "BASED_ON",
    "BEGIN",
    "BINARY",
    "BOOLEAN",
    "BY",
    "CASE",
    "CONSTANT",
    "CONST_E",
    "DERIVE",
    "DIV",
    "ELSE",
    "END",
    "END_ALIAS",
    "END_CASE",
    "END_CONSTANT",
    "END_ENTITY",
    "END_FUNCTION",
    "END_IF",
    "END_LOCAL",
    "END_PROCEDURE",
    "END_REPEAT",
    "END_RULE",
    "END_SCHEMA",
    "END_SUBTYPE_CONSTRAINT",
    "END_TYPE",
    "ENTITY",
    "ENUMERATION",
    "ESCAPE",
    "EXTENSIBLE",
    "FALSE",
    "FIXED",
    "FOR",
    "FROM",
    "FUNCTION",
    "GENERIC",
    "GENERIC_ENTITY",
    "IF",
    "IN",
    "INTEGER",
    "INVERSE",
    "LIKE",
    "LIST",
    "LOCAL",
    "LOGICAL",
    "MOD",
    "NOT",
    "NUMBER",
    "OF",
    "ONEOF",
    "OPTIONAL",
    "OR",
    "OTHERWISE",
    "PI",
    "PROCEDURE",
    "QUERY",
    "REAL",
    "REFERENCE",
    "RENAMED",
    "REPEAT",
    "RETURN",
    "RULE",
    "SCHEMA",
    "SELECT",
    "SELF",
    "SET",
    "SKIP",
    "STRING",
    "SUBTYPE",
    "SUBTYPE_CONSTRAINT",
    "SUPERTYPE",
    "THEN",
    "TO",
    "TOTAL_OVER",
    "TRUE",
    "TYPE",
    "UNIQUE",
    "UNKNOWN",
    "UNTIL",
    "USE",
    "VAR",
    "WHERE",
    "WHILE",
    "WITH",
    "XOR",
];

/// Parses one schema.
///
/// # Errors
///
/// Returns the first lexical or syntax error, with its position.
pub fn parse(source: &str) -> Result<Schema, Error> {
    if source.len() > MAX_INPUT_LEN {
        let kind = ErrorKind::InputTooLarge {
            limit: MAX_INPUT_LEN,
        };
        return Err(Error::new(kind));
    }
    let mut parser = Parser {
        tokens: tokenize(source)?,
        index: 0,
        depth: 0,
    };
    let schema = parser.schema()?;
    parser.expect_end()?;
    Ok(schema)
}

pub(crate) struct Parser {
    tokens: Vec<Spanned>,
    index: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        self.peek_at(0)
    }

    /// The token `ahead` tokens after the next one; the end token repeats past the end.
    fn peek_at(&self, ahead: usize) -> &Token {
        let last = self.tokens.len() - 1;
        &self.tokens[(self.index + ahead).min(last)].token
    }

    fn position(&self) -> Position {
        self.tokens[self.index].position
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if self.index < self.tokens.len() - 1 {
            self.index += 1;
        }
        token
    }

    fn unexpected(&self, expected: &'static str) -> Error {
        let found = self.peek().describe();
        Error::at(
            ErrorKind::UnexpectedToken { expected, found },
            self.position(),
        )
    }

    fn is_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek(), Token::Ident(name) if name.eq_ignore_ascii_case(keyword))
    }

    fn is_any_keyword(&self, keywords: &[&str]) -> bool {
        keywords.iter().any(|keyword| self.is_keyword(keyword))
    }

    fn eat_keyword(&mut self, keyword: &str) -> bool {
        let found = self.is_keyword(keyword);
        if found {
            self.advance();
        }
        found
    }

    fn expect_keyword(&mut self, keyword: &'static str) -> Result<(), Error> {
        if self.eat_keyword(keyword) {
            Ok(())
        } else {
            Err(self.unexpected(keyword))
        }
    }

    fn is_symbol(&self, symbol: Symbol) -> bool {
        *self.peek() == Token::Symbol(symbol)
    }

    fn eat_symbol(&mut self, symbol: Symbol) -> bool {
        let found = self.is_symbol(symbol);
        if found {
            self.advance();
        }
        found
    }

    fn expect_symbol(&mut self, symbol: Symbol) -> Result<(), Error> {
        if self.eat_symbol(symbol) {
            Ok(())
        } else {
            Err(self.unexpected(symbol_name(symbol)))
        }
    }

    fn expect_end(&self) -> Result<(), Error> {
        if *self.peek() == Token::End {
            Ok(())
        } else {
            Err(self.unexpected("end of input"))
        }
    }

    /// Whether the next token is an identifier that is not a reserved word.
    fn is_ident(&self) -> bool {
        is_name(self.peek())
    }

    /// Whether the next tokens are `label :`, which starts a labelled rule.
    fn is_label(&self) -> bool {
        self.is_ident() && *self.peek_at(1) == Token::Symbol(Symbol::Colon)
    }

    fn ident(&mut self) -> Result<Ident, Error> {
        let position = self.position();
        match self.peek() {
            Token::Ident(name) if is_name(self.peek()) => {
                let name = name.clone();
                self.advance();
                Ok(Ident { name, position })
            }
            _ => Err(self.unexpected("a name")),
        }
    }

    /// `name, name, …`
    fn ident_list(&mut self) -> Result<Vec<Ident>, Error> {
        self.separated(Self::ident)
    }

    /// `item, item, …`: one or more items separated by commas.
    fn separated<T>(
        &mut self,
        mut parse: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        let mut items = vec![parse(self)?];
        while self.eat_symbol(Symbol::Comma) {
            items.push(parse(self)?);
        }
        Ok(items)
    }

    /// `[item, item, …] close`: items separated by commas, possibly none, then `close`.
    fn separated_until<T>(
        &mut self,
        close: Symbol,
        parse: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        if self.eat_symbol(close) {
            return Ok(Vec::new());
        }
        let items = self.separated(parse)?;
        self.expect_symbol(close)?;
        Ok(items)
    }

    /// What `parse` reads when the next token is `keyword`, which is consumed.
    fn after_keyword<T>(
        &mut self,
        keyword: &str,
        parse: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<Option<T>, Error> {
        if self.eat_keyword(keyword) {
            parse(self).map(Some)
        } else {
            Ok(None)
        }
    }

    /// What `parse` reads when the next token is `symbol`, which is consumed.
    fn after_symbol<T>(
        &mut self,
        symbol: Symbol,
        parse: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<Option<T>, Error> {
        if self.eat_symbol(symbol) {
            parse(self).map(Some)
        } else {
            Ok(None)
        }
    }

    /// `keyword;`, which closes a block, such as `END_ENTITY;`.
    fn end(&mut self, keyword: &'static str) -> Result<(), Error> {
        self.expect_keyword(keyword)?;
        self.expect_symbol(Symbol::Semicolon)
    }

    /// `( name, name, … )`
    fn parenthesized_idents(&mut self) -> Result<Vec<Ident>, Error> {
        self.expect_symbol(Symbol::LeftParen)?;
        let names = self.ident_list()?;
        self.expect_symbol(Symbol::RightParen)?;
        Ok(names)
    }

    /// Runs `parse` one level deeper, failing past [`MAX_DEPTH`]. `parse` may [`enter`] more
    /// levels, for constructs that nest without recursing; the depth is restored afterwards.
    ///
    /// [`enter`]: Self::enter
    fn nested<T>(&mut self, parse: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        let depth = self.depth;
        let result = self.enter().and_then(|()| parse(self));
        self.depth = depth;
        result
    }

    /// Goes one level deeper, failing past [`MAX_DEPTH`].
    fn enter(&mut self) -> Result<(), Error> {
        if self.depth >= MAX_DEPTH {
            let kind = ErrorKind::TooDeep { limit: MAX_DEPTH };
            return Err(Error::at(kind, self.position()));
        }
        self.depth += 1;
        Ok(())
    }
}

fn is_name(token: &Token) -> bool {
    match token {
        Token::Ident(name) => !RESERVED
            .iter()
            .any(|reserved| name.eq_ignore_ascii_case(reserved)),
        _ => false,
    }
}

const fn symbol_name(symbol: Symbol) -> &'static str {
    match symbol {
        Symbol::LeftParen => "`(`",
        Symbol::RightParen => "`)`",
        Symbol::LeftBracket => "`[`",
        Symbol::RightBracket => "`]`",
        Symbol::RightBrace => "`}`",
        Symbol::Comma => "`,`",
        Symbol::Semicolon => "`;`",
        Symbol::Colon => "`:`",
        Symbol::Dot => "`.`",
        Symbol::Equal => "`=`",
        Symbol::Assign => "`:=`",
        Symbol::Backslash => "`\\`",
        Symbol::QueryFrom => "`<*`",
        Symbol::Pipe => "`|`",
        _ => "a symbol",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn error(source: &str) -> String {
        parse(source).unwrap_err().to_string()
    }

    #[test]
    fn reads_an_empty_schema() {
        let schema = parse("SCHEMA demo 'v1'; END_SCHEMA;").unwrap();
        assert_eq!(schema.name.name, "demo");
        assert_eq!(schema.version.as_deref(), Some("v1"));
        assert!(schema.declarations.is_empty());
        assert_eq!(parse("schema s; end_schema;").unwrap().version, None);
    }

    #[test]
    fn rejects_what_follows_the_schema() {
        assert_eq!(
            error("SCHEMA s; END_SCHEMA; x"),
            "line 1, column 23: expected end of input, found `x`"
        );
    }

    #[test]
    fn rejects_reserved_words_as_names() {
        assert_eq!(
            error("SCHEMA select; END_SCHEMA;"),
            "line 1, column 8: expected a name, found `select`"
        );
        assert_eq!(
            error("SCHEMA 1; END_SCHEMA;"),
            "line 1, column 8: expected a name, found an integer"
        );
    }

    #[test]
    fn reports_lexical_errors() {
        assert_eq!(
            error("SCHEMA ~"),
            "line 1, column 8: unexpected character '~'"
        );
    }

    #[test]
    fn rejects_oversized_input() {
        let source = " ".repeat(MAX_INPUT_LEN + 1);
        assert_eq!(
            parse(&source).unwrap_err().kind,
            ErrorKind::InputTooLarge {
                limit: MAX_INPUT_LEN
            }
        );
    }

    #[test]
    fn names_every_expected_symbol() {
        let cases = [
            (Symbol::LeftParen, "`(`"),
            (Symbol::RightParen, "`)`"),
            (Symbol::LeftBracket, "`[`"),
            (Symbol::RightBracket, "`]`"),
            (Symbol::RightBrace, "`}`"),
            (Symbol::Comma, "`,`"),
            (Symbol::Semicolon, "`;`"),
            (Symbol::Colon, "`:`"),
            (Symbol::Dot, "`.`"),
            (Symbol::Equal, "`=`"),
            (Symbol::Assign, "`:=`"),
            (Symbol::Backslash, "`\\`"),
            (Symbol::QueryFrom, "`<*`"),
            (Symbol::Pipe, "`|`"),
            (Symbol::Plus, "a symbol"),
        ];
        for (symbol, name) in cases {
            assert_eq!(symbol_name(symbol), name);
        }
    }
}
