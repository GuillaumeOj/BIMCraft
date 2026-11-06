//! The material properties the core schema defines, and the property sets that hold them.

/// The property set for mechanical and physical properties.
pub const MECHANICAL_SET: &str = "BIMCraft_MaterialMechanical";
/// The property set for concrete strengths and strains.
pub const CONCRETE_SET: &str = "BIMCraft_MaterialConcrete";
/// The property set for steel strengths and ductility.
pub const STEEL_SET: &str = "BIMCraft_MaterialSteel";

/// A material property (MAT-01). Values are in SI units: stresses in pascals, densities in
/// kilograms per cubic metre, strains and ratios without unit, expansion per kelvin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Property {
    /// Mass per unit volume.
    MassDensity,
    /// Modulus of elasticity; for concrete, the secant modulus.
    YoungModulus,
    /// Ratio of lateral to axial strain.
    PoissonRatio,
    /// Linear thermal expansion, per kelvin.
    ThermalExpansionCoefficient,
    /// Characteristic cylinder compressive strength of concrete.
    CharacteristicCompressiveStrength,
    /// Mean cylinder compressive strength of concrete.
    MeanCompressiveStrength,
    /// Mean axial tensile strength of concrete.
    MeanTensileStrength,
    /// Compressive strain at the peak of the concrete parabola.
    StrainAtPeakStress,
    /// Ultimate compressive strain of concrete.
    UltimateCompressiveStrain,
    /// Exponent of the concrete parabola.
    ParabolaExponent,
    /// Yield strength of steel.
    YieldStrength,
    /// Ultimate tensile strength of steel.
    UltimateStrength,
    /// Strain of steel at maximum load.
    UltimateStrain,
    /// Ratio of steel tensile strength to yield strength.
    HardeningRatio,
}

impl Property {
    /// Every property, in declaration order.
    pub const ALL: [Self; 14] = [
        Self::MassDensity,
        Self::YoungModulus,
        Self::PoissonRatio,
        Self::ThermalExpansionCoefficient,
        Self::CharacteristicCompressiveStrength,
        Self::MeanCompressiveStrength,
        Self::MeanTensileStrength,
        Self::StrainAtPeakStress,
        Self::UltimateCompressiveStrain,
        Self::ParabolaExponent,
        Self::YieldStrength,
        Self::UltimateStrength,
        Self::UltimateStrain,
        Self::HardeningRatio,
    ];

    /// The property's name in its property set.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MassDensity => "MassDensity",
            Self::YoungModulus => "YoungModulus",
            Self::PoissonRatio => "PoissonRatio",
            Self::ThermalExpansionCoefficient => "ThermalExpansionCoefficient",
            Self::CharacteristicCompressiveStrength => "CharacteristicCompressiveStrength",
            Self::MeanCompressiveStrength => "MeanCompressiveStrength",
            Self::MeanTensileStrength => "MeanTensileStrength",
            Self::StrainAtPeakStress => "StrainAtPeakStress",
            Self::UltimateCompressiveStrain => "UltimateCompressiveStrain",
            Self::ParabolaExponent => "ParabolaExponent",
            Self::YieldStrength => "YieldStrength",
            Self::UltimateStrength => "UltimateStrength",
            Self::UltimateStrain => "UltimateStrain",
            Self::HardeningRatio => "HardeningRatio",
        }
    }

    /// The property set that holds the property.
    #[must_use]
    pub const fn property_set(self) -> &'static str {
        match self {
            Self::MassDensity
            | Self::YoungModulus
            | Self::PoissonRatio
            | Self::ThermalExpansionCoefficient => MECHANICAL_SET,
            Self::CharacteristicCompressiveStrength
            | Self::MeanCompressiveStrength
            | Self::MeanTensileStrength
            | Self::StrainAtPeakStress
            | Self::UltimateCompressiveStrain
            | Self::ParabolaExponent => CONCRETE_SET,
            Self::YieldStrength
            | Self::UltimateStrength
            | Self::UltimateStrain
            | Self::HardeningRatio => STEEL_SET,
        }
    }

    /// Whether `value` is physically meaningful for the property.
    #[must_use]
    pub fn accepts(self, value: f64) -> bool {
        match self {
            Self::PoissonRatio => (0.0..0.5).contains(&value),
            Self::StrainAtPeakStress | Self::UltimateCompressiveStrain | Self::UltimateStrain => {
                value > 0.0 && value < 1.0
            }
            Self::HardeningRatio => value.is_finite() && value >= 1.0,
            _ => value.is_finite() && value > 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_sets_follow_the_schema_naming() {
        assert_eq!(Property::YoungModulus.name(), "YoungModulus");
        assert_eq!(Property::PoissonRatio.property_set(), MECHANICAL_SET);
        assert_eq!(Property::ParabolaExponent.property_set(), CONCRETE_SET);
        assert_eq!(Property::HardeningRatio.property_set(), STEEL_SET);
    }

    #[test]
    fn values_outside_their_physical_range_are_refused() {
        for property in Property::ALL {
            assert!(!property.accepts(f64::NAN), "{property:?}");
            assert!(!property.accepts(-1.0), "{property:?}");
            assert!(!property.accepts(f64::INFINITY), "{property:?}");
        }
        assert!(Property::PoissonRatio.accepts(0.0));
        assert!(Property::PoissonRatio.accepts(0.2));
        assert!(!Property::PoissonRatio.accepts(0.5));
        assert!(Property::UltimateStrain.accepts(0.05));
        assert!(!Property::UltimateStrain.accepts(1.0));
        assert!(Property::HardeningRatio.accepts(1.0));
        assert!(!Property::HardeningRatio.accepts(0.99));
        assert!(Property::YoungModulus.accepts(210e9));
        assert!(!Property::YoungModulus.accepts(0.0));
    }
}
