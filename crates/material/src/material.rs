//! A material: a named grade with a behaviour law and sourced properties (MAT-01).

use std::collections::BTreeMap;

use core_model::units::Quantity;
use thiserror::Error;

use crate::law::{
    BehaviourLaw, ConcreteParabolaRectangle, Hardening, LawError, LawKind, LinearElastic,
    SteelBilinear,
};
use crate::property::Property;
use crate::source::{MAX_TEXT_LEN, Sourced, bounded_text};

/// What a material is, as the `category` attribute stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// Concrete.
    Concrete,
    /// Steel bars for reinforced concrete.
    ReinforcingSteel,
    /// Steel for structural members.
    StructuralSteel,
    /// Any other material.
    Other,
}

impl Category {
    /// Every category.
    pub const ALL: [Self; 4] = [
        Self::Concrete,
        Self::ReinforcingSteel,
        Self::StructuralSteel,
        Self::Other,
    ];

    /// The name stored in the model.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Concrete => "concrete",
            Self::ReinforcingSteel => "reinforcing_steel",
            Self::StructuralSteel => "structural_steel",
            Self::Other => "other",
        }
    }

    /// The category stored under `name`, if any.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.name() == name)
    }

    /// Whether materials of this category may follow a law of kind `law`.
    #[must_use]
    pub const fn allows(self, law: LawKind) -> bool {
        matches!(
            (self, law),
            (_, LawKind::LinearElastic)
                | (Self::Concrete, LawKind::ConcreteParabolaRectangle)
                | (
                    Self::ReinforcingSteel | Self::StructuralSteel,
                    LawKind::SteelBilinear
                )
        )
    }
}

