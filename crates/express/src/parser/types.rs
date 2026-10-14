//! Types of attributes, parameters and variables.

use super::Parser;
use crate::ast::{AggregateKind, AggregateType, Bounds, Ident, SimpleType, TypeSpec, Width};
use crate::error::Error;
use crate::lexer::Symbol;

impl Parser {
    /// Any type: simple, named, aggregate or generic.
    pub(super) fn type_spec(&mut self) -> Result<TypeSpec, Error> {
        self.nested(Self::type_spec_inner)
    }

    fn type_spec_inner(&mut self) -> Result<TypeSpec, Error> {
        if let Some(simple) = self.simple_type()? {
            return Ok(TypeSpec::Simple(simple));
        }
        let aggregate = [
            ("ARRAY", AggregateKind::Array),
            ("BAG", AggregateKind::Bag),
            ("LIST", AggregateKind::List),
            ("SET", AggregateKind::Set),
        ]
        .into_iter()
        .find(|(keyword, _)| self.is_keyword(keyword));
        if let Some((_, kind)) = aggregate {
            self.advance();
            return Ok(TypeSpec::Aggregate(Box::new(self.aggregate(kind)?)));
        }
        if self.eat_keyword("AGGREGATE") {
            let label = self.type_label()?;
            self.expect_keyword("OF")?;
            let element = Box::new(self.type_spec()?);
            return Ok(TypeSpec::GenericAggregate { label, element });
        }
        if self.eat_keyword("GENERIC") {
            return Ok(TypeSpec::Generic(self.type_label()?));
        }
        if self.eat_keyword("GENERIC_ENTITY") {
            return Ok(TypeSpec::GenericEntity(self.type_label()?));
        }
        if self.is_ident() {
            return Ok(TypeSpec::Named(self.ident()?));
        }
        Err(self.unexpected("a type"))
    }

    fn simple_type(&mut self) -> Result<Option<SimpleType>, Error> {
        let simple = if self.eat_keyword("BINARY") {
            SimpleType::Binary(self.width()?)
        } else if self.eat_keyword("STRING") {
            SimpleType::String(self.width()?)
        } else if self.eat_keyword("REAL") {
            let precision = if self.eat_symbol(Symbol::LeftParen) {
                let precision = self.expression()?;
                self.expect_symbol(Symbol::RightParen)?;
                Some(precision)
            } else {
                None
            };
            SimpleType::Real(precision)
        } else if self.eat_keyword("BOOLEAN") {
            SimpleType::Boolean
        } else if self.eat_keyword("INTEGER") {
            SimpleType::Integer
        } else if self.eat_keyword("LOGICAL") {
            SimpleType::Logical
        } else if self.eat_keyword("NUMBER") {
            SimpleType::Number
        } else {
            return Ok(None);
        };
        Ok(Some(simple))
    }

    /// `[(width) [FIXED]]` after `STRING` or `BINARY`.
    fn width(&mut self) -> Result<Option<Width>, Error> {
        if !self.eat_symbol(Symbol::LeftParen) {
            return Ok(None);
        }
        let width = self.expression()?;
        self.expect_symbol(Symbol::RightParen)?;
        let fixed = self.eat_keyword("FIXED");
        Ok(Some(Width { width, fixed }))
    }

    /// `[bounds] OF [OPTIONAL] [UNIQUE] type`, after the aggregate keyword. `OPTIONAL` is for
    /// arrays only, `UNIQUE` for arrays and lists.
    fn aggregate(&mut self, kind: AggregateKind) -> Result<AggregateType, Error> {
        let bounds = self.optional_bounds()?;
        self.expect_keyword("OF")?;
        let optional = kind == AggregateKind::Array && self.eat_keyword("OPTIONAL");
        let unique = matches!(kind, AggregateKind::Array | AggregateKind::List)
            && self.eat_keyword("UNIQUE");
        let element = self.type_spec()?;
        Ok(AggregateType {
            kind,
            bounds,
            optional,
            unique,
            element,
        })
    }

    /// `[[lower : upper]]`
    pub(super) fn optional_bounds(&mut self) -> Result<Option<Bounds>, Error> {
        if !self.eat_symbol(Symbol::LeftBracket) {
            return Ok(None);
        }
        let lower = self.expression()?;
        self.expect_symbol(Symbol::Colon)?;
        let upper = self.expression()?;
        self.expect_symbol(Symbol::RightBracket)?;
        Ok(Some(Bounds { lower, upper }))
    }

