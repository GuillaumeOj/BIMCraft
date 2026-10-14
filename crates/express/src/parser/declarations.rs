//! Schemas, type and entity declarations, functions, procedures, rules and constants.

use super::Parser;
use crate::ast::{
    AggregateKind, Algorithm, AttrName, ConstantDecl, Declaration, DerivedAttr, EntityDecl,
    ExplicitAttr, FunctionDecl, Ident, InverseAttr, LocalDecl, Parameter, ProcedureDecl, RuleDecl,
    Schema, SupertypeExpr, TypeDecl, UnderlyingType, UniqueRule, WhereRule,
};
use crate::error::{Error, ErrorKind};
use crate::lexer::{Symbol, Token};

/// Keywords that end the explicit attributes of an entity.
const AFTER_EXPLICIT: [&str; 5] = ["DERIVE", "INVERSE", "UNIQUE", "WHERE", "END_ENTITY"];

impl Parser {
    /// `SCHEMA name ['version']; declarations END_SCHEMA;`
    pub(super) fn schema(&mut self) -> Result<Schema, Error> {
        self.expect_keyword("SCHEMA")?;
        let name = self.ident()?;
        let version = match self.peek() {
            Token::String(version) => {
                let version = version.clone();
                self.advance();
                Some(version)
            }
            _ => None,
        };
        self.expect_symbol(Symbol::Semicolon)?;
        if self.is_any_keyword(&["USE", "REFERENCE"]) {
            let kind = ErrorKind::Unsupported("an interface specification (USE or REFERENCE)");
            return Err(Error::at(kind, self.position()));
        }
        let mut declarations = Vec::new();
        while !self.eat_keyword("END_SCHEMA") {
            match self.declaration(true)? {
                Some(declaration) => declarations.push(declaration),
                None => return Err(self.unexpected("a declaration or END_SCHEMA")),
            }
        }
        self.expect_symbol(Symbol::Semicolon)?;
        Ok(Schema {
            name,
            version,
            declarations,
        })
    }

    /// A declaration, or `None` when the next token does not start one. Rules are only
    /// allowed in a schema.
    fn declaration(&mut self, allow_rule: bool) -> Result<Option<Declaration>, Error> {
        let declaration = if self.is_keyword("TYPE") {
            Declaration::Type(self.nested(Self::type_decl)?)
        } else if self.is_keyword("ENTITY") {
            Declaration::Entity(Box::new(self.nested(Self::entity_decl)?))
        } else if self.is_keyword("FUNCTION") {
            Declaration::Function(Box::new(self.nested(Self::function_decl)?))
        } else if self.is_keyword("PROCEDURE") {
            Declaration::Procedure(Box::new(self.nested(Self::procedure_decl)?))
        } else if allow_rule && self.is_keyword("RULE") {
            Declaration::Rule(Box::new(self.nested(Self::rule_decl)?))
        } else if self.is_keyword("CONSTANT") {
            Declaration::Constant(self.constant_decl()?)
        } else if self.is_keyword("SUBTYPE_CONSTRAINT") {
            let kind = ErrorKind::Unsupported("SUBTYPE_CONSTRAINT");
            return Err(Error::at(kind, self.position()));
        } else {
            return Ok(None);
        };
        Ok(Some(declaration))
    }

    /// `TYPE name = underlying; [WHERE …] END_TYPE;`
    fn type_decl(&mut self) -> Result<TypeDecl, Error> {
        self.expect_keyword("TYPE")?;
        let name = self.ident()?;
        self.expect_symbol(Symbol::Equal)?;
        let underlying = self.underlying_type()?;
        self.expect_symbol(Symbol::Semicolon)?;
        let where_rules = self.where_clause("END_TYPE")?;
        self.end("END_TYPE")?;
        Ok(TypeDecl {
            name,
            underlying,
            where_rules,
        })
    }

