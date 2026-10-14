//! Expressions (ISO 10303-11 §12), from the loosest operators to the tightest: relational,
//! additive, multiplicative, power, unary, then qualified primaries.

use super::Parser;
use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::error::Error;
use crate::lexer::{Symbol, Token};

impl Parser {
    /// `simple [relational-op simple]`
    pub(super) fn expression(&mut self) -> Result<Expr, Error> {
        self.nested(|parser| {
            let left = parser.simple_expression()?;
            match parser.binary_op(relational_op) {
                Some(op) => {
                    let right = parser.simple_expression()?;
                    Ok(binary(op, left, right))
                }
                None => Ok(left),
            }
        })
    }

    /// `term {(+ | - | OR | XOR) term}`
    fn simple_expression(&mut self) -> Result<Expr, Error> {
        self.left_chain(Self::term, additive_op)
    }

    /// `factor {(* | / | DIV | MOD | AND | ||) factor}`
    fn term(&mut self) -> Result<Expr, Error> {
        self.left_chain(Self::factor, multiplicative_op)
    }

    /// `simple-factor {** simple-factor}`
    fn factor(&mut self) -> Result<Expr, Error> {
        self.left_chain(Self::simple_factor, power_op)
    }

    /// `operand {op operand}`, grouped from the left. Each operator nests the tree one level
    /// deeper, so it counts against the depth limit like any other nesting.
    fn left_chain(
        &mut self,
        operand: fn(&mut Self) -> Result<Expr, Error>,
        classify: fn(&Token) -> Option<BinaryOp>,
    ) -> Result<Expr, Error> {
        self.nested(|parser| {
            let mut left = operand(parser)?;
            while let Some(op) = parser.binary_op(classify) {
                parser.enter()?;
                let right = operand(parser)?;
                left = binary(op, left, right);
            }
            Ok(left)
        })
    }

    /// The operator `classify` finds in the next token, which is then consumed.
    fn binary_op(&mut self, classify: fn(&Token) -> Option<BinaryOp>) -> Option<BinaryOp> {
        let op = classify(self.peek());
        if op.is_some() {
            self.advance();
        }
        op
    }

    fn simple_factor(&mut self) -> Result<Expr, Error> {
        self.nested(Self::simple_factor_inner)
    }

    fn simple_factor_inner(&mut self) -> Result<Expr, Error> {
        if self.eat_symbol(Symbol::LeftBracket) {
            let aggregate = self.aggregate_initializer()?;
            return self.qualifiers(aggregate);
        }
        if self.eat_symbol(Symbol::LeftBrace) {
            return self.interval();
        }
        if self.eat_keyword("QUERY") {
            return self.query();
        }
        let unary = if self.eat_symbol(Symbol::Plus) {
            Some(UnaryOp::Plus)
        } else if self.eat_symbol(Symbol::Minus) {
            Some(UnaryOp::Minus)
        } else if self.eat_keyword("NOT") {
            Some(UnaryOp::Not)
        } else {
            None
        };
        if let Some(op) = unary {
            let operand = Box::new(self.simple_factor()?);
            return Ok(Expr::Unary { op, operand });
        }
        if self.eat_symbol(Symbol::LeftParen) {
            let inner = self.expression()?;
            self.expect_symbol(Symbol::RightParen)?;
            return self.qualifiers(inner);
        }
        self.primary()
    }

    /// `[element [: repetitions], …]`, after `[`.
    fn aggregate_initializer(&mut self) -> Result<Expr, Error> {
        let elements = self.separated_until(Symbol::RightBracket, |parser| {
            let element = parser.expression()?;
            let repetitions = parser.after_symbol(Symbol::Colon, Self::expression)?;
            Ok((element, repetitions))
        })?;
        Ok(Expr::Aggregate(elements))
    }

    /// `{low (< | <=) item (< | <=) high}`, after `{`.
    fn interval(&mut self) -> Result<Expr, Error> {
        let low = Box::new(self.simple_expression()?);
        let low_inclusive = self.interval_op()?;
        let item = Box::new(self.simple_expression()?);
        let high_inclusive = self.interval_op()?;
        let high = Box::new(self.simple_expression()?);
        self.expect_symbol(Symbol::RightBrace)?;
        Ok(Expr::Interval {
            low,
            low_inclusive,
            item,
            high_inclusive,
            high,
        })
    }

