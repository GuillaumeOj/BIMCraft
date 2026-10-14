//! The resolved model of a schema: its data types and entities, with names checked and each
//! entity's explicit attributes flattened in STEP order.
//!
//! Functions, procedures, rules and constants are only checked for name clashes: code
//! generation does not need their bodies.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{
    AggregateKind, AttrName, Bounds, Declaration, EntityDecl, Expr, Ident, Schema, SimpleType,
    SupertypeExpr, TypeDecl, TypeSpec, UnaryOp, UnderlyingType,
};
use crate::error::{Error, ErrorKind};

/// A resolved schema. Types and entities are keyed by their upper-case name, so iteration is in
/// a stable, case-insensitive name order.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    /// The schema name, such as `IFC4X3_ADD2`.
    pub name: String,
    /// Defined types, enumerations and selects.
    pub types: BTreeMap<String, TypeDef>,
    /// Entities.
    pub entities: BTreeMap<String, Entity>,
    /// Names of functions, procedures and rules, in name order.
    pub algorithms: Vec<String>,
}

/// A type declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeDef {
    /// The type name as declared.
    pub name: String,
    /// What it stands for.
    pub kind: TypeKind,
}

/// The kinds of type declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    /// A defined type: a new name for a simple, named or aggregate type.
    Defined(AttrType),
    /// An enumeration: its items, those of the enumeration it is based on first.
    Enumeration(Vec<String>),
    /// A select: its direct members, those of the select it is based on first.
    Select(Vec<String>),
}

/// A type that a value can have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttrType {
    /// `BINARY`
    Binary,
    /// `BOOLEAN`
    Boolean,
    /// `INTEGER`
    Integer,
    /// `LOGICAL`
    Logical,
    /// `NUMBER`
    Number,
    /// `REAL`
    Real,
    /// `STRING`
    String,
    /// A declared type or entity, by its declared name.
    Named(String),
    /// An aggregate.
    Aggregate(Box<Aggregate>),
}

/// An aggregate type, with its bounds as element counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aggregate {
    /// Which aggregate.
    pub kind: AggregateKind,
    /// The fewest elements.
    pub min: usize,
    /// The most elements; `None` when unbounded or computed at run time.
    pub max: Option<usize>,
    /// Whether elements may be absent (`ARRAY OF OPTIONAL`).
    pub optional_elements: bool,
    /// Whether elements must be distinct (`UNIQUE`).
    pub unique: bool,
    /// The element type.
    pub element: AttrType,
}

/// An entity.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    /// The entity name as declared.
    pub name: String,
    /// Whether it is never instantiated on its own.
    pub is_abstract: bool,
    /// Its direct supertypes, by declared name.
    pub supertypes: Vec<String>,
    /// Its explicit attributes, inherited ones included, in STEP order.
    pub attributes: Vec<Attribute>,
}

/// One position of an entity in a STEP record.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    /// The attribute name in this entity: the declared name, or the new one when renamed.
    pub name: String,
    /// The entity that first declares the attribute.
    pub declared_by: String,
    /// The name `declared_by` gives it, which `SELF\entity.attribute` refers to even after a
    /// subtype renames it.
    pub declared_name: String,
    /// The value type, as redeclared by the entity or its supertypes.
    pub ty: AttrType,
    /// Whether the value may be absent.
    pub optional: bool,
    /// Whether a subtype turned the attribute into a derived one, written `*` in STEP.
    pub derived: bool,
}

/// What a declared name stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    /// An entity.
    Entity,
    /// A defined type.
    Defined,
    /// An enumeration.
    Enumeration,
    /// A select.
    Select,
}

impl Model {
    /// The entity or type declared as `name`, compared without regard to case.
    #[must_use]
    pub fn kind_of(&self, name: &str) -> Option<NameKind> {
        let key = name.to_ascii_uppercase();
        if self.entities.contains_key(&key) {
            return Some(NameKind::Entity);
        }
        self.types.get(&key).map(|def| match def.kind {
            TypeKind::Defined(_) => NameKind::Defined,
            TypeKind::Enumeration(_) => NameKind::Enumeration,
            TypeKind::Select(_) => NameKind::Select,
        })
    }

    /// The entity declared as `name`, compared without regard to case.
    #[must_use]
    pub fn entity(&self, name: &str) -> Option<&Entity> {
        self.entities.get(&name.to_ascii_uppercase())
    }

    /// The type declared as `name`, compared without regard to case.
    #[must_use]
    pub fn type_def(&self, name: &str) -> Option<&TypeDef> {
        self.types.get(&name.to_ascii_uppercase())
    }

