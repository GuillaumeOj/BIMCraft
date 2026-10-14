//! The syntax tree of an EXPRESS schema, as written (ISO 10303-11 §9 to §16).
//!
//! Names keep the spelling of the source; EXPRESS compares them without regard to case.

use crate::Position;

/// A name with where it is written.
#[derive(Clone, Debug, PartialEq)]
pub struct Ident {
    /// The name as written.
    pub name: String,
    /// Where it is written.
    pub position: Position,
}

/// `SCHEMA name; … END_SCHEMA;`
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    /// The schema name, such as `IFC4X3_ADD2`.
    pub name: Ident,
    /// The schema version identifier, when one follows the name.
    pub version: Option<String>,
    /// Declarations and rules in source order.
    pub declarations: Vec<Declaration>,
}

/// A declaration in a schema or in an algorithm.
#[derive(Clone, Debug, PartialEq)]
pub enum Declaration {
    /// `TYPE … END_TYPE;`
    Type(TypeDecl),
    /// `ENTITY … END_ENTITY;`
    Entity(Box<EntityDecl>),
    /// `FUNCTION … END_FUNCTION;`
    Function(Box<FunctionDecl>),
    /// `PROCEDURE … END_PROCEDURE;`
    Procedure(Box<ProcedureDecl>),
    /// `RULE … END_RULE;`
    Rule(Box<RuleDecl>),
    /// `CONSTANT … END_CONSTANT;`
    Constant(Vec<ConstantDecl>),
}

/// `TYPE name = underlying; [WHERE …] END_TYPE;`
#[derive(Clone, Debug, PartialEq)]
pub struct TypeDecl {
    /// The type name.
    pub name: Ident,
    /// What the type stands for.
    pub underlying: UnderlyingType,
    /// Domain rules.
    pub where_rules: Vec<WhereRule>,
}

/// The right-hand side of a type declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum UnderlyingType {
    /// A simple, named or aggregate type.
    Concrete(TypeSpec),
    /// `[EXTENSIBLE] ENUMERATION [OF (…) | BASED_ON name [WITH (…)]]`
    Enumeration {
        /// Whether other enumerations may extend it.
        extensible: bool,
        /// The enumeration it extends.
        based_on: Option<Ident>,
        /// Its own items, in order.
        items: Vec<Ident>,
    },
    /// `[EXTENSIBLE [GENERIC_ENTITY]] SELECT [(…) | BASED_ON name [WITH (…)]]`
    Select {
        /// Whether other selects may extend it.
        extensible: bool,
        /// Whether extensions may only add entities.
        generic_entity: bool,
        /// The select it extends.
        based_on: Option<Ident>,
        /// Its own members, in order.
        items: Vec<Ident>,
    },
}

/// A type as used by an attribute, a parameter or a variable.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeSpec {
    /// A built-in simple type.
    Simple(SimpleType),
    /// A type or entity declared by name.
    Named(Ident),
    /// `ARRAY`, `LIST`, `SET` or `BAG`.
    Aggregate(Box<AggregateType>),
    /// `AGGREGATE [: label] OF type`, for parameters only.
    GenericAggregate {
        /// The type label.
        label: Option<Ident>,
        /// The element type.
        element: Box<TypeSpec>,
    },
    /// `GENERIC [: label]`, for parameters only.
    Generic(Option<Ident>),
    /// `GENERIC_ENTITY [: label]`, for parameters only.
    GenericEntity(Option<Ident>),
}

/// The built-in simple types.
#[derive(Clone, Debug, PartialEq)]
pub enum SimpleType {
    /// `BINARY [(width) [FIXED]]`
    Binary(Option<Width>),
    /// `BOOLEAN`
    Boolean,
    /// `INTEGER`
    Integer,
    /// `LOGICAL`
    Logical,
    /// `NUMBER`
    Number,
    /// `REAL [(precision)]`
    Real(Option<Expr>),
    /// `STRING [(width) [FIXED]]`
    String(Option<Width>),
}

/// The width of a string or binary type.
#[derive(Clone, Debug, PartialEq)]
pub struct Width {
    /// The maximum, or with `fixed` exact, width.
    pub width: Expr,
    /// Whether the width is exact.
    pub fixed: bool,
}