    /// `<` (false) or `<=` (true).
    fn interval_op(&mut self) -> Result<bool, Error> {
        if self.eat_symbol(Symbol::Less) {
            Ok(false)
        } else if self.eat_symbol(Symbol::LessEqual) {
            Ok(true)
        } else {
            Err(self.unexpected("`<` or `<=`"))
        }
    }

    /// `(variable <* source | condition)`, after `QUERY`.
    fn query(&mut self) -> Result<Expr, Error> {
        self.expect_symbol(Symbol::LeftParen)?;
        let variable = self.ident()?;
        self.expect_symbol(Symbol::QueryFrom)?;
        let source = Box::new(self.simple_expression()?);
        self.expect_symbol(Symbol::Pipe)?;
        let condition = Box::new(self.expression()?);
        self.expect_symbol(Symbol::RightParen)?;
        Ok(Expr::Query {
            variable,
            source,
            condition,
        })
    }

    /// A literal, a built-in constant, or a qualified reference.
    fn primary(&mut self) -> Result<Expr, Error> {
        let literal = match self.peek() {
            Token::Integer(value) => Some(Expr::Integer(*value)),
            Token::Real(value) => Some(Expr::Real(*value)),
            Token::String(value) => Some(Expr::String(value.clone())),
            Token::Binary(value) => Some(Expr::BinaryLiteral(value.clone())),
            Token::Symbol(Symbol::Question) => Some(Expr::Indeterminate),
            _ => [
                ("TRUE", Expr::Logical(Some(true))),
                ("FALSE", Expr::Logical(Some(false))),
                ("UNKNOWN", Expr::Logical(None)),
                ("PI", Expr::Pi),
                ("CONST_E", Expr::ConstE),
            ]
            .into_iter()
            .find(|(keyword, _)| self.is_keyword(keyword))
            .map(|(_, constant)| constant),
        };
        if let Some(literal) = literal {
            self.advance();
            return Ok(literal);
        }
        self.reference()
    }

    /// `SELF`, a name or a call, followed by qualifiers.
    pub(super) fn reference(&mut self) -> Result<Expr, Error> {
        if self.eat_keyword("SELF") {
            return self.qualifiers(Expr::SelfRef);
        }
        if !self.is_ident() {
            return Err(self.unexpected("an expression"));
        }
        let name = self.ident()?;
        let base = if self.eat_symbol(Symbol::LeftParen) {
            let arguments = self.separated_until(Symbol::RightParen, Self::expression)?;
            Expr::Call { name, arguments }
        } else {
            Expr::Name(name)
        };
        self.qualifiers(base)
    }

    /// `{.attribute | \entity | [index [: high]]}` after `base`.
    /// Each qualifier nests the tree one level deeper, so it counts against the depth limit.
    fn qualifiers(&mut self, base: Expr) -> Result<Expr, Error> {
        self.nested(|parser| parser.qualifiers_inner(base))
    }

    fn qualifiers_inner(&mut self, mut base: Expr) -> Result<Expr, Error> {
        loop {
            base = if self.eat_symbol(Symbol::Dot) {
                self.enter()?;
                Expr::Attribute {
                    base: Box::new(base),
                    name: self.ident()?,
                }
            } else if self.eat_symbol(Symbol::Backslash) {
                self.enter()?;
                Expr::Group {
                    base: Box::new(base),
                    entity: self.ident()?,
                }
            } else if self.eat_symbol(Symbol::LeftBracket) {
                self.enter()?;
                let index = Box::new(self.expression()?);
                let high = self
                    .after_symbol(Symbol::Colon, Self::expression)?
                    .map(Box::new);
                self.expect_symbol(Symbol::RightBracket)?;
                Expr::Index {
                    base: Box::new(base),
                    index,
                    high,
                }
            } else {
                return Ok(base);
            };
        }
    }
}

fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn keyword_op(token: &Token, keywords: &[(&str, BinaryOp)]) -> Option<BinaryOp> {
    let Token::Ident(word) = token else {
        return None;
    };
    keywords
        .iter()
        .find(|(keyword, _)| word.eq_ignore_ascii_case(keyword))
        .map(|&(_, op)| op)
}

fn power_op(token: &Token) -> Option<BinaryOp> {
    (*token == Token::Symbol(Symbol::Power)).then_some(BinaryOp::Power)
}