    /// Every type and entity a select can hold, through nested selects, in name order.
    #[must_use]
    pub fn select_members(&self, name: &str) -> Vec<String> {
        let mut members = BTreeMap::new();
        let mut pending = vec![name.to_owned()];
        let mut seen = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current.to_ascii_uppercase()) {
                continue;
            }
            let Some(TypeKind::Select(items)) = self.type_def(&current).map(|def| &def.kind) else {
                continue;
            };
            for item in items {
                if self.kind_of(item) == Some(NameKind::Select) {
                    pending.push(item.clone());
                } else {
                    members.insert(item.to_ascii_uppercase(), item.clone());
                }
            }
        }
        members.into_values().collect()
    }
}

/// Resolves a parsed schema.
///
/// # Errors
///
/// Returns every problem found: duplicate names, references to undeclared types or entities,
/// supertype or type cycles, invalid redeclarations, generic types outside algorithms, and
/// invalid aggregate bounds.
pub fn resolve(schema: &Schema) -> Result<Model, Vec<Error>> {
    let mut resolver = Resolver::default();
    resolver.collect(schema);
    resolver.check_errors()?;
    let types = resolver.resolve_types();
    resolver.check_errors()?;
    let model = Model {
        name: schema.name.name.clone(),
        types,
        entities: BTreeMap::new(),
        algorithms: resolver.algorithms.values().cloned().collect(),
    };
    resolver.check_type_cycles(&model);
    resolver.check_errors()?;
    let entities = resolver.resolve_entities(&model);
    resolver.check_errors()?;
    Ok(Model { entities, ..model })
}

#[derive(Default)]
struct Resolver<'a> {
    types: BTreeMap<String, &'a TypeDecl>,
    entities: BTreeMap<String, &'a EntityDecl>,
    algorithms: BTreeMap<String, String>,
    /// Entities that could not be resolved, so their subtypes do not report them again.
    failed: BTreeSet<String>,
    errors: Vec<Error>,
}

impl<'a> Resolver<'a> {
    fn check_errors(&mut self) -> Result<(), Vec<Error>> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    fn error(&mut self, kind: ErrorKind, at: &Ident) {
        self.errors.push(Error::at(kind, at.position));
    }

    /// Indexes the schema's declarations by upper-case name, reporting clashes.
    fn collect(&mut self, schema: &'a Schema) {
        let mut names = BTreeSet::new();
        for declaration in &schema.declarations {
            let key = |ident: &Ident| ident.name.to_ascii_uppercase();
            let idents: Vec<&Ident> = match declaration {
                Declaration::Type(decl) => {
                    self.types.insert(key(&decl.name), decl);
                    vec![&decl.name]
                }
                Declaration::Entity(decl) => {
                    self.entities.insert(key(&decl.name), decl);
                    vec![&decl.name]
                }
                Declaration::Function(decl) => self.add_algorithm(&decl.name),
                Declaration::Procedure(decl) => self.add_algorithm(&decl.name),
                Declaration::Rule(decl) => self.add_algorithm(&decl.name),
                Declaration::Constant(constants) => {
                    constants.iter().map(|constant| &constant.name).collect()
                }
            };
            for ident in idents {
                if !names.insert(key(ident)) {
                    self.error(ErrorKind::Duplicate(ident.name.clone()), ident);
                }
            }
        }
        // Rules may name entities declared after them, so they are checked once all are known.
        for declaration in &schema.declarations {
            if let Declaration::Rule(rule) = declaration {
                for entity in &rule.entities {
                    self.check_entity(entity);
                }
            }
        }
    }