    fn underlying_type(&mut self) -> Result<UnderlyingType, Error> {
        let extensible = self.eat_keyword("EXTENSIBLE");
        let generic_entity = extensible && self.eat_keyword("GENERIC_ENTITY");
        if !generic_entity && self.eat_keyword("ENUMERATION") {
            let (based_on, items) = if self.eat_keyword("OF") {
                (None, self.parenthesized_idents()?)
            } else {
                self.based_on()?
            };
            return Ok(UnderlyingType::Enumeration {
                extensible,
                based_on,
                items,
            });
        }
        if self.eat_keyword("SELECT") {
            let (based_on, items) = if self.is_symbol(Symbol::LeftParen) {
                (None, self.parenthesized_idents()?)
            } else {
                self.based_on()?
            };
            return Ok(UnderlyingType::Select {
                extensible,
                generic_entity,
                based_on,
                items,
            });
        }
        if extensible {
            let expected = if generic_entity {
                "SELECT"
            } else {
                "ENUMERATION or SELECT"
            };
            return Err(self.unexpected(expected));
        }
        Ok(UnderlyingType::Concrete(self.type_spec()?))
    }

    /// `[BASED_ON name [WITH (items)]]`, after `ENUMERATION` or `SELECT`.
    fn based_on(&mut self) -> Result<(Option<Ident>, Vec<Ident>), Error> {
        if !self.eat_keyword("BASED_ON") {
            return Ok((None, Vec::new()));
        }
        let base = self.ident()?;
        let items = if self.eat_keyword("WITH") {
            self.parenthesized_idents()?
        } else {
            Vec::new()
        };
        Ok((Some(base), items))
    }

    /// `[WHERE [label :] condition; …]`, up to `end`.
    fn where_clause(&mut self, end: &str) -> Result<Vec<WhereRule>, Error> {
        let mut rules = Vec::new();
        if !self.eat_keyword("WHERE") {
            return Ok(rules);
        }
        while !self.is_keyword(end) {
            let label = self.label()?;
            let condition = self.expression()?;
            self.expect_symbol(Symbol::Semicolon)?;
            rules.push(WhereRule { label, condition });
        }
        Ok(rules)
    }

    /// `label :` when the next tokens are one.
    fn label(&mut self) -> Result<Option<Ident>, Error> {
        if !self.is_label() {
            return Ok(None);
        }
        let label = self.ident()?;
        self.expect_symbol(Symbol::Colon)?;
        Ok(Some(label))
    }

    /// `ENTITY name [ABSTRACT] [SUPERTYPE OF (…)] [SUBTYPE OF (…)]; … END_ENTITY;`
    fn entity_decl(&mut self) -> Result<EntityDecl, Error> {
        self.expect_keyword("ENTITY")?;
        let name = self.ident()?;
        let is_abstract = self.eat_keyword("ABSTRACT");
        let mut supertype_of = None;
        // `ABSTRACT SUPERTYPE` may stand alone; otherwise `OF (…)` follows `SUPERTYPE`.
        if self.eat_keyword("SUPERTYPE") && (!is_abstract || self.is_keyword("OF")) {
            self.expect_keyword("OF")?;
            self.expect_symbol(Symbol::LeftParen)?;
            supertype_of = Some(self.supertype_expr()?);
            self.expect_symbol(Symbol::RightParen)?;
        }
        let mut subtype_of = Vec::new();
        if self.eat_keyword("SUBTYPE") {
            self.expect_keyword("OF")?;
            subtype_of = self.parenthesized_idents()?;
        }
        self.expect_symbol(Symbol::Semicolon)?;
        let mut explicit = Vec::new();
        while !self.is_any_keyword(&AFTER_EXPLICIT) {
            explicit.push(self.explicit_attr()?);
        }
        let derived = self.derive_clause()?;
        let inverse = self.inverse_clause()?;
        let unique = self.unique_clause()?;
        let where_rules = self.where_clause("END_ENTITY")?;
        self.end("END_ENTITY")?;
        Ok(EntityDecl {
            name,
            is_abstract,
            supertype_of,
            subtype_of,
            explicit,
            derived,
            inverse,
            unique,
            where_rules,
        })
    }