fn relational_op(token: &Token) -> Option<BinaryOp> {
    match token {
        Token::Symbol(Symbol::Equal) => Some(BinaryOp::Equal),
        Token::Symbol(Symbol::NotEqual) => Some(BinaryOp::NotEqual),
        Token::Symbol(Symbol::Less) => Some(BinaryOp::Less),
        Token::Symbol(Symbol::Greater) => Some(BinaryOp::Greater),
        Token::Symbol(Symbol::LessEqual) => Some(BinaryOp::LessEqual),
        Token::Symbol(Symbol::GreaterEqual) => Some(BinaryOp::GreaterEqual),
        Token::Symbol(Symbol::InstanceEqual) => Some(BinaryOp::InstanceEqual),
        Token::Symbol(Symbol::InstanceNotEqual) => Some(BinaryOp::InstanceNotEqual),
        _ => keyword_op(token, &[("IN", BinaryOp::In), ("LIKE", BinaryOp::Like)]),
    }
}

fn additive_op(token: &Token) -> Option<BinaryOp> {
    match token {
        Token::Symbol(Symbol::Plus) => Some(BinaryOp::Add),
        Token::Symbol(Symbol::Minus) => Some(BinaryOp::Subtract),
        _ => keyword_op(token, &[("OR", BinaryOp::Or), ("XOR", BinaryOp::Xor)]),
    }
}