    /// Records a function, procedure or rule, and returns its name to check for clashes.
    fn add_algorithm<'n>(&mut self, name: &'n Ident) -> Vec<&'n Ident> {
        self.algorithms
            .insert(name.name.to_ascii_uppercase(), name.name.clone());
        vec![name]
    }

    /// Reports `ident` when it names no entity.
    fn check_entity(&mut self, ident: &Ident) {
        if !self.entities.contains_key(&ident.name.to_ascii_uppercase()) {
            let kind = ErrorKind::Undefined {
                name: ident.name.clone(),
                expected: "entity",
            };
            self.error(kind, ident);
        }
    }

    /// The declared name of the type or entity `ident` names, reporting it when there is none.
    fn type_name(&mut self, ident: &Ident) -> Option<String> {
        let key = ident.name.to_ascii_uppercase();
        if let Some(decl) = self.types.get(&key) {
            return Some(decl.name.name.clone());
        }
        if let Some(decl) = self.entities.get(&key) {
            return Some(decl.name.name.clone());
        }
        let kind = ErrorKind::Undefined {
            name: ident.name.clone(),
            expected: "type or entity",
        };
        self.error(kind, ident);
        None
    }

    fn resolve_types(&mut self) -> BTreeMap<String, TypeDef> {
        let decls: Vec<(String, &'a TypeDecl)> = self
            .types
            .iter()
            .map(|(key, decl)| (key.clone(), *decl))
            .collect();
        let mut types = BTreeMap::new();
        for (key, decl) in decls {
            let kind = match &decl.underlying {
                UnderlyingType::Concrete(spec) => {
                    self.attr_type(spec, &decl.name).map(TypeKind::Defined)
                }
                UnderlyingType::Enumeration {
                    based_on, items, ..
                }
                | UnderlyingType::Select {
                    based_on, items, ..
                } => {
                    let is_select = matches!(decl.underlying, UnderlyingType::Select { .. });
                    let kind = if is_select {
                        TypeKind::Select
                    } else {
                        TypeKind::Enumeration
                    };
                    let mut visiting = BTreeSet::new();
                    self.extended_items(
                        &decl.name,
                        based_on.as_ref(),
                        items,
                        is_select,
                        &mut visiting,
                    )
                    .map(kind)
                }
            };
            if let Some(kind) = kind {
                let name = decl.name.name.clone();
                types.insert(key, TypeDef { name, kind });
            }
        }
        types
    }

    /// The items of the enumeration, or the members of the select, `name`: those of the type it
    /// is based on first, then its `own`. `visiting` holds the types being extended, to report
    /// cycles.
    fn extended_items(
        &mut self,
        name: &Ident,
        based_on: Option<&'a Ident>,
        own: &'a [Ident],
        is_select: bool,
        visiting: &mut BTreeSet<String>,
    ) -> Option<Vec<String>> {
        if !visiting.insert(name.name.to_ascii_uppercase()) {
            self.error(ErrorKind::TypeCycle(name.name.clone()), name);
            return None;
        }
        let mut items = Vec::new();
        if let Some(base) = based_on {
            let base_decl = self.types.get(&base.name.to_ascii_uppercase()).copied();
            items = match base_decl.map(|base_decl| (&base_decl.name, &base_decl.underlying)) {
                Some((
                    base_name,
                    underlying @ (UnderlyingType::Enumeration {
                        based_on, items, ..
                    }
                    | UnderlyingType::Select {
                        based_on, items, ..
                    }),
                )) if is_select == matches!(underlying, UnderlyingType::Select { .. }) => {
                    self.extended_items(base_name, based_on.as_ref(), items, is_select, visiting)?
                }
                _ => {
                    let expected = if is_select { "select" } else { "enumeration" };
                    let kind = ErrorKind::Undefined {
                        name: base.name.clone(),
                        expected,
                    };
                    self.error(kind, base);
                    return None;
                }
            };
        }
        let mut seen: BTreeSet<String> =
            items.iter().map(|item| item.to_ascii_uppercase()).collect();
        for item in own {
            let member = if is_select {
                self.type_name(item)?
            } else {
                item.name.clone()
            };
            if !seen.insert(member.to_ascii_uppercase()) {
                let duplicate = format!("{}.{}", name.name, item.name);
                self.error(ErrorKind::Duplicate(duplicate), item);
                return None;
            }
            items.push(member);
        }
        Some(items)
    }

    /// The type of an attribute or a defined type; `owner` names the declaration for errors.
    fn attr_type(&mut self, spec: &TypeSpec, owner: &Ident) -> Option<AttrType> {
        let ty = match spec {
            TypeSpec::Simple(simple) => match simple {
                SimpleType::Binary(_) => AttrType::Binary,
                SimpleType::Boolean => AttrType::Boolean,
                SimpleType::Integer => AttrType::Integer,
                SimpleType::Logical => AttrType::Logical,
                SimpleType::Number => AttrType::Number,
                SimpleType::Real(_) => AttrType::Real,
                SimpleType::String(_) => AttrType::String,
            },
            TypeSpec::Named(name) => AttrType::Named(self.type_name(name)?),
            TypeSpec::Aggregate(aggregate) => {
                let element = self.attr_type(&aggregate.element, owner)?;
                let Some((min, max)) = counts(aggregate.kind, aggregate.bounds.as_ref()) else {
                    self.error(ErrorKind::InvalidBounds(owner.name.clone()), owner);
                    return None;
                };
                AttrType::Aggregate(Box::new(Aggregate {
                    kind: aggregate.kind,
                    min,
                    max,
                    optional_elements: aggregate.optional,
                    unique: aggregate.unique,
                    element,
                }))
            }
            TypeSpec::GenericAggregate { .. }
            | TypeSpec::Generic(_)
            | TypeSpec::GenericEntity(_) => {
                self.error(ErrorKind::GenericType(owner.name.clone()), owner);
                return None;
            }
        };
        Some(ty)
    }

    /// Reports types that contain themselves by value: a defined type through its underlying
    /// type, or a select through its members, with no aggregate in between.
    fn check_type_cycles(&mut self, model: &Model) {
        for def in model.types.values() {
            let mut pending = by_value(model, def);
            let mut seen = BTreeSet::new();
            while let Some(name) = pending.pop() {
                if name.eq_ignore_ascii_case(&def.name) {
                    let ident = &self.types[&def.name.to_ascii_uppercase()].name;
                    self.error(ErrorKind::TypeCycle(def.name.clone()), ident);
                    break;
                }
                if seen.insert(name.to_ascii_uppercase())
                    && let Some(next) = model.type_def(&name)
                {
                    pending.extend(by_value(model, next));
                }
            }
        }
    }

    fn resolve_entities(&mut self, model: &Model) -> BTreeMap<String, Entity> {
        let decls: Vec<&'a EntityDecl> = self.entities.values().copied().collect();
        for decl in &decls {
            self.check_entity_references(decl);
        }
        if !self.errors.is_empty() {
            return BTreeMap::new();
        }
        let mut entities = BTreeMap::new();
        for decl in decls {
            let mut visiting = BTreeSet::new();
            self.entity(decl, model, &mut entities, &mut visiting);
        }
        entities
    }

    /// Checks the names an entity refers to outside its explicit attributes.
    fn check_entity_references(&mut self, decl: &EntityDecl) {
        for supertype in &decl.subtype_of {
            self.check_entity(supertype);
        }
        if let Some(expr) = &decl.supertype_of {
            let mut pending = vec![expr];
            while let Some(expr) = pending.pop() {
                match expr {
                    SupertypeExpr::Entity(ident) => {
                        self.check_entity(ident);
                    }
                    SupertypeExpr::OneOf(items)
                    | SupertypeExpr::And(items)
                    | SupertypeExpr::AndOr(items) => pending.extend(items),
                }
            }
        }
        for inverse in &decl.inverse {
            self.check_entity(&inverse.entity);
            if let Some(for_entity) = &inverse.for_entity {
                self.check_entity(for_entity);
            }
        }
    }

    /// Resolves an entity after its supertypes, memoised in `done`. `visiting` holds the
    /// entities being resolved, to report supertype cycles.
    fn entity(
        &mut self,
        decl: &'a EntityDecl,
        model: &Model,
        done: &mut BTreeMap<String, Entity>,
        visiting: &mut BTreeSet<String>,
    ) -> Option<()> {
        let key = decl.name.name.to_ascii_uppercase();
        if done.contains_key(&key) {
            return Some(());
        }
        if self.failed.contains(&key) {
            return None;
        }
        if !visiting.insert(key.clone()) {
            self.error(
                ErrorKind::SupertypeCycle(decl.name.name.clone()),
                &decl.name,
            );
            return None;
        }
        let entity = self.flatten(decl, model, done, visiting);
        visiting.remove(&key);
        let Some(entity) = entity else {
            self.failed.insert(key);
            return None;
        };
        done.insert(key, entity);
        Some(())
    }

    fn flatten(
        &mut self,
        decl: &'a EntityDecl,
        model: &Model,
        done: &mut BTreeMap<String, Entity>,
        visiting: &mut BTreeSet<String>,
    ) -> Option<Entity> {
        let mut attributes: Vec<Attribute> = Vec::new();
        let mut supertypes = Vec::new();
        for supertype in &decl.subtype_of {
            let super_decl = self.entities[&supertype.name.to_ascii_uppercase()];
            self.entity(super_decl, model, done, visiting)?;
            let parent = &done[&super_decl.name.name.to_ascii_uppercase()];
            supertypes.push(parent.name.clone());
            for attribute in &parent.attributes {
                let inherited = attributes.iter().any(|existing| {
                    existing.declared_by == attribute.declared_by
                        && existing.declared_name == attribute.declared_name
                });
                if !inherited {
                    attributes.push(attribute.clone());
                }
            }
        }
        self.own_attributes(decl, &mut attributes)?;
        Some(Entity {
            name: decl.name.name.clone(),
            is_abstract: decl.is_abstract,
            supertypes,
            attributes,
        })
    }

    /// Adds an entity's explicit attributes to the inherited ones, applies its redeclarations
    /// and marks the attributes it derives.
    fn own_attributes(&mut self, decl: &EntityDecl, attributes: &mut Vec<Attribute>) -> Option<()> {
        for explicit in &decl.explicit {
            let ty = self.attr_type(&explicit.ty, &decl.name)?;
            for name in &explicit.names {
                match name {
                    AttrName::New(ident) => {
                        if attributes
                            .iter()
                            .any(|existing| existing.name.eq_ignore_ascii_case(&ident.name))
                        {
                            let duplicate = format!("{}.{}", decl.name.name, ident.name);
                            self.error(ErrorKind::Duplicate(duplicate), ident);
                            return None;
                        }
                        attributes.push(Attribute {
                            name: ident.name.clone(),
                            declared_by: decl.name.name.clone(),
                            declared_name: ident.name.clone(),
                            ty: ty.clone(),
                            optional: explicit.optional,
                            derived: false,
                        });
                    }
                    AttrName::Redeclared {
                        entity,
                        attribute,
                        renamed,
                    } => {
                        let attribute =
                            self.redeclared(decl, entity, attribute, renamed.as_ref(), attributes)?;
                        attribute.ty = ty.clone();
                        attribute.optional = explicit.optional;
                    }
                }
            }
        }
        for derived in &decl.derived {
            if let AttrName::Redeclared {
                entity,
                attribute,
                renamed,
            } = &derived.name
            {
                self.redeclared(decl, entity, attribute, renamed.as_ref(), attributes)?
                    .derived = true;
            }
        }
        Some(())
    }

    /// The inherited attribute that `SELF\entity.attribute` names, renamed when asked.
    fn redeclared<'b>(
        &mut self,
        decl: &EntityDecl,
        entity: &Ident,
        attribute: &Ident,
        renamed: Option<&Ident>,
        attributes: &'b mut [Attribute],
    ) -> Option<&'b mut Attribute> {
        let index = attributes.iter().position(|existing| {
            existing.declared_by.eq_ignore_ascii_case(&entity.name)
                && existing.declared_name.eq_ignore_ascii_case(&attribute.name)
        });
        let Some(index) = index else {
            let kind = ErrorKind::UnknownRedeclaration {
                entity: decl.name.name.clone(),
                attribute: format!("{}.{}", entity.name, attribute.name),
            };
            self.error(kind, attribute);
            return None;
        };
        let found = &mut attributes[index];
        if let Some(renamed) = renamed {
            found.name.clone_from(&renamed.name);
        }
        Some(found)
    }
}