/// Why a material was rejected. Messages name properties, never values (SEC-03).
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum MaterialError {
    /// The name is empty.
    #[error("the material has no name")]
    EmptyName,
    /// The name or design code is longer than [`MAX_TEXT_LEN`] characters.
    #[error("the material's {0} is longer than {MAX_TEXT_LEN} characters")]
    TooLong(&'static str),
    /// The law does not suit the category, such as a concrete law for steel.
    #[error("a {} material cannot follow a {} law", .category.name(), .law.name())]
    LawNotForCategory {
        /// The material's category.
        category: Category,
        /// The rejected law.
        law: LawKind,
    },
    /// A property is below another that may not exceed it, such as an ultimate strength below
    /// the yield strength.
    #[error("the property {} is below the property {}", .property.name(), .below.name())]
    Inconsistent {
        /// The property that is too low.
        property: Property,
        /// The property it may not be below.
        below: Property,
    },
    /// The law needs a property the material does not give.
    #[error("the behaviour law needs the property {}", .0.name())]
    MissingProperty(Property),
    /// A property value is outside its physical range.
    #[error("the property {} is out of range", .0.name())]
    OutOfRange(Property),
    /// The law's parameters are inconsistent.
    #[error(transparent)]
    Law(#[from] LawError),
}

/// A material grade, valid by construction.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    name: String,
    category: Category,
    law: BehaviourLaw,
    design_code: Option<String>,
    properties: BTreeMap<Property, Sourced<f64>>,
}

impl Material {
    /// A material named `name` following a law of kind `law`, whose parameters come from
    /// `properties`; `design_code` names the code the values follow.
    ///
    /// The laws read these properties: linear elastic, [`Property::YoungModulus`]; concrete,
    /// the characteristic compressive strength, the peak and ultimate strains and the parabola
    /// exponent; steel, the yield strength and Young's modulus, plus the hardening ratio and
    /// ultimate strain, which make it harden instead of staying perfectly plastic.
    ///
    /// # Errors
    ///
    /// Fails when the name is empty, a text is too long, the law does not suit the category, a
    /// value is out of range, or the law misses a property or gets inconsistent ones.
    pub fn new(
        name: &str,
        category: Category,
        law: LawKind,
        design_code: Option<&str>,
        properties: BTreeMap<Property, Sourced<f64>>,
    ) -> Result<Self, MaterialError> {
        let too_long = |field| move |_| MaterialError::TooLong(field);
        let name = bounded_text(name)
            .map_err(too_long("name"))?
            .ok_or(MaterialError::EmptyName)?;
        let design_code = design_code
            .map(bounded_text)
            .transpose()
            .map_err(too_long("design code"))?
            .flatten();
        if !category.allows(law) {
            return Err(MaterialError::LawNotForCategory { category, law });
        }
        if let Some((property, _)) = properties
            .iter()
            .find(|(property, value)| !property.accepts(value.value))
        {
            return Err(MaterialError::OutOfRange(*property));
        }
        check_order(&properties)?;
        let law = build_law(law, &properties)?;
        Ok(Self {
            name: name.to_owned(),
            category,
            law,
            design_code: design_code.map(str::to_owned),
            properties,
        })
    }

    /// The material's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The material's category.
    #[must_use]
    pub const fn category(&self) -> Category {
        self.category
    }

    /// The material's behaviour law, with characteristic strengths.
    #[must_use]
    pub const fn law(&self) -> &BehaviourLaw {
        &self.law
    }

    /// The design code the values follow, if given.
    #[must_use]
    pub fn design_code(&self) -> Option<&str> {
        self.design_code.as_deref()
    }

    /// The value of `property` with its source, if the material gives it.
    #[must_use]
    pub fn property(&self, property: Property) -> Option<&Sourced<f64>> {
        self.properties.get(&property)
    }

    /// Every property the material gives, in [`Property`] order.
    pub fn properties(&self) -> impl Iterator<Item = (Property, &Sourced<f64>)> {
        self.properties
            .iter()
            .map(|(property, value)| (*property, value))
    }
}

/// Pairs of properties where the second may never be below the first.
const ORDERED: [(Property, Property); 2] = [
    (
        Property::CharacteristicCompressiveStrength,
        Property::MeanCompressiveStrength,
    ),
    (Property::YieldStrength, Property::UltimateStrength),
];

fn check_order(properties: &BTreeMap<Property, Sourced<f64>>) -> Result<(), MaterialError> {
    for (lower, upper) in ORDERED {
        if let (Some(low), Some(high)) = (properties.get(&lower), properties.get(&upper))
            && high.value < low.value
        {
            return Err(MaterialError::Inconsistent {
                property: upper,
                below: lower,
            });
        }
    }
    Ok(())
}

fn build_law(
    kind: LawKind,
    properties: &BTreeMap<Property, Sourced<f64>>,
) -> Result<BehaviourLaw, MaterialError> {
    let value = |property| {
        properties
            .get(&property)
            .map(|sourced| sourced.value)
            .ok_or(MaterialError::MissingProperty(property))
    };
    Ok(match kind {
        LawKind::LinearElastic => BehaviourLaw::LinearElastic(LinearElastic::new(
            Quantity::from_si(value(Property::YoungModulus)?),
        )?),
        LawKind::ConcreteParabolaRectangle => {
            BehaviourLaw::ConcreteParabolaRectangle(ConcreteParabolaRectangle::new(
                Quantity::from_si(value(Property::CharacteristicCompressiveStrength)?),
                value(Property::StrainAtPeakStress)?,
                value(Property::UltimateCompressiveStrain)?,
                value(Property::ParabolaExponent)?,
            )?)
        }
        LawKind::SteelBilinear => {
            let hardening = if properties.contains_key(&Property::HardeningRatio)
                || properties.contains_key(&Property::UltimateStrain)
            {
                Some(Hardening {
                    ratio: value(Property::HardeningRatio)?,
                    ultimate_strain: value(Property::UltimateStrain)?,
                })
            } else {
                None
            };
            BehaviourLaw::SteelBilinear(SteelBilinear::new(
                Quantity::from_si(value(Property::YieldStrength)?),
                Quantity::from_si(value(Property::YoungModulus)?),
                hardening,
            )?)
        }
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::source::Source;

    pub(crate) fn sourced(value: f64, clause: &str) -> Sourced<f64> {
        Sourced::new(value, Source::new("EN 1992-1-1:2004", clause).unwrap())
    }

    /// C30/37 to EN 1992-1-1:2004, Table 3.1.
    pub(crate) fn c30_37() -> Material {
        let properties = BTreeMap::from([
            (
                Property::CharacteristicCompressiveStrength,
                sourced(30e6, "Table 3.1"),
            ),
            (
                Property::MeanCompressiveStrength,
                sourced(38e6, "Table 3.1"),
            ),
            (Property::YoungModulus, sourced(33e9, "Table 3.1")),
            (Property::PoissonRatio, sourced(0.2, "3.1.3(4)")),
            (Property::StrainAtPeakStress, sourced(0.002, "Table 3.1")),
            (
                Property::UltimateCompressiveStrain,
                sourced(0.0035, "Table 3.1"),
            ),
            (Property::ParabolaExponent, sourced(2.0, "Table 3.1")),
        ]);
        Material::new(
            "C30/37",
            Category::Concrete,
            LawKind::ConcreteParabolaRectangle,
            Some("EN 1992-1-1:2004"),
            properties,
        )
        .unwrap()
    }

    fn steel(properties: &[(Property, f64)]) -> Result<Material, MaterialError> {
        let properties = properties
            .iter()
            .map(|(property, value)| (*property, sourced(*value, "3.2.7")))
            .collect();
        Material::new(
            "B500B",
            Category::ReinforcingSteel,
            LawKind::SteelBilinear,
            None,
            properties,
        )
    }

    #[test]
    fn categories_round_trip_through_their_names() {
        for category in Category::ALL {
            assert_eq!(Category::from_name(category.name()), Some(category));
        }
        assert_eq!(Category::from_name("timber"), None);
    }

    #[test]
    fn categories_allow_their_own_laws_and_linear_elasticity() {
        use Category::{Concrete, Other, ReinforcingSteel, StructuralSteel};
        use LawKind::{ConcreteParabolaRectangle, LinearElastic, SteelBilinear};
        for category in Category::ALL {
            assert!(category.allows(LinearElastic));
        }
        assert!(Concrete.allows(ConcreteParabolaRectangle));
        assert!(!Concrete.allows(SteelBilinear));
        assert!(ReinforcingSteel.allows(SteelBilinear));
        assert!(StructuralSteel.allows(SteelBilinear));
        assert!(!StructuralSteel.allows(ConcreteParabolaRectangle));
        assert!(!Other.allows(SteelBilinear));
    }

    #[test]
    fn a_material_builds_its_law_from_its_properties() {
        let concrete = c30_37();
        assert_eq!(concrete.name(), "C30/37");
        assert_eq!(concrete.category(), Category::Concrete);
        assert_eq!(concrete.design_code(), Some("EN 1992-1-1:2004"));
        assert_eq!(concrete.law().kind(), LawKind::ConcreteParabolaRectangle);
        assert_eq!(concrete.law().stress(-0.002), Ok(Quantity::from_si(-30e6)));
        let modulus = concrete.property(Property::YoungModulus).unwrap();
        assert_eq!(modulus.source.clause(), "Table 3.1");
        assert_eq!(concrete.properties().count(), 7);
        assert_eq!(concrete.property(Property::YieldStrength), None);
    }

    #[test]
    fn steel_hardens_only_when_both_hardening_properties_are_given() {
        let base = [
            (Property::YieldStrength, 500e6),
            (Property::YoungModulus, 200e9),
        ];
        let plastic = steel(&base).unwrap();
        assert_eq!(plastic.law().stress(0.5), Ok(Quantity::from_si(500e6)));
        let hardening = [
            base[0],
            base[1],
            (Property::HardeningRatio, 1.08),
            (Property::UltimateStrain, 0.05),
        ];
        let hardening = steel(&hardening).unwrap();
        assert!((hardening.law().stress(0.05).unwrap().si() - 540e6).abs() < 1e-3);
        assert_eq!(
            steel(&[base[0], base[1], (Property::HardeningRatio, 1.08)]),
            Err(MaterialError::MissingProperty(Property::UltimateStrain))
        );
        assert_eq!(
            steel(&[base[0], base[1], (Property::UltimateStrain, 0.05)]),
            Err(MaterialError::MissingProperty(Property::HardeningRatio))
        );
    }

    #[test]
    fn a_linear_elastic_material_needs_only_a_modulus() {
        let properties = BTreeMap::from([(Property::YoungModulus, sourced(11e9, "Table 1"))]);
        let timber = Material::new(
            "GL24h",
            Category::Other,
            LawKind::LinearElastic,
            Some(" "),
            properties,
        )
        .unwrap();
        assert_eq!(timber.design_code(), None);
        assert_eq!(timber.law().stress(1e-3), Ok(Quantity::from_si(11e6)));
    }

    #[test]
    fn invalid_materials_are_rejected_with_the_reason() {
        let elastic = |name: &str, code: Option<&str>, value: f64| {
            let properties = BTreeMap::from([(Property::YoungModulus, sourced(value, "1"))]);
            Material::new(
                name,
                Category::Other,
                LawKind::LinearElastic,
                code,
                properties,
            )
        };
        let long = "x".repeat(MAX_TEXT_LEN + 1);
        assert_eq!(elastic(" ", None, 1.0), Err(MaterialError::EmptyName));
        assert_eq!(
            elastic(&long, None, 1.0),
            Err(MaterialError::TooLong("name"))
        );
        assert_eq!(
            elastic("A", Some(&long), 1.0),
            Err(MaterialError::TooLong("design code"))
        );
        assert_eq!(
            steel(&[
                (Property::YieldStrength, 500e6),
                (Property::UltimateStrength, 400e6),
                (Property::YoungModulus, 200e9),
            ]),
            Err(MaterialError::Inconsistent {
                property: Property::UltimateStrength,
                below: Property::YieldStrength,
            })
        );
        assert_eq!(
            elastic("A", None, -1.0),
            Err(MaterialError::OutOfRange(Property::YoungModulus))
        );
        assert_eq!(
            Material::new(
                "A",
                Category::Other,
                LawKind::LinearElastic,
                None,
                BTreeMap::new()
            ),
            Err(MaterialError::MissingProperty(Property::YoungModulus))
        );
        assert_eq!(
            Material::new(
                "A",
                Category::Concrete,
                LawKind::SteelBilinear,
                None,
                BTreeMap::new()
            ),
            Err(MaterialError::LawNotForCategory {
                category: Category::Concrete,
                law: LawKind::SteelBilinear
            })
        );
        assert_eq!(
            steel(&[
                (Property::YieldStrength, 500e6),
                (Property::YoungModulus, 200e9),
                (Property::HardeningRatio, 1.08),
                (Property::UltimateStrain, 0.001)
            ]),
            Err(MaterialError::Law(LawError::InvalidParameter(
                "ultimate_strain"
            )))
        );
    }

    #[test]
    fn error_messages_name_properties_not_values() {
        let error = MaterialError::LawNotForCategory {
            category: Category::Concrete,
            law: LawKind::SteelBilinear,
        };
        assert_eq!(
            error.to_string(),
            "a concrete material cannot follow a steel_bilinear law"
        );
        assert_eq!(
            MaterialError::OutOfRange(Property::PoissonRatio).to_string(),
            "the property PoissonRatio is out of range"
        );
        assert_eq!(
            MaterialError::MissingProperty(Property::YoungModulus).to_string(),
            "the behaviour law needs the property YoungModulus"
        );
        assert_eq!(
            MaterialError::EmptyName.to_string(),
            "the material has no name"
        );
        let inconsistent = MaterialError::Inconsistent {
            property: Property::MeanCompressiveStrength,
            below: Property::CharacteristicCompressiveStrength,
        };
        assert_eq!(
            inconsistent.to_string(),
            "the property MeanCompressiveStrength is below the property \
             CharacteristicCompressiveStrength"
        );
        assert_eq!(
            MaterialError::TooLong("name").to_string(),
            "the material's name is longer than 200 characters"
        );
    }
}