/// The kinds of aggregate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AggregateKind {
    /// `ARRAY`: fixed size, indexed by its bounds.
    Array,
    /// `BAG`: unordered, duplicates allowed.
    Bag,
    /// `LIST`: ordered.
    List,
    /// `SET`: unordered, no duplicates.
    Set,
}

/// `ARRAY [l:h] OF [OPTIONAL] [UNIQUE] type`, `LIST [l:h] OF [UNIQUE] type`, `SET [l:h] OF type`
/// or `BAG [l:h] OF type`.
#[derive(Clone, Debug, PartialEq)]
pub struct AggregateType {
    /// Which aggregate.
    pub kind: AggregateKind,
    /// The bounds; required for `ARRAY` in declarations, optional elsewhere.
    pub bounds: Option<Bounds>,
    /// `OPTIONAL`: an array whose elements may be absent.
    pub optional: bool,
    /// `UNIQUE`: no two elements are the same instance.
    pub unique: bool,
    /// The element type.
    pub element: TypeSpec,
}

/// `[lower : upper]`; `?` as upper bound means unbounded.
#[derive(Clone, Debug, PartialEq)]
pub struct Bounds {
    /// The lower bound.
    pub lower: Expr,
    /// The upper bound.
    pub upper: Expr,
}

/// `ENTITY name …; attributes … END_ENTITY;`
#[derive(Clone, Debug, PartialEq)]
pub struct EntityDecl {
    /// The entity name.
    pub name: Ident,
    /// `ABSTRACT` or `ABSTRACT SUPERTYPE`: never instantiated on its own.
    pub is_abstract: bool,
    /// `SUPERTYPE OF (…)`: how its subtypes combine.
    pub supertype_of: Option<SupertypeExpr>,
    /// `SUBTYPE OF (…)`: its direct supertypes.
    pub subtype_of: Vec<Ident>,
    /// Explicit attributes, in order.
    pub explicit: Vec<ExplicitAttr>,
    /// `DERIVE` attributes, in order.
    pub derived: Vec<DerivedAttr>,
    /// `INVERSE` attributes, in order.
    pub inverse: Vec<InverseAttr>,
    /// `UNIQUE` rules.
    pub unique: Vec<UniqueRule>,
    /// `WHERE` rules.
    pub where_rules: Vec<WhereRule>,
}

/// A supertype expression: how subtypes of an entity combine.
#[derive(Clone, Debug, PartialEq)]
pub enum SupertypeExpr {
    /// A subtype.
    Entity(Ident),
    /// `ONEOF (…)`: exactly one of them.
    OneOf(Vec<SupertypeExpr>),
    /// `a AND b`: all of them.
    And(Vec<SupertypeExpr>),
    /// `a ANDOR b`: any combination of them.
    AndOr(Vec<SupertypeExpr>),
}

/// The name of an attribute in its declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum AttrName {
    /// A new attribute.
    New(Ident),
    /// `SELF\entity.attribute [RENAMED name]`: an inherited attribute declared again.
    Redeclared {
        /// The supertype that declares the attribute.
        entity: Ident,
        /// The inherited attribute.
        attribute: Ident,
        /// A new name for it in this entity.
        renamed: Option<Ident>,
    },
}

/// `names : [OPTIONAL] type;`
#[derive(Clone, Debug, PartialEq)]
pub struct ExplicitAttr {
    /// The attributes declared, in order.
    pub names: Vec<AttrName>,
    /// Whether a value may be absent.
    pub optional: bool,
    /// The value type.
    pub ty: TypeSpec,
}

/// `name : type := expression;` in a `DERIVE` clause.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedAttr {
    /// The attribute.
    pub name: AttrName,
    /// The value type.
    pub ty: TypeSpec,
    /// How the value is computed.
    pub value: Expr,
}

/// `name : [SET|BAG [bounds] OF] entity FOR [entity.]attribute;` in an `INVERSE` clause.
#[derive(Clone, Debug, PartialEq)]
pub struct InverseAttr {
    /// The attribute.
    pub name: AttrName,
    /// `SET` or `BAG` with its bounds, when the inverse is an aggregate.
    pub aggregate: Option<(AggregateKind, Option<Bounds>)>,
    /// The entity whose attribute refers here.
    pub entity: Ident,
    /// The entity that declares that attribute, when qualified.
    pub for_entity: Option<Ident>,
    /// The attribute that refers here.
    pub attribute: Ident,
}