/// The types a type holds by value: a defined type's underlying named type, or the defined
/// types and enumerations of a select, through nested selects. Entities are held by reference.
fn by_value(model: &Model, def: &TypeDef) -> Vec<String> {
    match &def.kind {
        TypeKind::Defined(AttrType::Named(name)) => vec![name.clone()],
        TypeKind::Select(_) => model
            .select_members(&def.name)
            .into_iter()
            .filter(|member| model.kind_of(member) != Some(NameKind::Entity))
            .collect(),
        TypeKind::Defined(_) | TypeKind::Enumeration(_) => Vec::new(),
    }
}

/// The fewest and most elements that `bounds` allow for an aggregate of `kind`, or `None` when
/// the bounds are invalid. Bounds computed at run time allow any count.
fn counts(kind: AggregateKind, bounds: Option<&Bounds>) -> Option<(usize, Option<usize>)> {
    let Some(bounds) = bounds else {
        return Some((0, None));
    };
    let lower = literal(&bounds.lower);
    let upper = literal(&bounds.upper);
    if kind == AggregateKind::Array {
        return match (lower, upper) {
            (Some(lower), Some(upper)) => {
                let count = usize::try_from(i128::from(upper) - i128::from(lower) + 1).ok()?;
                (count > 0).then_some((count, Some(count)))
            }
            _ => Some((0, None)),
        };
    }
    let min = match lower {
        Some(lower) => usize::try_from(lower).ok()?,
        None => 0,
    };
    let max = match upper {
        Some(upper) => Some(usize::try_from(upper).ok().filter(|&upper| upper >= min)?),
        None => None,
    };
    Some((min, max))
}

