use std::fmt;
use std::str::FromStr;

/// How many `list:` prefixes a value type may nest. Deeper types are never needed by a building
/// model and only make hostile input expensive.
pub const MAX_LIST_DEPTH: usize = 4;

/// The type of an attribute or property value, written `bool`, `quantity:length`,
/// `list:ref:Layer`… in definition files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueType {
    /// `true` or `false`.
    Bool,
    /// A signed 64-bit integer.
    Integer,
    /// A real number without a unit.
    Real,
    /// Free text.
    Text,
    /// One value of a named enumeration.
    Enum(String),
    /// A physical quantity, stored in the SI unit of its kind (CORE-09).
    Quantity(QuantityKind),
    /// A reference to another object of the named entity.
    Ref(String),
    /// An ordered list of values of the inner type.
    List(Box<ValueType>),
}

/// The kinds of physical quantity a value can hold, each stored in one SI unit (CORE-09).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuantityKind {
    /// Length, in metres.
    Length,
    /// Area, in square metres.
    Area,
    /// Volume, in cubic metres.
    Volume,
    /// Plane angle, in radians.
    PlaneAngle,
    /// Mass, in kilograms.
    Mass,
    /// Mass density, in kilograms per cubic metre.
    MassDensity,
    /// Force, in newtons.
    Force,
    /// Pressure or stress, in pascals.
    Pressure,
    /// Thermodynamic temperature, in kelvins.
    Temperature,
    /// Thermal transmittance (U-value), in watts per square metre kelvin.
    ThermalTransmittance,
    /// Thermal conductivity, in watts per metre kelvin.
    ThermalConductivity,
}

impl QuantityKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 11] = [
        Self::Length,
        Self::Area,
        Self::Volume,
        Self::PlaneAngle,
        Self::Mass,
        Self::MassDensity,
        Self::Force,
        Self::Pressure,
        Self::Temperature,
        Self::ThermalTransmittance,
        Self::ThermalConductivity,
    ];

    /// The name used after `quantity:` in definition files.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Length => "length",
            Self::Area => "area",
            Self::Volume => "volume",
            Self::PlaneAngle => "plane_angle",
            Self::Mass => "mass",
            Self::MassDensity => "mass_density",
            Self::Force => "force",
            Self::Pressure => "pressure",
            Self::Temperature => "temperature",
            Self::ThermalTransmittance => "thermal_transmittance",
            Self::ThermalConductivity => "thermal_conductivity",
        }
    }

    /// The SI unit values of this kind are stored in, as a symbol.
    #[must_use]
    pub const fn si_unit(self) -> &'static str {
        match self {
            Self::Length => "m",
            Self::Area => "m²",
            Self::Volume => "m³",
            Self::PlaneAngle => "rad",
            Self::Mass => "kg",
            Self::MassDensity => "kg/m³",
            Self::Force => "N",
            Self::Pressure => "Pa",
            Self::Temperature => "K",
            Self::ThermalTransmittance => "W/(m²·K)",
            Self::ThermalConductivity => "W/(m·K)",
        }
    }
}

/// Why a value type string could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValueTypeError {
    /// The string names no known value type.
    #[error("unknown value type \"{0}\"")]
    Unknown(String),
    /// `quantity:` is followed by an unknown kind.
    #[error("unknown quantity kind \"{0}\"")]
    UnknownQuantity(String),
    /// `enum:` or `ref:` is followed by nothing.
    #[error("\"{0}\" needs a name after the colon")]
    MissingName(String),
    /// More than [`MAX_LIST_DEPTH`] nested `list:` prefixes.
    #[error("lists nest more than {MAX_LIST_DEPTH} levels deep")]
    TooDeep,
}

impl FromStr for ValueType {
    type Err = ValueTypeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut depth = 0;
        let mut rest = text;
        while let Some(inner) = rest.strip_prefix("list:") {
            depth += 1;
            if depth > MAX_LIST_DEPTH {
                return Err(ValueTypeError::TooDeep);
            }
            rest = inner;
        }
        let mut value = scalar(rest)?;
        for _ in 0..depth {
            value = Self::List(Box::new(value));
        }
        Ok(value)
    }
}

