//! Statements of functions, procedures and rules (ISO 10303-11 §13).

use super::Parser;
use crate::ast::{Expr, Increment, Stmt};
use crate::error::Error;
use crate::lexer::{Symbol, Token};

impl Parser {
    /// Statements up to, not including, one of the keywords in `ends`.
    pub(super) fn statements_until(&mut self, ends: &[&str]) -> Result<Vec<Stmt>, Error> {
        let mut statements = Vec::new();
        while !self.is_any_keyword(ends) {
            statements.push(self.statement()?);
        }
        Ok(statements)
    }

    fn statement(&mut self) -> Result<Stmt, Error> {
        self.nested(Self::statement_inner)
    }

    fn statement_inner(&mut self) -> Result<Stmt, Error> {
        if self.eat_symbol(Symbol::Semicolon) {
            return Ok(Stmt::Null);
        }
        if self.eat_keyword("ALIAS") {
            return self.alias();
        }
        if self.eat_keyword("BEGIN") {
            let body = self.statements_until(&["END"])?;
            self.end("END")?;
            return Ok(Stmt::Compound(body));
        }
        if self.eat_keyword("CASE") {
            return self.case();
        }
        if self.eat_keyword("ESCAPE") {
            self.expect_symbol(Symbol::Semicolon)?;
            return Ok(Stmt::Escape);
        }
        if self.eat_keyword("IF") {
            return self.if_statement();
        }
        if self.eat_keyword("REPEAT") {
            return self.repeat();
        }
        if self.eat_keyword("RETURN") {
            let value = if self.eat_symbol(Symbol::LeftParen) {
                let value = self.expression()?;
                self.expect_symbol(Symbol::RightParen)?;
                Some(value)
            } else {
                None
            };
            self.expect_symbol(Symbol::Semicolon)?;
            return Ok(Stmt::Return(value));
        }
        if self.eat_keyword("SKIP") {
            self.expect_symbol(Symbol::Semicolon)?;
            return Ok(Stmt::Skip);
        }
        if self.is_ident() {
            return self.assignment_or_call();
        }
        Err(self.unexpected("a statement"))
    }

    /// `ALIAS name FOR reference; statements END_ALIAS;`, after `ALIAS`.
    fn alias(&mut self) -> Result<Stmt, Error> {
        let name = self.ident()?;
        self.expect_keyword("FOR")?;
        let target = self.reference()?;
        self.expect_symbol(Symbol::Semicolon)?;
        let body = self.statements_until(&["END_ALIAS"])?;
        self.end("END_ALIAS")?;
        Ok(Stmt::Alias { name, target, body })
    }

    /// `CASE selector OF labels : statement … [OTHERWISE : statement] END_CASE;`, after `CASE`.
    fn case(&mut self) -> Result<Stmt, Error> {
        let selector = self.expression()?;
        self.expect_keyword("OF")?;
        let mut branches = Vec::new();
        while !self.is_any_keyword(&["OTHERWISE", "END_CASE"]) {
            let labels = self.separated(Self::expression)?;
            self.expect_symbol(Symbol::Colon)?;
            branches.push((labels, self.statement()?));
        }
        let otherwise = if self.eat_keyword("OTHERWISE") {
            self.expect_symbol(Symbol::Colon)?;
            Some(Box::new(self.statement()?))
        } else {
            None
        };
        self.end("END_CASE")?;
        Ok(Stmt::Case {
            selector,
            branches,
            otherwise,
        })
    }

    /// `IF condition THEN statements [ELSE statements] END_IF;`, after `IF`.
    fn if_statement(&mut self) -> Result<Stmt, Error> {
        let condition = self.expression()?;
        self.expect_keyword("THEN")?;
        let then = self.statements_until(&["ELSE", "END_IF"])?;
        let otherwise = if self.eat_keyword("ELSE") {
            self.statements_until(&["END_IF"])?
        } else {
            Vec::new()
        };
        self.end("END_IF")?;
        Ok(Stmt::If {
            condition,
            then,
            otherwise,
        })
    }