/// The value of an integer literal, possibly negated; `None` for `?` and computed bounds.
fn literal(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Integer(value) => Some(*value),
        Expr::Unary {
            op: UnaryOp::Minus,
            operand,
        } => match **operand {
            Expr::Integer(value) => value.checked_neg(),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn model(body: &str) -> Model {
        let schema = parse(&format!("SCHEMA s;\n{body}\nEND_SCHEMA;")).unwrap();
        resolve(&schema).unwrap()
    }

    fn errors(body: &str) -> Vec<String> {
        let schema = parse(&format!("SCHEMA s;\n{body}\nEND_SCHEMA;")).unwrap();
        resolve(&schema)
            .unwrap_err()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn attribute_names(entity: &Entity) -> Vec<&str> {
        entity
            .attributes
            .iter()
            .map(|attribute| attribute.name.as_str())
            .collect()
    }

    #[test]
    fn resolves_types() {
        let model = model(
            "TYPE Label = STRING; END_TYPE;\n\
             TYPE Point = ARRAY [1:3] OF OPTIONAL REAL; END_TYPE;\n\
             TYPE Kind = ENUMERATION OF (A, B); END_TYPE;\n\
             TYPE More = ENUMERATION BASED_ON Kind WITH (C); END_TYPE;\n\
             TYPE Value = SELECT (Label, Thing); END_TYPE;\n\
             TYPE Wider = SELECT BASED_ON Value WITH (Kind); END_TYPE;\n\
             ENTITY Thing; END_ENTITY;\n\
             FUNCTION f : INTEGER; RETURN (1); END_FUNCTION;\n\
             RULE r FOR (Thing); WHERE TRUE; END_RULE;\n\
             PROCEDURE p; END_PROCEDURE;\n\
             CONSTANT k : INTEGER := 1; END_CONSTANT;",
        );
        assert_eq!(model.name, "s");
        assert_eq!(
            model.type_def("label").unwrap().kind,
            TypeKind::Defined(AttrType::String)
        );
        assert_eq!(
            model.type_def("Point").unwrap().kind,
            TypeKind::Defined(AttrType::Aggregate(Box::new(Aggregate {
                kind: AggregateKind::Array,
                min: 3,
                max: Some(3),
                optional_elements: true,
                unique: false,
                element: AttrType::Real,
            })))
        );
        assert_eq!(
            model.type_def("More").unwrap().kind,
            TypeKind::Enumeration(vec!["A".into(), "B".into(), "C".into()])
        );
        assert_eq!(
            model.type_def("Wider").unwrap().kind,
            TypeKind::Select(vec!["Label".into(), "Thing".into(), "Kind".into()])
        );
        assert_eq!(model.kind_of("thing"), Some(NameKind::Entity));
        assert_eq!(model.kind_of("Label"), Some(NameKind::Defined));
        assert_eq!(model.kind_of("Kind"), Some(NameKind::Enumeration));
        assert_eq!(model.kind_of("Value"), Some(NameKind::Select));
        assert_eq!(model.kind_of("f"), None);
        assert_eq!(model.algorithms, ["f", "p", "r"]);
    }

    #[test]
    fn maps_every_simple_type() {
        let model = model(
            "TYPE B = BINARY; END_TYPE; TYPE O = BOOLEAN; END_TYPE; TYPE I = INTEGER; END_TYPE;\n\
             TYPE L = LOGICAL; END_TYPE; TYPE N = NUMBER; END_TYPE; TYPE R = REAL(4); END_TYPE;",
        );
        let kinds: Vec<&TypeKind> = model.types.values().map(|def| &def.kind).collect();
        assert_eq!(
            kinds,
            [
                &TypeKind::Defined(AttrType::Binary),
                &TypeKind::Defined(AttrType::Integer),
                &TypeKind::Defined(AttrType::Logical),
                &TypeKind::Defined(AttrType::Number),
                &TypeKind::Defined(AttrType::Boolean),
                &TypeKind::Defined(AttrType::Real),
            ]
        );
    }

    #[test]
    fn collects_select_members_through_nested_selects() {
        let model = model(
            "TYPE A = SELECT (B, X); END_TYPE;\n\
             TYPE B = SELECT (C, A); END_TYPE;\n\
             TYPE C = INTEGER; END_TYPE;\n\
             ENTITY X; END_ENTITY;",
        );
        assert_eq!(model.select_members("A"), ["C", "X"]);
        assert!(model.select_members("C").is_empty());
    }

    #[test]
    fn computes_aggregate_counts() {
        let bounds = |lower: Expr, upper: Expr| Some(Bounds { lower, upper });
        let int = Expr::Integer;
        let neg = |value| Expr::Unary {
            op: UnaryOp::Minus,
            operand: Box::new(Expr::Integer(value)),
        };
        let name = || {
            Expr::Name(Ident {
                name: "n".into(),
                position: crate::Position::START,
            })
        };
        let cases = [
            (AggregateKind::List, None, Some((0, None))),
            (
                AggregateKind::List,
                bounds(int(2), Expr::Indeterminate),
                Some((2, None)),
            ),
            (
                AggregateKind::Set,
                bounds(int(1), int(4)),
                Some((1, Some(4))),
            ),
            (
                AggregateKind::Bag,
                bounds(name(), int(4)),
                Some((0, Some(4))),
            ),
            (AggregateKind::List, bounds(int(3), int(2)), None),
            (AggregateKind::List, bounds(neg(1), int(2)), None),
            (
                AggregateKind::Array,
                bounds(int(0), int(2)),
                Some((3, Some(3))),
            ),
            (
                AggregateKind::Array,
                bounds(neg(1), int(1)),
                Some((3, Some(3))),
            ),
            (
                AggregateKind::Array,
                bounds(int(0), name()),
                Some((0, None)),
            ),
            (AggregateKind::Array, bounds(int(2), int(1)), None),
            (
                AggregateKind::Array,
                bounds(neg(i64::MAX), int(i64::MAX)),
                Some((usize::MAX, Some(usize::MAX))),
            ),
        ];
        for (kind, bounds, expected) in cases {
            assert_eq!(
                counts(kind, bounds.as_ref()),
                expected,
                "{kind:?} {bounds:?}"
            );
        }
        let min = Expr::Unary {
            op: UnaryOp::Minus,
            operand: Box::new(Expr::Integer(i64::MIN)),
        };
        assert_eq!(literal(&min), None);
        let not = Expr::Unary {
            op: UnaryOp::Not,
            operand: Box::new(Expr::Integer(1)),
        };
        assert_eq!(literal(&not), None);
        let minus_name = Expr::Unary {
            op: UnaryOp::Minus,
            operand: Box::new(name()),
        };
        assert_eq!(literal(&minus_name), None);
    }

    #[test]
    fn flattens_attributes_in_step_order() {
        let model = model(
            "ENTITY Root ABSTRACT SUPERTYPE OF (ONEOF (Mid)); Id : STRING; Name : OPTIONAL STRING; END_ENTITY;\n\
             ENTITY Mid SUBTYPE OF (Root); Size : REAL; END_ENTITY;\n\
             ENTITY Leaf SUBTYPE OF (Mid);\n\
               SELF\\Mid.Size RENAMED Width : INTEGER;\n\
               Tag : OPTIONAL STRING;\n\
             DERIVE\n\
               SELF\\Root.Name : STRING := 'x';\n\
               Area : REAL := 1.0;\n\
             INVERSE\n\
               Owner : SET OF Mid FOR Size;\n\
             END_ENTITY;",
        );
        let root = model.entity("root").unwrap();
        assert!(root.is_abstract);
        assert!(root.supertypes.is_empty());
        let leaf = model.entity("Leaf").unwrap();
        assert!(!leaf.is_abstract);
        assert_eq!(leaf.supertypes, ["Mid"]);
        assert_eq!(attribute_names(leaf), ["Id", "Name", "Width", "Tag"]);
        assert_eq!(leaf.attributes[1].declared_by, "Root");
        assert!(leaf.attributes[1].derived);
        assert_eq!(leaf.attributes[2].ty, AttrType::Integer);
        assert_eq!(leaf.attributes[2].declared_by, "Mid");
        assert!(leaf.attributes[3].optional);
        let mid = model.entity("Mid").unwrap();
        assert_eq!(attribute_names(mid), ["Id", "Name", "Size"]);
        assert!(!mid.attributes[1].derived);
    }

    #[test]
    fn redeclares_renamed_attributes_by_their_declared_name() {
        let model = model(
            "ENTITY Root; Name : STRING; END_ENTITY;\n\
             ENTITY Mid SUBTYPE OF (Root); SELF\\Root.Name RENAMED Label : STRING; END_ENTITY;\n\
             ENTITY Leaf SUBTYPE OF (Mid); DERIVE SELF\\Root.Name : STRING := 'x'; END_ENTITY;",
        );
        let leaf = model.entity("Leaf").unwrap();
        assert_eq!(attribute_names(leaf), ["Label"]);
        assert_eq!(leaf.attributes[0].declared_name, "Name");
        assert!(leaf.attributes[0].derived);
    }

    #[test]
    fn inherits_shared_supertypes_once() {
        let model = model(
            "ENTITY Top; Id : STRING; END_ENTITY;\n\
             ENTITY Left SUBTYPE OF (Top); L : REAL; END_ENTITY;\n\
             ENTITY Right SUBTYPE OF (Top); R : REAL; END_ENTITY;\n\
             ENTITY Both SUBTYPE OF (Left, Right); B : REAL; END_ENTITY;",
        );
        let both = model.entity("Both").unwrap();
        assert_eq!(attribute_names(both), ["Id", "L", "R", "B"]);
        assert_eq!(both.supertypes, ["Left", "Right"]);
    }

    #[test]
    fn reports_name_problems() {
        assert_eq!(
            errors(
                "TYPE a = REAL; END_TYPE; ENTITY A; END_ENTITY; CONSTANT a : INTEGER := 1; END_CONSTANT;"
            ),
            [
                "line 2, column 33: `A` is declared more than once",
                "line 2, column 57: `a` is declared more than once"
            ]
        );
        assert_eq!(
            errors("TYPE t = Missing; END_TYPE;"),
            ["line 2, column 10: `Missing` is not a declared type or entity"]
        );
        assert_eq!(
            errors("TYPE e = ENUMERATION OF (A, a); END_TYPE;"),
            ["line 2, column 29: `e.a` is declared more than once"]
        );
        assert_eq!(
            errors("TYPE s = SELECT (t, T); END_TYPE; TYPE t = REAL; END_TYPE;"),
            ["line 2, column 21: `s.T` is declared more than once"]
        );
        assert_eq!(
            errors("TYPE s = SELECT (Missing); END_TYPE;"),
            ["line 2, column 18: `Missing` is not a declared type or entity"]
        );
        assert_eq!(
            errors("TYPE e = ENUMERATION BASED_ON s; END_TYPE; TYPE s = SELECT; END_TYPE;"),
            ["line 2, column 31: `s` is not a declared enumeration"]
        );
        assert_eq!(
            errors("TYPE s = SELECT BASED_ON e; END_TYPE; TYPE e = ENUMERATION OF (A); END_TYPE;"),
            ["line 2, column 26: `e` is not a declared select"]
        );
        assert_eq!(
            errors("TYPE s = SELECT BASED_ON Nope; END_TYPE;"),
            ["line 2, column 26: `Nope` is not a declared select"]
        );
        assert_eq!(
            errors("RULE r FOR (Nope); WHERE TRUE; END_RULE;"),
            ["line 2, column 13: `Nope` is not a declared entity"]
        );
    }

    #[test]
    fn reports_type_problems() {
        assert_eq!(
            errors(
                "TYPE a = ENUMERATION BASED_ON b; END_TYPE; TYPE b = ENUMERATION BASED_ON a; END_TYPE;"
            ),
            [
                "line 2, column 6: `a` is defined in terms of itself",
                "line 2, column 49: `b` is defined in terms of itself"
            ]
        );
        assert_eq!(
            errors("TYPE a = b; END_TYPE; TYPE b = s; END_TYPE; TYPE s = SELECT (a); END_TYPE;"),
            [
                "line 2, column 6: `a` is defined in terms of itself",
                "line 2, column 28: `b` is defined in terms of itself",
                "line 2, column 50: `s` is defined in terms of itself"
            ]
        );
        assert_eq!(
            errors("TYPE l = LIST [2:1] OF REAL; END_TYPE;"),
            ["line 2, column 6: invalid aggregate bounds in `l`"]
        );
        assert_eq!(
            errors("TYPE g = LIST OF GENERIC; END_TYPE;"),
            ["line 2, column 6: generic type in `g`; generic types are only for parameters"]
        );
        // A list breaks the cycle: the type holds its elements behind an allocation.
        let model = model("TYPE a = SELECT (l); END_TYPE; TYPE l = LIST OF a; END_TYPE;");
        assert_eq!(model.types.len(), 2);
    }

    #[test]
    fn reports_entity_problems() {
        assert_eq!(
            errors("ENTITY a SUBTYPE OF (b); END_ENTITY; ENTITY b SUBTYPE OF (a); END_ENTITY;"),
            ["line 2, column 8: `a` is its own supertype"]
        );
        assert_eq!(
            errors("ENTITY a SUBTYPE OF (Nope); END_ENTITY;"),
            ["line 2, column 22: `Nope` is not a declared entity"]
        );
        assert_eq!(
            errors("TYPE t = REAL; END_TYPE; ENTITY a SUBTYPE OF (t); END_ENTITY;"),
            ["line 2, column 47: `t` is not a declared entity"]
        );
        assert_eq!(
            errors(
                "ENTITY a SUPERTYPE OF (ONEOF (b, Nope) ANDOR (c AND b)); END_ENTITY; ENTITY b; END_ENTITY; ENTITY c; END_ENTITY;"
            ),
            ["line 2, column 34: `Nope` is not a declared entity"]
        );
        assert_eq!(
            errors(
                "ENTITY a; INVERSE x : Nope FOR y; z : b FOR Nope2.y; END_ENTITY; ENTITY b; END_ENTITY;"
            ),
            [
                "line 2, column 23: `Nope` is not a declared entity",
                "line 2, column 45: `Nope2` is not a declared entity"
            ]
        );
        assert_eq!(
            errors("ENTITY a; x : REAL; X : REAL; END_ENTITY;"),
            ["line 2, column 21: `a.X` is declared more than once"]
        );
        assert_eq!(
            errors(
                "ENTITY a; x : REAL; END_ENTITY; ENTITY b SUBTYPE OF (a); x : REAL; END_ENTITY;"
            ),
            ["line 2, column 58: `b.x` is declared more than once"]
        );
        assert_eq!(
            errors(
                "ENTITY a; x : REAL; END_ENTITY; ENTITY b SUBTYPE OF (a); SELF\\a.y : REAL; END_ENTITY;"
            ),
            ["line 2, column 65: `b` does not inherit an attribute `a.y` to redeclare"]
        );
        assert_eq!(
            errors(
                "ENTITY a; END_ENTITY; ENTITY b SUBTYPE OF (a); DERIVE SELF\\a.y : REAL := 1; END_ENTITY;"
            ),
            ["line 2, column 62: `b` does not inherit an attribute `a.y` to redeclare"]
        );
        assert_eq!(
            errors("ENTITY a; x : Nope; END_ENTITY;"),
            ["line 2, column 15: `Nope` is not a declared type or entity"]
        );
    }
}
