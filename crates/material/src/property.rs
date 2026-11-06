//! The material properties the core schema defines, each a sourced attribute of `Material`.

use crate::annex::{Factor, NationalAnnex};

/// A material property (MAT-01), stored in the `Material` attribute [`Property::name`] gives.
/// Values are in SI units: stresses and moduli in pascals, densities in kilograms per cubic
/// metre, strains and ratios without unit, expansion per kelvin, conductivity in W/(m·K) and
/// specific heat in J/(kg·K).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Property {
    /// Mass per unit volume, in kg/m³.
    MassDensity,
    /// Modulus of elasticity; for concrete, the secant modulus Ecm.
    YoungModulus,
    /// Shear modulus G.
    ShearModulus,
    /// Ratio of lateral to axial strain under axial load.
    PoissonRatio,
    /// Linear thermal expansion coefficient, per kelvin.
    ThermalExpansionCoefficient,
    /// Thermal conductivity λ, in W/(m·K).
    ThermalConductivity,
    /// Specific heat capacity, in J/(kg·K).
    SpecificHeat,
    /// Concrete characteristic cylinder compressive strength at 28 days, fck.
    CharacteristicCompressiveStrength,
    /// Concrete characteristic cube compressive strength at 28 days, fck,cube.
    CharacteristicCubeStrength,
    /// Concrete mean cylinder compressive strength at 28 days, fcm.
    MeanCompressiveStrength,
    /// Concrete mean axial tensile strength, fctm.
    MeanTensileStrength,
    /// Concrete compressive strain at the peak of the parabola-rectangle diagram, εc2, as a ratio.
    StrainAtPeakStress,
    /// Concrete ultimate compressive strain of the parabola-rectangle diagram, εcu2, as a ratio.
    UltimateCompressiveStrain,
    /// Exponent n of the parabola of the parabola-rectangle diagram.
    ParabolaExponent,
    /// Concrete compressive strain at the end of the rising branch of the bilinear diagram, εc3, as a ratio.
    BilinearStrainAtPeakStress,
    /// Concrete ultimate compressive strain of the bilinear diagram, εcu3, as a ratio.
    BilinearUltimateCompressiveStrain,
    /// Steel yield strength: fy for structural steel, fyk for reinforcement.
    YieldStrength,
    /// Steel ultimate tensile strength: fu for structural steel, ftk for reinforcement.
    UltimateStrength,
    /// Ratio k of the tensile strength to the yield strength, at least 1.
    HardeningRatio,
    /// Steel strain at maximum load: εu for structural steel, εuk for reinforcement, as a ratio.
    UltimateStrain,
    /// Timber characteristic bending strength, fm,k.
    TimberBendingStrength,
    /// Timber characteristic tensile strength parallel to the grain, ft,0,k.
    TimberTensileStrength,
    /// Timber characteristic compressive strength parallel to the grain, fc,0,k.
    TimberCompressiveStrength,
    /// Timber characteristic shear strength, fv,k.
    TimberShearStrength,
    /// Timber mean modulus of elasticity parallel to the grain, E0,mean.
    TimberMeanModulus,
    /// Timber fifth-percentile modulus of elasticity parallel to the grain, E0,05.
    TimberFifthPercentileModulus,
    /// Timber mean shear modulus, Gmean.
    TimberMeanShearModulus,
    /// Timber characteristic density, ρk.
    CharacteristicDensity,
    /// Timber mean density, ρmean.
    MeanDensity,
    /// Masonry characteristic compressive strength, fk.
    MasonryCompressiveStrength,
    /// A partial factor to a national annex.
    PartialFactor(Factor, NationalAnnex),
}