/// `[label :] attribute, …;` in a `UNIQUE` clause.
#[derive(Clone, Debug, PartialEq)]
pub struct UniqueRule {
    /// The rule label.
    pub label: Option<Ident>,
    /// The attributes whose values, together, are unique.
    pub attributes: Vec<AttrName>,
}

/// `[label :] expression;` in a `WHERE` clause.
#[derive(Clone, Debug, PartialEq)]
pub struct WhereRule {
    /// The rule label.
    pub label: Option<Ident>,
    /// The condition.
    pub condition: Expr,
}

/// A formal parameter of a function or procedure.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    /// The parameter names.
    pub names: Vec<Ident>,
    /// `VAR`: a procedure parameter passed by reference.
    pub var: bool,
    /// The parameter type.
    pub ty: TypeSpec,
}

/// Local declarations and statements of a function, procedure or rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Algorithm {
    /// Nested declarations: types, entities, functions, procedures and constants.
    pub declarations: Vec<Declaration>,
    /// `LOCAL` variables.
    pub locals: Vec<LocalDecl>,
    /// The statements.
    pub body: Vec<Stmt>,
}

/// `FUNCTION name [(parameters)] : type; algorithm END_FUNCTION;`
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDecl {
    /// The function name.
    pub name: Ident,
    /// Its parameters.
    pub parameters: Vec<Parameter>,
    /// The result type.
    pub result: TypeSpec,
    /// Its body.
    pub algorithm: Algorithm,
}

/// `PROCEDURE name [(parameters)]; algorithm END_PROCEDURE;`
#[derive(Clone, Debug, PartialEq)]
pub struct ProcedureDecl {
    /// The procedure name.
    pub name: Ident,
    /// Its parameters.
    pub parameters: Vec<Parameter>,
    /// Its body.
    pub algorithm: Algorithm,
}

/// `RULE name FOR (entities); algorithm WHERE … END_RULE;`
#[derive(Clone, Debug, PartialEq)]
pub struct RuleDecl {
    /// The rule name.
    pub name: Ident,
    /// The entities whose populations it constrains.
    pub entities: Vec<Ident>,
    /// Its body.
    pub algorithm: Algorithm,
    /// Its conditions.
    pub where_rules: Vec<WhereRule>,
}

/// `name : type := expression;` in a `CONSTANT` block.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstantDecl {
    /// The constant name.
    pub name: Ident,
    /// Its type.
    pub ty: TypeSpec,
    /// Its value.
    pub value: Expr,
}

/// `names : type [:= expression];` in a `LOCAL` block.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalDecl {
    /// The variable names.
    pub names: Vec<Ident>,
    /// Their type.
    pub ty: TypeSpec,
    /// Their initial value.
    pub initial: Option<Expr>,
}

/// A statement (ISO 10303-11 §13).
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// `;`
    Null,
    /// `ALIAS name FOR reference; statements END_ALIAS;`
    Alias {
        /// The alias.
        name: Ident,
        /// What it stands for.
        target: Expr,
        /// The statements where it applies.
        body: Vec<Stmt>,
    },
    /// `reference := expression;`
    Assign {
        /// What is assigned.
        target: Expr,
        /// The new value.
        value: Expr,
    },
    /// `CASE selector OF label : statement … [OTHERWISE : statement] END_CASE;`
    Case {
        /// The value compared with the labels.
        selector: Expr,
        /// The labelled statements, in order.
        branches: Vec<(Vec<Expr>, Stmt)>,
        /// The statement when no label matches.
        otherwise: Option<Box<Stmt>>,
    },
    /// `BEGIN statements END;`
    Compound(Vec<Stmt>),
    /// `ESCAPE;`
    Escape,
    /// `IF condition THEN statements [ELSE statements] END_IF;`
    If {
        /// The condition.
        condition: Expr,
        /// Statements when it is true.
        then: Vec<Stmt>,
        /// Statements otherwise.
        otherwise: Vec<Stmt>,
    },
    /// `name [(arguments)];`: a procedure call.
    Call {
        /// The procedure.
        name: Ident,
        /// The arguments.
        arguments: Vec<Expr>,
    },
    /// `REPEAT [i := a TO b [BY c]] [WHILE x] [UNTIL y]; statements END_REPEAT;`
    Repeat {
        /// The loop variable and its range.
        increment: Option<Box<Increment>>,
        /// Loop while this is true.
        while_condition: Option<Expr>,
        /// Loop until this is true.
        until_condition: Option<Expr>,
        /// The loop body.
        body: Vec<Stmt>,
    },
    /// `RETURN [(expression)];`
    Return(Option<Expr>),
    /// `SKIP;`
    Skip,
}