fn multiplicative_op(token: &Token) -> Option<BinaryOp> {
    match token {
        Token::Symbol(Symbol::Star) => Some(BinaryOp::Multiply),
        Token::Symbol(Symbol::Slash) => Some(BinaryOp::Divide),
        Token::Symbol(Symbol::Concat) => Some(BinaryOp::Concat),
        _ => keyword_op(
            token,
            &[
                ("DIV", BinaryOp::IntegerDivide),
                ("MOD", BinaryOp::Modulo),
                ("AND", BinaryOp::And),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::super::tests::error;
    use super::super::{Parser, parse};
    use crate::ast::{BinaryOp, Declaration, Expr, UnaryOp};
    use crate::lexer::tokenize;
    use crate::{ErrorKind, MAX_DEPTH};

    fn expr(source: &str) -> Expr {
        let mut parser = Parser {
            tokens: tokenize(source).unwrap(),
            index: 0,
            depth: 0,
        };
        let expr = parser.expression().unwrap();
        parser.expect_end().unwrap();
        expr
    }

    /// The expression in a compact, fully parenthesised form, to check precedence.
    fn show(expr: &Expr) -> String {
        match expr {
            Expr::Integer(value) => value.to_string(),
            Expr::Real(value) => format!("{value:?}"),
            Expr::String(value) => format!("'{value}'"),
            Expr::BinaryLiteral(value) => format!("%{value}"),
            Expr::Logical(value) => format!("{value:?}"),
            Expr::Indeterminate => "?".into(),
            Expr::SelfRef => "SELF".into(),
            Expr::Pi => "PI".into(),
            Expr::ConstE => "E".into(),
            Expr::Name(name) => name.name.clone(),
            Expr::Unary { op, operand } => format!("({op:?} {})", show(operand)),
            Expr::Binary { op, left, right } => {
                format!("({} {op:?} {})", show(left), show(right))
            }
            Expr::Interval {
                low,
                low_inclusive,
                item,
                high_inclusive,
                high,
            } => format!(
                "{{{} {} {} {} {}}}",
                show(low),
                if *low_inclusive { "<=" } else { "<" },
                show(item),
                if *high_inclusive { "<=" } else { "<" },
                show(high)
            ),
            Expr::Call { name, arguments } => {
                let arguments: Vec<String> = arguments.iter().map(show).collect();
                format!("{}({})", name.name, arguments.join(", "))
            }
            Expr::Attribute { base, name } => format!("{}.{}", show(base), name.name),
            Expr::Group { base, entity } => format!("{}\\{}", show(base), entity.name),
            Expr::Index { base, index, high } => match high {
                Some(high) => format!("{}[{}:{}]", show(base), show(index), show(high)),
                None => format!("{}[{}]", show(base), show(index)),
            },
            Expr::Aggregate(elements) => {
                let elements: Vec<String> = elements
                    .iter()
                    .map(|(element, repetitions)| match repetitions {
                        Some(repetitions) => format!("{}:{}", show(element), show(repetitions)),
                        None => show(element),
                    })
                    .collect();
                format!("[{}]", elements.join(", "))
            }
            Expr::Query {
                variable,
                source,
                condition,
            } => format!(
                "QUERY({} <* {} | {})",
                variable.name,
                show(source),
                show(condition)
            ),
        }
    }

    #[test]
    fn reads_literals_and_constants() {
        let cases = [
            ("42", "42"),
            ("1.5", "1.5"),
            ("'a'", "'a'"),
            ("%01", "%01"),
            ("TRUE", "Some(true)"),
            ("false", "Some(false)"),
            ("UNKNOWN", "None"),
            ("?", "?"),
            ("SELF", "SELF"),
            ("PI", "PI"),
            ("CONST_E", "E"),
        ];
        for (source, expected) in cases {
            assert_eq!(show(&expr(source)), expected, "{source}");
        }
    }

    #[test]
    fn follows_operator_precedence() {
        let cases = [
            ("a + b * c", "(a Add (b Multiply c))"),
            ("a * b + c", "((a Multiply b) Add c)"),
            ("a - b - c", "((a Subtract b) Subtract c)"),
            ("a OR b AND c", "(a Or (b And c))"),
            ("a XOR b", "(a Xor b)"),
            ("-a ** 2", "((Minus a) Power 2)"),
            ("+a", "(Plus a)"),
            ("NOT a = b", "((Not a) Equal b)"),
            ("NOT (a = b)", "(Not (a Equal b))"),
            (
                "a / b DIV c MOD d",
                "(((a Divide b) IntegerDivide c) Modulo d)",
            ),
            ("a || b() || c(1)", "((a Concat b()) Concat c(1))"),
            ("(a + b) * c", "((a Add b) Multiply c)"),
            ("a <> b", "(a NotEqual b)"),
            ("a < b", "(a Less b)"),
            ("a > b", "(a Greater b)"),
            ("a <= b", "(a LessEqual b)"),
            ("a >= b", "(a GreaterEqual b)"),
            ("a :=: b", "(a InstanceEqual b)"),
            ("a :<>: b", "(a InstanceNotEqual b)"),
            ("'x' IN TYPEOF(a)", "('x' In TYPEOF(a))"),
            ("a LIKE 'b'", "(a Like 'b')"),
        ];
        for (source, expected) in cases {
            assert_eq!(show(&expr(source)), expected, "{source}");
        }
    }

    #[test]
    fn reads_qualified_references() {
        let cases = [
            (
                "SELF\\IfcObject.IsTypedBy[1].RelatingType",
                "SELF\\IfcObject.IsTypedBy[1].RelatingType",
            ),
            ("Enum.ITEM", "Enum.ITEM"),
            ("l[1:n]", "l[1:n]"),
            ("f().x", "f().x"),
            ("(a).b", "a.b"),
            ("[1, 2][1]", "[1, 2][1]"),
        ];
        for (source, expected) in cases {
            assert_eq!(show(&expr(source)), expected, "{source}");
        }
    }

    #[test]
    fn reads_compound_expressions() {
        let cases = [
            ("[]", "[]"),
            ("[a, l[1] : n]", "[a, l[1]:n]"),
            ("{1 <= SELF < 31}", "{1 <= SELF < 31}"),
            ("{0 < x <= 3}", "{0 < x <= 3}"),
            (
                "QUERY(t <* SELF.items | t.x > 0)",
                "QUERY(t <* SELF.items | (t.x Greater 0))",
            ),
            (
                "SIZEOF(QUERY(t <* s | TRUE)) = 0",
                "(SIZEOF(QUERY(t <* s | Some(true))) Equal 0)",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(show(&expr(source)), expected, "{source}");
        }
    }

    #[test]
    fn rejects_malformed_expressions() {
        let wrap = |source: &str| {
            format!("SCHEMA s; CONSTANT c : INTEGER := {source}; END_CONSTANT; END_SCHEMA;")
        };
        let cases = [
            (
                "{1 = x < 2}",
                "line 1, column 38: expected `<` or `<=`, found `=`",
            ),
            ("{1 < x < 2", "line 1, column 45: expected `}`, found `;`"),
            ("QUERY t", "line 1, column 41: expected `(`, found `t`"),
            ("QUERY(t s)", "line 1, column 43: expected `<*`, found `s`"),
            (
                "QUERY(t <* s t)",
                "line 1, column 48: expected `|`, found `t`",
            ),
            (
                "QUERY(t <* s | t",
                "line 1, column 51: expected `)`, found `;`",
            ),
            ("[1, 2", "line 1, column 40: expected `]`, found `;`"),
            ("(1", "line 1, column 37: expected `)`, found `;`"),
            ("f(1", "line 1, column 38: expected `)`, found `;`"),
            ("a[1", "line 1, column 38: expected `]`, found `;`"),
            ("a.", "line 1, column 37: expected a name, found `;`"),
            ("*", "line 1, column 35: expected an expression, found `*`"),
        ];
        for (source, message) in cases {
            assert_eq!(error(&wrap(source)), message, "{source}");
        }
    }

    #[test]
    fn rejects_deep_nesting() {
        let source = format!(
            "SCHEMA s; CONSTANT c : INTEGER := {}1{}; END_CONSTANT; END_SCHEMA;",
            "(".repeat(MAX_DEPTH),
            ")".repeat(MAX_DEPTH)
        );
        assert_eq!(
            parse(&source).unwrap_err().kind,
            ErrorKind::TooDeep { limit: MAX_DEPTH }
        );
        let source = format!(
            "SCHEMA s; CONSTANT c : INTEGER := {}1; END_CONSTANT; END_SCHEMA;",
            "NOT ".repeat(MAX_DEPTH)
        );
        assert_eq!(
            parse(&source).unwrap_err().kind,
            ErrorKind::TooDeep { limit: MAX_DEPTH }
        );
    }

    #[test]
    fn rejects_long_operator_and_qualifier_chains() {
        let chains = [
            vec!["1"; MAX_DEPTH * 2].join(" + "),
            vec!["1"; MAX_DEPTH * 2].join(" * "),
            vec!["1"; MAX_DEPTH * 2].join(" ** "),
            format!("a{}", ".b".repeat(MAX_DEPTH * 2)),
            format!("a{}", "\\b".repeat(MAX_DEPTH * 2)),
            format!("a{}", "[1]".repeat(MAX_DEPTH * 2)),
        ];
        for chain in chains {
            let source =
                format!("SCHEMA s; CONSTANT c : INTEGER := {chain}; END_CONSTANT; END_SCHEMA;");
            assert_eq!(
                parse(&source).unwrap_err().kind,
                ErrorKind::TooDeep { limit: MAX_DEPTH },
                "{chain}"
            );
        }
        // A chain within the limit still reads, grouped from the left.
        let chain = vec!["1"; MAX_DEPTH / 2].join(" - ");
        assert!(matches!(
            expr(&chain),
            Expr::Binary {
                op: BinaryOp::Subtract,
                ..
            }
        ));
    }

    #[test]
    fn reads_constants_in_a_schema() {
        let schema =
            parse("SCHEMA s; CONSTANT c : REAL := -PI; END_CONSTANT; END_SCHEMA;").unwrap();
        let Declaration::Constant(constants) = &schema.declarations[0] else {
            panic!("not constants");
        };
        assert!(matches!(
            constants[0].value,
            Expr::Unary {
                op: UnaryOp::Minus,
                ..
            }
        ));
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Any token soup is parsed or rejected; nothing panics or overflows the stack.
        #[test]
        fn never_panics(source in "([a-z]{1,3}|[0-9]{1,3}|[-+*/()\\[\\]{}<>=:;,.|\\\\?']| ){0,200}") {
            let _ = parse(&format!("SCHEMA s; CONSTANT c : INTEGER := {source}; END_CONSTANT; END_SCHEMA;"));
        }

        /// Keywords and operators read the same whatever their case.
        #[test]
        fn keywords_ignore_case(upper in proptest::bool::ANY) {
            let source = "a AND b OR NOT c XOR d DIV e MOD f IN g";
            let source = if upper { source.to_owned() } else { source.to_lowercase() };
            let expected = "((((a And b) Or (Not c)) Xor ((d IntegerDivide e) Modulo f)) In g)";
            prop_assert_eq!(show(&expr(&source)).to_lowercase(), expected.to_lowercase());
            let is_in = matches!(expr(&source), Expr::Binary { op: BinaryOp::In, .. });
            prop_assert!(is_in);
        }
    }
}