    /// `factor {ANDOR factor}`, where a factor is `term {AND term}`.
    fn supertype_expr(&mut self) -> Result<SupertypeExpr, Error> {
        self.nested(|parser| {
            let mut factors = vec![parser.supertype_factor()?];
            while parser.eat_keyword("ANDOR") {
                factors.push(parser.supertype_factor()?);
            }
            Ok(single_or(factors, SupertypeExpr::AndOr))
        })
    }

    fn supertype_factor(&mut self) -> Result<SupertypeExpr, Error> {
        let mut terms = vec![self.supertype_term()?];
        while self.eat_keyword("AND") {
            terms.push(self.supertype_term()?);
        }
        Ok(single_or(terms, SupertypeExpr::And))
    }

    /// `ONEOF (expr, …)`, `(expr)` or an entity name.
    fn supertype_term(&mut self) -> Result<SupertypeExpr, Error> {
        if self.eat_keyword("ONEOF") {
            self.expect_symbol(Symbol::LeftParen)?;
            let choices = self.separated(Self::supertype_expr)?;
            self.expect_symbol(Symbol::RightParen)?;
            return Ok(SupertypeExpr::OneOf(choices));
        }
        if self.eat_symbol(Symbol::LeftParen) {
            let inner = self.supertype_expr()?;
            self.expect_symbol(Symbol::RightParen)?;
            return Ok(inner);
        }
        Ok(SupertypeExpr::Entity(self.ident()?))
    }

    /// `names : [OPTIONAL] type;`
    fn explicit_attr(&mut self) -> Result<ExplicitAttr, Error> {
        let names = self.separated(Self::attr_name)?;
        self.expect_symbol(Symbol::Colon)?;
        let optional = self.eat_keyword("OPTIONAL");
        let ty = self.type_spec()?;
        self.expect_symbol(Symbol::Semicolon)?;
        Ok(ExplicitAttr {
            names,
            optional,
            ty,
        })
    }

    /// `name` or `SELF\entity.attribute [RENAMED name]`.
    fn attr_name(&mut self) -> Result<AttrName, Error> {
        if !self.eat_keyword("SELF") {
            return Ok(AttrName::New(self.ident()?));
        }
        self.expect_symbol(Symbol::Backslash)?;
        let entity = self.ident()?;
        self.expect_symbol(Symbol::Dot)?;
        let attribute = self.ident()?;
        let renamed = self.after_keyword("RENAMED", Self::ident)?;
        Ok(AttrName::Redeclared {
            entity,
            attribute,
            renamed,
        })
    }

    /// `[DERIVE name : type := value; …]`
    fn derive_clause(&mut self) -> Result<Vec<DerivedAttr>, Error> {
        let mut attributes = Vec::new();
        if !self.eat_keyword("DERIVE") {
            return Ok(attributes);
        }
        while !self.is_any_keyword(&AFTER_EXPLICIT[1..]) {
            let name = self.attr_name()?;
            self.expect_symbol(Symbol::Colon)?;
            let ty = self.type_spec()?;
            self.expect_symbol(Symbol::Assign)?;
            let value = self.expression()?;
            self.expect_symbol(Symbol::Semicolon)?;
            attributes.push(DerivedAttr { name, ty, value });
        }
        Ok(attributes)
    }

    /// `[INVERSE name : [SET|BAG [bounds] OF] entity FOR [entity.]attribute; …]`
    fn inverse_clause(&mut self) -> Result<Vec<InverseAttr>, Error> {
        let mut attributes = Vec::new();
        if !self.eat_keyword("INVERSE") {
            return Ok(attributes);
        }
        while !self.is_any_keyword(&AFTER_EXPLICIT[2..]) {
            let name = self.attr_name()?;
            self.expect_symbol(Symbol::Colon)?;
            let kind = if self.eat_keyword("SET") {
                Some(AggregateKind::Set)
            } else if self.eat_keyword("BAG") {
                Some(AggregateKind::Bag)
            } else {
                None
            };
            let aggregate = kind
                .map(|kind| {
                    let bounds = self.optional_bounds()?;
                    self.expect_keyword("OF")?;
                    Ok((kind, bounds))
                })
                .transpose()?;
            let entity = self.ident()?;
            self.expect_keyword("FOR")?;
            let first = self.ident()?;
            let (for_entity, attribute) = if self.eat_symbol(Symbol::Dot) {
                (Some(first), self.ident()?)
            } else {
                (None, first)
            };
            self.expect_symbol(Symbol::Semicolon)?;
            attributes.push(InverseAttr {
                name,
                aggregate,
                entity,
                for_entity,
                attribute,
            });
        }
        Ok(attributes)
    }