    /// `[: label]` after a generic type.
    fn type_label(&mut self) -> Result<Option<Ident>, Error> {
        self.after_symbol(Symbol::Colon, Self::ident)
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::super::tests::error;
    use crate::ast::{AggregateKind, Declaration, Expr, SimpleType, TypeSpec, UnderlyingType};

    fn type_of(spec: &str) -> TypeSpec {
        let source = format!("SCHEMA s; TYPE t = {spec}; END_TYPE; END_SCHEMA;");
        match parse(&source).unwrap().declarations.remove(0) {
            Declaration::Type(decl) => match decl.underlying {
                UnderlyingType::Concrete(spec) => spec,
                other => panic!("not concrete: {other:?}"),
            },
            other => panic!("not a type: {other:?}"),
        }
    }

    #[test]
    fn reads_simple_types() {
        let cases = [
            ("BINARY", SimpleType::Binary(None)),
            ("BOOLEAN", SimpleType::Boolean),
            ("INTEGER", SimpleType::Integer),
            ("LOGICAL", SimpleType::Logical),
            ("NUMBER", SimpleType::Number),
            ("REAL", SimpleType::Real(None)),
            ("REAL(6)", SimpleType::Real(Some(Expr::Integer(6)))),
            ("STRING", SimpleType::String(None)),
        ];
        for (spec, expected) in cases {
            assert_eq!(type_of(spec), TypeSpec::Simple(expected), "{spec}");
        }
        let TypeSpec::Simple(SimpleType::String(Some(width))) = type_of("STRING(22) FIXED") else {
            panic!("not a fixed string");
        };
        assert!(width.fixed);
        assert_eq!(width.width, Expr::Integer(22));
        let TypeSpec::Simple(SimpleType::Binary(Some(width))) = type_of("BINARY(32)") else {
            panic!("not a binary");
        };
        assert!(!width.fixed);
    }

    #[test]
    fn reads_aggregates() {
        let TypeSpec::Aggregate(array) = type_of("ARRAY [1:2] OF OPTIONAL UNIQUE REAL") else {
            panic!("not an aggregate");
        };
        assert_eq!(array.kind, AggregateKind::Array);
        assert!(array.optional && array.unique);
        let bounds = array.bounds.unwrap();
        assert_eq!(
            (bounds.lower, bounds.upper),
            (Expr::Integer(1), Expr::Integer(2))
        );

        let TypeSpec::Aggregate(list) = type_of("LIST [2:?] OF UNIQUE LIST [3:3] OF Point") else {
            panic!("not an aggregate");
        };
        assert_eq!(list.kind, AggregateKind::List);
        assert!(list.unique && !list.optional);
        assert_eq!(list.bounds.unwrap().upper, Expr::Indeterminate);
        assert!(matches!(list.element, TypeSpec::Aggregate(_)));

        let TypeSpec::Aggregate(set) = type_of("SET OF Point") else {
            panic!("not an aggregate");
        };
        assert_eq!(set.kind, AggregateKind::Set);
        assert!(set.bounds.is_none());
        let TypeSpec::Aggregate(bag) = type_of("BAG [0:3] OF Point") else {
            panic!("not an aggregate");
        };
        assert_eq!(bag.kind, AggregateKind::Bag);
    }

    #[test]
    fn reads_generic_types() {
        let TypeSpec::GenericAggregate { label, element } = type_of("AGGREGATE : A OF GENERIC : T")
        else {
            panic!("not a generic aggregate");
        };
        assert_eq!(label.unwrap().name, "A");
        assert!(matches!(*element, TypeSpec::Generic(Some(_))));
        assert_eq!(type_of("GENERIC"), TypeSpec::Generic(None));
        assert!(matches!(
            type_of("GENERIC_ENTITY : E"),
            TypeSpec::GenericEntity(Some(_))
        ));
        assert!(matches!(
            type_of("AGGREGATE OF GENERIC_ENTITY"),
            TypeSpec::GenericAggregate { label: None, .. }
        ));
    }

    #[test]
    fn reads_named_types() {
        assert!(matches!(type_of("IfcLabel"), TypeSpec::Named(name) if name.name == "IfcLabel"));
    }

    #[test]
    fn rejects_malformed_types() {
        let wrap = |spec: &str| format!("SCHEMA s; TYPE t = {spec}; END_TYPE; END_SCHEMA;");
        assert_eq!(
            error(&wrap("LIST [1:2] REAL")),
            "line 1, column 31: expected OF, found `REAL`"
        );
        assert_eq!(
            error(&wrap("LIST [1 2] OF REAL")),
            "line 1, column 28: expected `:`, found an integer"
        );
        assert_eq!(
            error(&wrap("LIST [1:2 OF REAL")),
            "line 1, column 30: expected `]`, found `OF`"
        );
        assert_eq!(
            error(&wrap("STRING(2")),
            "line 1, column 28: expected `)`, found `;`"
        );
        assert_eq!(
            error(&wrap("REAL(2")),
            "line 1, column 26: expected `)`, found `;`"
        );
        assert_eq!(
            error(&wrap("AGGREGATE GENERIC")),
            "line 1, column 30: expected OF, found `GENERIC`"
        );
        assert_eq!(
            error(&wrap("SET OF 3")),
            "line 1, column 27: expected a type, found an integer"
        );
    }
}