fn scalar(text: &str) -> Result<ValueType, ValueTypeError> {
    let named = |name: &str, build: fn(String) -> ValueType| {
        if name.is_empty() {
            Err(ValueTypeError::MissingName(text.to_owned()))
        } else {
            Ok(build(name.to_owned()))
        }
    };
    match text {
        "bool" => Ok(ValueType::Bool),
        "integer" => Ok(ValueType::Integer),
        "real" => Ok(ValueType::Real),
        "text" => Ok(ValueType::Text),
        _ => {
            if let Some(kind) = text.strip_prefix("quantity:") {
                QuantityKind::ALL
                    .into_iter()
                    .find(|candidate| candidate.name() == kind)
                    .map(ValueType::Quantity)
                    .ok_or_else(|| ValueTypeError::UnknownQuantity(kind.to_owned()))
            } else if let Some(name) = text.strip_prefix("enum:") {
                named(name, ValueType::Enum)
            } else if let Some(name) = text.strip_prefix("ref:") {
                named(name, ValueType::Ref)
            } else {
                Err(ValueTypeError::Unknown(text.to_owned()))
            }
        }
    }
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => f.write_str("bool"),
            Self::Integer => f.write_str("integer"),
            Self::Real => f.write_str("real"),
            Self::Text => f.write_str("text"),
            Self::Enum(name) => write!(f, "enum:{name}"),
            Self::Quantity(kind) => write!(f, "quantity:{}", kind.name()),
            Self::Ref(name) => write!(f, "ref:{name}"),
            Self::List(inner) => write!(f, "list:{inner}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn reads_every_scalar_type() {
        assert_eq!("bool".parse(), Ok(ValueType::Bool));
        assert_eq!("integer".parse(), Ok(ValueType::Integer));
        assert_eq!("real".parse(), Ok(ValueType::Real));
        assert_eq!("text".parse(), Ok(ValueType::Text));
        assert_eq!("enum:Side".parse(), Ok(ValueType::Enum("Side".into())));
        assert_eq!("ref:Wall".parse(), Ok(ValueType::Ref("Wall".into())));
        for kind in QuantityKind::ALL {
            assert_eq!(
                format!("quantity:{}", kind.name()).parse(),
                Ok(ValueType::Quantity(kind))
            );
        }
    }

    #[test]
    fn reads_nested_lists() {
        assert_eq!(
            "list:list:ref:Layer".parse(),
            Ok(ValueType::List(Box::new(ValueType::List(Box::new(
                ValueType::Ref("Layer".into())
            )))))
        );
    }

    #[test]
    fn rejects_unknown_and_incomplete_types() {
        assert_eq!(
            "float".parse::<ValueType>(),
            Err(ValueTypeError::Unknown("float".into()))
        );
        assert_eq!(
            "list:".parse::<ValueType>(),
            Err(ValueTypeError::Unknown(String::new()))
        );
        assert_eq!(
            "quantity:speed".parse::<ValueType>(),
            Err(ValueTypeError::UnknownQuantity("speed".into()))
        );
        assert_eq!(
            "ref:".parse::<ValueType>(),
            Err(ValueTypeError::MissingName("ref:".into()))
        );
        assert_eq!(
            "enum:".parse::<ValueType>(),
            Err(ValueTypeError::MissingName("enum:".into()))
        );
    }

    #[test]
    fn limits_list_nesting() {
        let deepest = format!("{}bool", "list:".repeat(MAX_LIST_DEPTH));
        assert!(deepest.parse::<ValueType>().is_ok());
        let too_deep = format!("list:{deepest}");
        assert_eq!(too_deep.parse::<ValueType>(), Err(ValueTypeError::TooDeep));
    }

    #[test]
    fn every_quantity_kind_has_a_unit() {
        for kind in QuantityKind::ALL {
            assert!(!kind.si_unit().is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn errors_explain_themselves() {
        assert_eq!(
            ValueTypeError::TooDeep.to_string(),
            "lists nest more than 4 levels deep"
        );
        assert_eq!(
            ValueTypeError::MissingName("ref:".into()).to_string(),
            "\"ref:\" needs a name after the colon"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            rng_seed: prop::test_runner::RngSeed::Fixed(0x5C4E_3A01),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        #[test]
        fn display_round_trips(text in r"(list:){0,4}(bool|integer|real|text|enum:[A-Z][a-z]{0,8}|ref:[A-Z][a-z]{0,8}|quantity:(length|area|plane_angle))") {
            let parsed: ValueType = text.parse().unwrap();
            prop_assert_eq!(parsed.to_string(), text);
        }

        #[test]
        fn never_panics(text in ".{0,64}") {
            let _ = text.parse::<ValueType>();
        }
    }
}