    /// `[UNIQUE [label :] attribute, …; …]`
    fn unique_clause(&mut self) -> Result<Vec<UniqueRule>, Error> {
        let mut rules = Vec::new();
        if !self.eat_keyword("UNIQUE") {
            return Ok(rules);
        }
        while !self.is_any_keyword(&AFTER_EXPLICIT[3..]) {
            let label = self.label()?;
            let attributes = self.separated(Self::attr_name)?;
            self.expect_symbol(Symbol::Semicolon)?;
            rules.push(UniqueRule { label, attributes });
        }
        Ok(rules)
    }

    /// `FUNCTION name [(parameters)] : type; algorithm END_FUNCTION;`
    fn function_decl(&mut self) -> Result<FunctionDecl, Error> {
        self.expect_keyword("FUNCTION")?;
        let name = self.ident()?;
        let parameters = self.parameters()?;
        self.expect_symbol(Symbol::Colon)?;
        let result = self.type_spec()?;
        self.expect_symbol(Symbol::Semicolon)?;
        let algorithm = self.algorithm(&["END_FUNCTION"])?;
        self.end("END_FUNCTION")?;
        Ok(FunctionDecl {
            name,
            parameters,
            result,
            algorithm,
        })
    }

    /// `PROCEDURE name [(parameters)]; algorithm END_PROCEDURE;`
    fn procedure_decl(&mut self) -> Result<ProcedureDecl, Error> {
        self.expect_keyword("PROCEDURE")?;
        let name = self.ident()?;
        let parameters = self.parameters()?;
        self.expect_symbol(Symbol::Semicolon)?;
        let algorithm = self.algorithm(&["END_PROCEDURE"])?;
        self.end("END_PROCEDURE")?;
        Ok(ProcedureDecl {
            name,
            parameters,
            algorithm,
        })
    }

    /// `RULE name FOR (entities); algorithm WHERE … END_RULE;`
    fn rule_decl(&mut self) -> Result<RuleDecl, Error> {
        self.expect_keyword("RULE")?;
        let name = self.ident()?;
        self.expect_keyword("FOR")?;
        let entities = self.parenthesized_idents()?;
        self.expect_symbol(Symbol::Semicolon)?;
        let algorithm = self.algorithm(&["WHERE"])?;
        let where_rules = self.where_clause("END_RULE")?;
        self.end("END_RULE")?;
        Ok(RuleDecl {
            name,
            entities,
            algorithm,
            where_rules,
        })
    }

    /// `[( [VAR] names : type; … )]`
    fn parameters(&mut self) -> Result<Vec<Parameter>, Error> {
        let mut parameters = Vec::new();
        if !self.eat_symbol(Symbol::LeftParen) {
            return Ok(parameters);
        }
        loop {
            let var = self.eat_keyword("VAR");
            let names = self.ident_list()?;
            self.expect_symbol(Symbol::Colon)?;
            let ty = self.type_spec()?;
            parameters.push(Parameter { names, var, ty });
            if !self.eat_symbol(Symbol::Semicolon) {
                break;
            }
        }
        self.expect_symbol(Symbol::RightParen)?;
        Ok(parameters)
    }