/// `variable := start TO end [BY step]` in a `REPEAT` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct Increment {
    /// The loop variable.
    pub variable: Ident,
    /// Its first value.
    pub start: Expr,
    /// Its last value.
    pub end: Expr,
    /// The step, 1 when absent.
    pub step: Option<Expr>,
}

/// An expression (ISO 10303-11 §12).
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// An integer literal.
    Integer(i64),
    /// A real literal.
    Real(f64),
    /// A string literal.
    String(String),
    /// A binary literal: its digits.
    BinaryLiteral(String),
    /// `TRUE`, `FALSE` or `UNKNOWN`.
    Logical(Option<bool>),
    /// `?`: the indeterminate value.
    Indeterminate,
    /// `SELF`.
    SelfRef,
    /// `PI`.
    Pi,
    /// `CONST_E`.
    ConstE,
    /// A variable, attribute, constant, enumeration item or type name.
    Name(Ident),
    /// `+x`, `-x` or `NOT x`.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: Box<Expr>,
    },
    /// `x op y`.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// The left operand.
        left: Box<Expr>,
        /// The right operand.
        right: Box<Expr>,
    },
    /// `{low op item op high}`, with `<` or `<=` as operators.
    Interval {
        /// The lower bound.
        low: Box<Expr>,
        /// Whether `low` may equal `item`.
        low_inclusive: bool,
        /// The value tested.
        item: Box<Expr>,
        /// Whether `item` may equal `high`.
        high_inclusive: bool,
        /// The upper bound.
        high: Box<Expr>,
    },
    /// `name(arguments)`: a function call or an entity constructor.
    Call {
        /// The function or entity.
        name: Ident,
        /// The arguments.
        arguments: Vec<Expr>,
    },
    /// `base.attribute`: an attribute or an enumeration item.
    Attribute {
        /// What the attribute is read from.
        base: Box<Expr>,
        /// The attribute.
        name: Ident,
    },
    /// `base\entity`: the partial value of a supertype.
    Group {
        /// The instance.
        base: Box<Expr>,
        /// The supertype.
        entity: Ident,
    },
    /// `base[index]` or `base[low:high]`.
    Index {
        /// What is indexed.
        base: Box<Expr>,
        /// The index, or the first index of a range.
        index: Box<Expr>,
        /// The last index of a range.
        high: Option<Box<Expr>>,
    },
    /// `[element, value : repetitions, …]`.
    Aggregate(Vec<(Expr, Option<Expr>)>),
    /// `QUERY(variable <* source | condition)`.
    Query {
        /// The variable bound to each element.
        variable: Ident,
        /// The aggregate queried.
        source: Box<Expr>,
        /// The condition elements keep.
        condition: Box<Expr>,
    },
}

/// Unary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `NOT`
    Not,
}

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `=`
    Equal,
    /// `<>`
    NotEqual,
    /// `<`
    Less,
    /// `>`
    Greater,
    /// `<=`
    LessEqual,
    /// `>=`
    GreaterEqual,
    /// `:=:`
    InstanceEqual,
    /// `:<>:`
    InstanceNotEqual,
    /// `IN`
    In,
    /// `LIKE`
    Like,
    /// `+`
    Add,
    /// `-`
    Subtract,
    /// `OR`
    Or,
    /// `XOR`
    Xor,
    /// `*`
    Multiply,
    /// `/`
    Divide,
    /// `DIV`
    IntegerDivide,
    /// `MOD`
    Modulo,
    /// `AND`
    And,
    /// `||`: complex entity instance construction.
    Concat,
    /// `**`
    Power,
}