    /// `REPEAT [i := a TO b [BY c]] [WHILE x] [UNTIL y]; statements END_REPEAT;`, after `REPEAT`.
    fn repeat(&mut self) -> Result<Stmt, Error> {
        let increment = if self.is_ident() && *self.peek_at(1) == Token::Symbol(Symbol::Assign) {
            let variable = self.ident()?;
            self.advance();
            let start = self.expression()?;
            self.expect_keyword("TO")?;
            let end = self.expression()?;
            let step = self.after_keyword("BY", Self::expression)?;
            Some(Box::new(Increment {
                variable,
                start,
                end,
                step,
            }))
        } else {
            None
        };
        let while_condition = self.after_keyword("WHILE", Self::expression)?;
        let until_condition = self.after_keyword("UNTIL", Self::expression)?;
        self.expect_symbol(Symbol::Semicolon)?;
        let body = self.statements_until(&["END_REPEAT"])?;
        self.end("END_REPEAT")?;
        Ok(Stmt::Repeat {
            increment,
            while_condition,
            until_condition,
            body,
        })
    }

    /// `reference := value;` or `procedure [(arguments)];`.
    fn assignment_or_call(&mut self) -> Result<Stmt, Error> {
        let start = self.index;
        let target = self.reference()?;
        if self.eat_symbol(Symbol::Assign) {
            let value = self.expression()?;
            self.expect_symbol(Symbol::Semicolon)?;
            return Ok(Stmt::Assign { target, value });
        }
        let statement = match target {
            Expr::Call { name, arguments } => Stmt::Call { name, arguments },
            Expr::Name(name) => Stmt::Call {
                name,
                arguments: Vec::new(),
            },
            _ => {
                // Point the error at the start of the reference.
                self.index = start;
                return Err(self.unexpected("a procedure call or an assignment"));
            }
        };
        self.expect_symbol(Symbol::Semicolon)?;
        Ok(statement)
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::super::tests::error;
    use crate::ast::{Declaration, Expr, Stmt};

    fn body(statements: &str) -> Vec<Stmt> {
        let source = format!("SCHEMA s; PROCEDURE p; {statements} END_PROCEDURE; END_SCHEMA;");
        match parse(&source).unwrap().declarations.remove(0) {
            Declaration::Procedure(procedure) => procedure.algorithm.body,
            other => panic!("not a procedure: {other:?}"),
        }
    }

    fn statement_error(statements: &str) -> String {
        error(&format!(
            "SCHEMA s; PROCEDURE p; {statements} END_PROCEDURE; END_SCHEMA;"
        ))
    }

    #[test]
    fn reads_simple_statements() {
        assert_eq!(
            body("; ESCAPE; SKIP; RETURN; RETURN (?);"),
            [
                Stmt::Null,
                Stmt::Escape,
                Stmt::Skip,
                Stmt::Return(None),
                Stmt::Return(Some(Expr::Indeterminate)),
            ]
        );
    }

    #[test]
    fn reads_assignments_and_calls() {
        let statements = body("x := 1; a[2].b := 3; INSERT(l, 1, 0); go;");
        assert!(matches!(
            &statements[0],
            Stmt::Assign {
                target: Expr::Name(_),
                ..
            }
        ));
        assert!(matches!(
            &statements[1],
            Stmt::Assign {
                target: Expr::Attribute { .. },
                ..
            }
        ));
        assert!(matches!(&statements[2], Stmt::Call { arguments, .. } if arguments.len() == 3));
        assert!(matches!(&statements[3], Stmt::Call { arguments, .. } if arguments.is_empty()));
    }

    #[test]
    fn reads_compound_statements() {
        let statements = body(
            "BEGIN x := 1; END;\n\
             IF a THEN x := 1; ELSE x := 2; x := 3; END_IF;\n\
             IF a THEN ; END_IF;\n\
             CASE k OF 1, 2 : x := 1; 3 : ; OTHERWISE : x := 0; END_CASE;\n\
             CASE k OF 1 : ; END_CASE;\n\
             ALIAS p FOR a.b[1]; p := 1; END_ALIAS;",
        );
        assert!(matches!(&statements[0], Stmt::Compound(body) if body.len() == 1));
        assert!(
            matches!(&statements[1], Stmt::If { then, otherwise, .. } if then.len() == 1 && otherwise.len() == 2)
        );
        assert!(matches!(&statements[2], Stmt::If { otherwise, .. } if otherwise.is_empty()));
        let Stmt::Case {
            branches,
            otherwise,
            ..
        } = &statements[3]
        else {
            panic!("not a case");
        };
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0].0.len(), 2);
        assert!(otherwise.is_some());
        assert!(matches!(
            &statements[4],
            Stmt::Case {
                otherwise: None,
                ..
            }
        ));
        assert!(
            matches!(&statements[5], Stmt::Alias { target: Expr::Index { .. }, body, .. } if body.len() == 1)
        );
    }

    #[test]
    fn reads_repeats() {
        let statements = body(
            "REPEAT i := 1 TO n BY 2 WHILE i < 9 UNTIL i > 5; x := i; END_REPEAT;\n\
             REPEAT i := 1 TO n; END_REPEAT;\n\
             REPEAT WHILE TRUE; ESCAPE; END_REPEAT;",
        );
        let Stmt::Repeat {
            increment,
            while_condition,
            until_condition,
            body,
        } = &statements[0]
        else {
            panic!("not a repeat");
        };
        let increment = increment.as_ref().unwrap();
        assert_eq!(increment.variable.name, "i");
        assert!(increment.step.is_some());
        assert!(while_condition.is_some() && until_condition.is_some());
        assert_eq!(body.len(), 1);
        assert!(
            matches!(&statements[1], Stmt::Repeat { increment: Some(increment), .. } if increment.step.is_none())
        );
        assert!(matches!(
            &statements[2],
            Stmt::Repeat {
                increment: None,
                until_condition: None,
                ..
            }
        ));
    }

    #[test]
    fn rejects_malformed_statements() {
        assert_eq!(
            statement_error("x 1;"),
            "line 1, column 26: expected `;`, found an integer"
        );
        assert_eq!(
            statement_error("a.b;"),
            "line 1, column 24: expected a procedure call or an assignment, found `a`"
        );
        assert_eq!(
            statement_error("IF a x := 1; END_IF;"),
            "line 1, column 29: expected THEN, found `x`"
        );
        assert_eq!(
            statement_error("REPEAT i := 1 n; END_REPEAT;"),
            "line 1, column 38: expected TO, found `n`"
        );
        assert_eq!(
            statement_error("CASE k 1 : ; END_CASE;"),
            "line 1, column 31: expected OF, found an integer"
        );
        assert_eq!(
            statement_error("CASE k OF 1 ; END_CASE;"),
            "line 1, column 36: expected `:`, found `;`"
        );
        assert_eq!(
            statement_error("CASE k OF OTHERWISE ; END_CASE;"),
            "line 1, column 44: expected `:`, found `;`"
        );
        assert_eq!(
            statement_error("ALIAS p a; END_ALIAS;"),
            "line 1, column 32: expected FOR, found `a`"
        );
        assert_eq!(
            statement_error("BEGIN END"),
            "line 1, column 34: expected `;`, found `END_PROCEDURE`"
        );
        assert_eq!(
            statement_error("RETURN (1;"),
            "line 1, column 33: expected `)`, found `;`"
        );
        assert_eq!(
            statement_error("ESCAPE"),
            "line 1, column 31: expected `;`, found `END_PROCEDURE`"
        );
        assert_eq!(
            statement_error("SKIP"),
            "line 1, column 29: expected `;`, found `END_PROCEDURE`"
        );
        assert_eq!(
            statement_error("42;"),
            "line 1, column 24: expected a statement, found an integer"
        );
    }
}