    /// Nested declarations, constants, `LOCAL` variables, then statements up to one of `ends`.
    fn algorithm(&mut self, ends: &[&str]) -> Result<Algorithm, Error> {
        let mut declarations = Vec::new();
        while let Some(declaration) = self.declaration(false)? {
            declarations.push(declaration);
        }
        let locals = self.local_decl()?;
        let body = self.statements_until(ends)?;
        Ok(Algorithm {
            declarations,
            locals,
            body,
        })
    }

    /// `CONSTANT name : type := value; … END_CONSTANT;`
    fn constant_decl(&mut self) -> Result<Vec<ConstantDecl>, Error> {
        self.expect_keyword("CONSTANT")?;
        let mut constants = Vec::new();
        while !self.eat_keyword("END_CONSTANT") {
            let name = self.ident()?;
            self.expect_symbol(Symbol::Colon)?;
            let ty = self.type_spec()?;
            self.expect_symbol(Symbol::Assign)?;
            let value = self.expression()?;
            self.expect_symbol(Symbol::Semicolon)?;
            constants.push(ConstantDecl { name, ty, value });
        }
        self.expect_symbol(Symbol::Semicolon)?;
        Ok(constants)
    }

    /// `[LOCAL names : type [:= value]; … END_LOCAL;]`
    fn local_decl(&mut self) -> Result<Vec<LocalDecl>, Error> {
        let mut locals = Vec::new();
        if !self.eat_keyword("LOCAL") {
            return Ok(locals);
        }
        while !self.eat_keyword("END_LOCAL") {
            let names = self.ident_list()?;
            self.expect_symbol(Symbol::Colon)?;
            let ty = self.type_spec()?;
            let initial = self.after_symbol(Symbol::Assign, Self::expression)?;
            self.expect_symbol(Symbol::Semicolon)?;
            locals.push(LocalDecl { names, ty, initial });
        }
        self.expect_symbol(Symbol::Semicolon)?;
        Ok(locals)
    }
}