impl Property {
    /// Every property, in declaration order.
    pub const ALL: [Self; 44] = [
        Self::MassDensity,
        Self::YoungModulus,
        Self::ShearModulus,
        Self::PoissonRatio,
        Self::ThermalExpansionCoefficient,
        Self::ThermalConductivity,
        Self::SpecificHeat,
        Self::CharacteristicCompressiveStrength,
        Self::CharacteristicCubeStrength,
        Self::MeanCompressiveStrength,
        Self::MeanTensileStrength,
        Self::StrainAtPeakStress,
        Self::UltimateCompressiveStrain,
        Self::ParabolaExponent,
        Self::BilinearStrainAtPeakStress,
        Self::BilinearUltimateCompressiveStrain,
        Self::YieldStrength,
        Self::UltimateStrength,
        Self::HardeningRatio,
        Self::UltimateStrain,
        Self::TimberBendingStrength,
        Self::TimberTensileStrength,
        Self::TimberCompressiveStrength,
        Self::TimberShearStrength,
        Self::TimberMeanModulus,
        Self::TimberFifthPercentileModulus,
        Self::TimberMeanShearModulus,
        Self::CharacteristicDensity,
        Self::MeanDensity,
        Self::MasonryCompressiveStrength,
        Self::PartialFactor(Factor::GammaC, NationalAnnex::Recommended),
        Self::PartialFactor(Factor::GammaC, NationalAnnex::France),
        Self::PartialFactor(Factor::AlphaCc, NationalAnnex::Recommended),
        Self::PartialFactor(Factor::AlphaCc, NationalAnnex::France),
        Self::PartialFactor(Factor::GammaS, NationalAnnex::Recommended),
        Self::PartialFactor(Factor::GammaS, NationalAnnex::France),
        Self::PartialFactor(Factor::GammaM0, NationalAnnex::Recommended),
        Self::PartialFactor(Factor::GammaM0, NationalAnnex::France),
        Self::PartialFactor(Factor::GammaM1, NationalAnnex::Recommended),
        Self::PartialFactor(Factor::GammaM1, NationalAnnex::France),
        Self::PartialFactor(Factor::GammaM2, NationalAnnex::Recommended),
        Self::PartialFactor(Factor::GammaM2, NationalAnnex::France),
        Self::PartialFactor(Factor::GammaM, NationalAnnex::Recommended),
        Self::PartialFactor(Factor::GammaM, NationalAnnex::France),
    ];

    /// The name of the `Material` attribute that stores the property.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MassDensity => "density",
            Self::YoungModulus => "e_modulus",
            Self::ShearModulus => "shear_modulus",
            Self::PoissonRatio => "poisson_ratio",
            Self::ThermalExpansionCoefficient => "thermal_expansion",
            Self::ThermalConductivity => "thermal_conductivity",
            Self::SpecificHeat => "specific_heat",
            Self::CharacteristicCompressiveStrength => "fck",
            Self::CharacteristicCubeStrength => "fck_cube",
            Self::MeanCompressiveStrength => "fcm",
            Self::MeanTensileStrength => "fctm",
            Self::StrainAtPeakStress => "eps_c2",
            Self::UltimateCompressiveStrain => "eps_cu2",
            Self::ParabolaExponent => "n",
            Self::BilinearStrainAtPeakStress => "eps_c3",
            Self::BilinearUltimateCompressiveStrain => "eps_cu3",
            Self::YieldStrength => "fy",
            Self::UltimateStrength => "fu",
            Self::HardeningRatio => "k",
            Self::UltimateStrain => "eps_u",
            Self::TimberBendingStrength => "fm_k",
            Self::TimberTensileStrength => "ft0_k",
            Self::TimberCompressiveStrength => "fc0_k",
            Self::TimberShearStrength => "fv_k",
            Self::TimberMeanModulus => "e0_mean",
            Self::TimberFifthPercentileModulus => "e0_05",
            Self::TimberMeanShearModulus => "g_mean",
            Self::CharacteristicDensity => "rho_k",
            Self::MeanDensity => "rho_mean",
            Self::MasonryCompressiveStrength => "fk",
            Self::PartialFactor(factor, annex) => factor.attribute(annex),
        }
    }

    /// The property stored under the attribute `name`, if any.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|property| property.name() == name)
    }

    /// Whether `value` is physically meaningful for the property.
    #[must_use]
    pub fn accepts(self, value: f64) -> bool {
        match self {
            Self::PoissonRatio => (0.0..0.5).contains(&value),
            Self::StrainAtPeakStress
            | Self::UltimateCompressiveStrain
            | Self::BilinearStrainAtPeakStress
            | Self::BilinearUltimateCompressiveStrain
            | Self::UltimateStrain => value > 0.0 && value < 1.0,
            Self::HardeningRatio => value.is_finite() && value >= 1.0,
            Self::PartialFactor(factor, _) => factor.accepts(value),
            _ => value.is_finite() && value > 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_round_trip_through_their_attribute_names() {
        assert_eq!(Property::YoungModulus.name(), "e_modulus");
        for property in Property::ALL {
            assert_eq!(Property::from_name(property.name()), Some(property));
        }
        assert_eq!(Property::from_name("YoungModulus"), None);
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
        assert!(Property::BilinearUltimateCompressiveStrain.accepts(0.0035));
        assert!(Property::HardeningRatio.accepts(1.0));
        assert!(!Property::HardeningRatio.accepts(0.99));
        assert!(Property::YoungModulus.accepts(210e9));
        assert!(!Property::YoungModulus.accepts(0.0));
        assert!(Property::ThermalConductivity.accepts(0.035));
        let gamma_c = Property::PartialFactor(Factor::GammaC, NationalAnnex::France);
        assert_eq!(gamma_c.name(), "gamma_c_fr");
        assert!(gamma_c.accepts(1.5));
        assert!(!gamma_c.accepts(0.9));
    }
}
