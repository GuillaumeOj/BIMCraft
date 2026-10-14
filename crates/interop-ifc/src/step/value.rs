/// One parameter of a record, as written in the file.
#[derive(Clone, Debug, PartialEq)]
pub enum Parameter {
    /// `$`: the value is absent.
    Null,
    /// `*`: the value is derived from other attributes.
    Derived,
    /// An integer, such as `42` or `-7`.
    Integer(i64),
    /// A real, such as `1.5` or `-2.E-3`.
    Real(f64),
    /// A string without its quotes, with `''` unescaped and encoding directives kept as written.
    String(String),
    /// An enumeration value without its dots, such as `T` for `.T.`.
    Enumeration(String),
    /// A binary literal without its quotes: hexadecimal digits, the first giving the unused bits.
    Binary(String),
    /// A reference to another instance, such as `#12`.
    Reference(u64),
    /// A value tagged with its type, such as `IFCLENGTHMEASURE(2.5)`.
    Typed {
        /// The type name, such as `IFCLENGTHMEASURE`.
        type_name: String,
        /// The tagged value.
        value: Box<Parameter>,
    },
    /// A list of parameters, such as `(1,2,3)`.
    List(Vec<Parameter>),
}

/// An entity name with its parameters, such as `IFCWALL('2O2Fr$t4X7Zf8NOew3FLOH',#5,$)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// The entity name, such as `IFCWALL`.
    pub name: String,
    /// The parameters in file order.
    pub parameters: Vec<Parameter>,
}

/// An instance of the data section, such as `#7=IFCWALL(...);`.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// The instance name: `7` for `#7`.
    pub id: u64,
    /// The instance content.
    pub body: InstanceBody,
}

/// The content of an [`Instance`].
#[derive(Clone, Debug, PartialEq)]
pub enum InstanceBody {
    /// One record: `#7=IFCWALL(...);`.
    Simple(Record),
    /// A complex instance, one record per entity of the combination, in file order:
    /// `#7=(A(...)B(...));`.
    Complex(Vec<Record>),
}