/// The only item, or `combine` of several.
fn single_or(
    mut items: Vec<SupertypeExpr>,
    combine: fn(Vec<SupertypeExpr>) -> SupertypeExpr,
) -> SupertypeExpr {
    if items.len() == 1 {
        items.remove(0)
    } else {
        combine(items)
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::super::tests::error;
    use crate::ast::{
        AggregateKind, AttrName, Declaration, EntityDecl, Expr, SimpleType, SupertypeExpr,
        TypeSpec, UnderlyingType,
    };

    fn declarations(body: &str) -> Vec<Declaration> {
        parse(&format!("SCHEMA s;\n{body}\nEND_SCHEMA;"))
            .unwrap()
            .declarations
    }

    fn entity(body: &str) -> EntityDecl {
        match declarations(body).remove(0) {
            Declaration::Entity(entity) => *entity,
            other => panic!("not an entity: {other:?}"),
        }
    }

    fn underlying(body: &str) -> UnderlyingType {
        match declarations(body).remove(0) {
            Declaration::Type(decl) => decl.underlying,
            other => panic!("not a type: {other:?}"),
        }
    }

    fn names(idents: &[crate::ast::Ident]) -> Vec<&str> {
        idents.iter().map(|ident| ident.name.as_str()).collect()
    }

    #[test]
    fn reads_types() {
        let source =
            "TYPE Label = STRING(255);\n WHERE\n  WR1 : SELF <> '';\n  SELF <> 'x';\nEND_TYPE;";
        let Declaration::Type(decl) = declarations(source).remove(0) else {
            panic!("not a type");
        };
        assert_eq!(decl.name.name, "Label");
        assert!(matches!(
            decl.underlying,
            UnderlyingType::Concrete(TypeSpec::Simple(SimpleType::String(Some(_))))
        ));
        assert_eq!(decl.where_rules.len(), 2);
        assert_eq!(decl.where_rules[0].label.as_ref().unwrap().name, "WR1");
        assert!(decl.where_rules[1].label.is_none());
    }

    #[test]
    fn reads_enumerations() {
        let UnderlyingType::Enumeration {
            extensible,
            based_on,
            items,
        } = underlying("TYPE E = ENUMERATION OF (A, B); END_TYPE;")
        else {
            panic!("not an enumeration");
        };
        assert!(!extensible);
        assert!(based_on.is_none());
        assert_eq!(names(&items), ["A", "B"]);

        let UnderlyingType::Enumeration {
            extensible,
            based_on,
            items,
        } = underlying("TYPE F = EXTENSIBLE ENUMERATION BASED_ON E WITH (C); END_TYPE;")
        else {
            panic!("not an enumeration");
        };
        assert!(extensible);
        assert_eq!(based_on.unwrap().name, "E");
        assert_eq!(names(&items), ["C"]);

        let UnderlyingType::Enumeration { items, .. } =
            underlying("TYPE G = EXTENSIBLE ENUMERATION; END_TYPE;")
        else {
            panic!("not an enumeration");
        };
        assert!(items.is_empty());
    }

    #[test]
    fn reads_selects() {
        let UnderlyingType::Select {
            extensible,
            generic_entity,
            based_on,
            items,
        } = underlying("TYPE S = SELECT (A, B); END_TYPE;")
        else {
            panic!("not a select");
        };
        assert!(!extensible && !generic_entity && based_on.is_none());
        assert_eq!(names(&items), ["A", "B"]);

        let UnderlyingType::Select {
            extensible,
            generic_entity,
            based_on,
            items,
        } = underlying("TYPE T = EXTENSIBLE GENERIC_ENTITY SELECT BASED_ON S; END_TYPE;")
        else {
            panic!("not a select");
        };
        assert!(extensible && generic_entity);
        assert_eq!(based_on.unwrap().name, "S");
        assert!(items.is_empty());
    }

    #[test]
    fn rejects_malformed_types() {
        assert_eq!(
            error("SCHEMA s; TYPE t = EXTENSIBLE REAL; END_TYPE; END_SCHEMA;"),
            "line 1, column 31: expected ENUMERATION or SELECT, found `REAL`"
        );
        assert_eq!(
            error(
                "SCHEMA s; TYPE t = EXTENSIBLE GENERIC_ENTITY ENUMERATION; END_TYPE; END_SCHEMA;"
            ),
            "line 1, column 46: expected SELECT, found `ENUMERATION`"
        );
        assert_eq!(
            error("SCHEMA s; TYPE t = REAL END_TYPE; END_SCHEMA;"),
            "line 1, column 25: expected `;`, found `END_TYPE`"
        );
    }

    #[test]
    fn reads_entities() {
        let entity = entity(
            "ENTITY Wall\n ABSTRACT SUPERTYPE OF (ONEOF (A, B) ANDOR C AND (D))\n SUBTYPE OF (Element, Thing);\n\
             Name, Tag : OPTIONAL STRING;\n SELF\\Element.Size RENAMED Width : REAL;\n\
             DERIVE\n Area : REAL := Width * 2;\n SELF\\Thing.Kind : INTEGER := 1;\n\
             INVERSE\n Holes : SET [0:?] OF Opening FOR Wall;\n Parent : Group FOR Group.Members;\n Other : BAG OF X FOR Y;\n\
             UNIQUE\n UR1 : Name, Tag;\n SELF\\Element.Id;\n\
             WHERE\n Positive : Width > 0;\n\
             END_ENTITY;",
        );
        assert_eq!(entity.name.name, "Wall");
        assert!(entity.is_abstract);
        assert_eq!(names(&entity.subtype_of), ["Element", "Thing"]);
        let Some(SupertypeExpr::AndOr(factors)) = &entity.supertype_of else {
            panic!("not ANDOR");
        };
        assert!(matches!(&factors[0], SupertypeExpr::OneOf(choices) if choices.len() == 2));
        assert!(matches!(&factors[1], SupertypeExpr::And(terms) if terms.len() == 2));

        assert_eq!(entity.explicit.len(), 2);
        assert_eq!(entity.explicit[0].names.len(), 2);
        assert!(entity.explicit[0].optional);
        let AttrName::Redeclared {
            entity: parent,
            attribute,
            renamed,
        } = &entity.explicit[1].names[0]
        else {
            panic!("not a redeclaration");
        };
        assert_eq!(
            (
                parent.name.as_str(),
                attribute.name.as_str(),
                renamed.as_ref().unwrap().name.as_str()
            ),
            ("Element", "Size", "Width")
        );

        assert_eq!(entity.derived.len(), 2);
        assert!(matches!(
            entity.derived[1].name,
            AttrName::Redeclared { renamed: None, .. }
        ));
        assert_eq!(entity.derived[1].value, Expr::Integer(1));

        assert_eq!(entity.inverse.len(), 3);
        assert!(matches!(
            entity.inverse[0].aggregate,
            Some((AggregateKind::Set, Some(_)))
        ));
        assert_eq!(entity.inverse[0].attribute.name, "Wall");
        assert!(entity.inverse[1].aggregate.is_none());
        assert_eq!(entity.inverse[1].for_entity.as_ref().unwrap().name, "Group");
        assert!(matches!(
            entity.inverse[2].aggregate,
            Some((AggregateKind::Bag, None))
        ));

        assert_eq!(entity.unique.len(), 2);
        assert_eq!(entity.unique[0].attributes.len(), 2);
        assert!(entity.unique[1].label.is_none());
        assert_eq!(entity.where_rules.len(), 1);
    }

    #[test]
    fn reads_supertype_forms() {
        let plain = entity("ENTITY A SUPERTYPE OF (B); END_ENTITY;");
        assert!(!plain.is_abstract);
        assert!(matches!(plain.supertype_of, Some(SupertypeExpr::Entity(_))));
        let bare = entity("ENTITY A ABSTRACT; END_ENTITY;");
        assert!(bare.is_abstract && bare.supertype_of.is_none());
        let abstract_supertype = entity("ENTITY A ABSTRACT SUPERTYPE; END_ENTITY;");
        assert!(abstract_supertype.is_abstract && abstract_supertype.supertype_of.is_none());
    }

    #[test]
    fn rejects_malformed_entities() {
        assert_eq!(
            error("SCHEMA s; ENTITY A SUPERTYPE (B); END_ENTITY; END_SCHEMA;"),
            "line 1, column 30: expected OF, found `(`"
        );
        assert_eq!(
            error("SCHEMA s; ENTITY A SUBTYPE (B); END_ENTITY; END_SCHEMA;"),
            "line 1, column 28: expected OF, found `(`"
        );
        assert_eq!(
            error("SCHEMA s; ENTITY A; x : REAL END_ENTITY; END_SCHEMA;"),
            "line 1, column 30: expected `;`, found `END_ENTITY`"
        );
        assert_eq!(
            error("SCHEMA s; ENTITY A; SELF.x : REAL; END_ENTITY; END_SCHEMA;"),
            "line 1, column 25: expected `\\`, found `.`"
        );
        assert_eq!(
            error("SCHEMA s; ENTITY A; INVERSE x : SET B FOR y; END_ENTITY; END_SCHEMA;"),
            "line 1, column 37: expected OF, found `B`"
        );
        assert_eq!(
            error("SCHEMA s; ENTITY A; INVERSE x : B y; END_ENTITY; END_SCHEMA;"),
            "line 1, column 35: expected FOR, found `y`"
        );
        assert_eq!(
            error("SCHEMA s; ENTITY A SUPERTYPE OF (ONEOF (B C)); END_ENTITY; END_SCHEMA;"),
            "line 1, column 43: expected `)`, found `C`"
        );
        assert_eq!(
            error("SCHEMA s; ENTITY A SUPERTYPE OF ((B); END_ENTITY; END_SCHEMA;"),
            "line 1, column 37: expected `)`, found `;`"
        );
    }

    #[test]
    fn reads_functions_and_procedures() {
        let mut declarations = declarations(
            "FUNCTION F (A, B : INTEGER; C : LIST OF GENERIC : T) : BOOLEAN;\n\
             TYPE Inner = REAL; END_TYPE;\n\
             CONSTANT K : INTEGER := 2; END_CONSTANT;\n\
             LOCAL x, y : INTEGER := 0; z : REAL; END_LOCAL;\n\
             RETURN (A = B);\n\
             END_FUNCTION;\n\
             FUNCTION G : INTEGER; RETURN (1); END_FUNCTION;\n\
             PROCEDURE P (VAR X : INTEGER; Y : REAL); X := 1; END_PROCEDURE;\n\
             PROCEDURE Q; END_PROCEDURE;",
        );
        let Declaration::Function(function) = declarations.remove(0) else {
            panic!("not a function");
        };
        assert_eq!(function.parameters.len(), 2);
        assert_eq!(names(&function.parameters[0].names), ["A", "B"]);
        assert!(matches!(
            function.result,
            TypeSpec::Simple(SimpleType::Boolean)
        ));
        assert_eq!(function.algorithm.declarations.len(), 2);
        assert_eq!(function.algorithm.locals.len(), 2);
        assert!(function.algorithm.locals[0].initial.is_some());
        assert!(function.algorithm.locals[1].initial.is_none());
        assert_eq!(function.algorithm.body.len(), 1);
        let Declaration::Function(function) = declarations.remove(0) else {
            panic!("not a function");
        };
        assert!(function.parameters.is_empty());
        let Declaration::Procedure(procedure) = declarations.remove(0) else {
            panic!("not a procedure");
        };
        assert!(procedure.parameters[0].var && !procedure.parameters[1].var);
        let Declaration::Procedure(procedure) = declarations.remove(0) else {
            panic!("not a procedure");
        };
        assert!(procedure.algorithm.body.is_empty());
    }

    #[test]
    fn reads_rules_and_constants() {
        let mut declarations = declarations(
            "RULE Single FOR (Project, Site);\n LOCAL n : INTEGER; END_LOCAL;\n n := SIZEOF(Project);\n\
             WHERE\n WR1 : n = 1;\nEND_RULE;\n\
             CONSTANT Zero : REAL := 0.0; One : REAL := 1.0; END_CONSTANT;",
        );
        let Declaration::Rule(rule) = declarations.remove(0) else {
            panic!("not a rule");
        };
        assert_eq!(names(&rule.entities), ["Project", "Site"]);
        assert_eq!(rule.algorithm.body.len(), 1);
        assert_eq!(rule.where_rules.len(), 1);
        let Declaration::Constant(constants) = declarations.remove(0) else {
            panic!("not constants");
        };
        assert_eq!(constants.len(), 2);
    }

    #[test]
    fn rejects_malformed_algorithms() {
        assert_eq!(
            error("SCHEMA s; RULE r FOR (A); END_RULE; END_SCHEMA;"),
            "line 1, column 27: expected a statement, found `END_RULE`"
        );
        assert_eq!(
            error(
                "SCHEMA s; FUNCTION f : REAL; RULE r FOR (A); WHERE x; END_RULE; END_FUNCTION; END_SCHEMA;"
            ),
            "line 1, column 30: expected a statement, found `RULE`"
        );
        assert_eq!(
            error("SCHEMA s; FUNCTION f (a : REAL : REAL; END_FUNCTION; END_SCHEMA;"),
            "line 1, column 32: expected `)`, found `:`"
        );
    }

    #[test]
    fn rejects_unsupported_constructs() {
        assert_eq!(
            error("SCHEMA s; USE FROM other; END_SCHEMA;"),
            "line 1, column 11: an interface specification (USE or REFERENCE) is not supported"
        );
        assert_eq!(
            error("SCHEMA s; SUBTYPE_CONSTRAINT c FOR a; END_SUBTYPE_CONSTRAINT; END_SCHEMA;"),
            "line 1, column 11: SUBTYPE_CONSTRAINT is not supported"
        );
        assert_eq!(
            error("SCHEMA s; 42; END_SCHEMA;"),
            "line 1, column 11: expected a declaration or END_SCHEMA, found an integer"
        );
    }
}
