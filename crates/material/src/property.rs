//! The material properties the core schema defines, each a sourced attribute of `Material`.

use crate::annex::{Factor, NationalAnnex};

/// What a property measures; a client picks the unit it shows it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Quantity {
    /// A strength, in pascals.
    Stress,
    /// A modulus of elasticity or shear, in pascals.
    Modulus,
    /// A mass per volume, in kg/m³.
    Density,
    /// A strain, as a ratio.
    Strain,
    /// A number without unit, or per kelvin.
    Ratio,
    /// A thermal conductivity, in W/(m·K).
    ThermalConductivity,
    /// A specific heat capacity, in J/(kg·K).
    SpecificHeat,
}

impl Quantity {
    /// The name on the wire.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Stress => "stress",
            Self::Modulus => "modulus",
            Self::Density => "density",
            Self::Strain => "strain",
            Self::Ratio => "ratio",
            Self::ThermalConductivity => "thermal_conductivity",
            Self::SpecificHeat => "specific_heat",
        }
    }
}

/// The bounds of the values a property accepts; `None` is unbounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    /// The lower bound.
    pub min: Option<f64>,
    /// Whether the lower bound itself is accepted.
    pub min_inclusive: bool,
    /// The upper bound.
    pub max: Option<f64>,
    /// Whether the upper bound itself is accepted.
    pub max_inclusive: bool,
}

impl Range {
    const fn new(
        min: Option<f64>,
        min_inclusive: bool,
        max: Option<f64>,
        max_inclusive: bool,
    ) -> Self {
        Self {
            min,
            min_inclusive,
            max,
            max_inclusive,
        }
    }

    /// Whether `value` lies within the bounds.
    #[must_use]
    pub fn contains(&self, value: f64) -> bool {
        let above = self
            .min
            .is_none_or(|min| value > min || (self.min_inclusive && value >= min));
        let below = self
            .max
            .is_none_or(|max| value < max || (self.max_inclusive && value <= max));
        value.is_finite() && above && below
    }
}

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

    /// The property's key on the wire: the attribute name, or for a partial factor the factor's
    /// key, which the annex then completes.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::PartialFactor(factor, _) => factor.key(),
            _ => self.name(),
        }
    }

    /// The national annex of a partial factor; `None` for every other property.
    #[must_use]
    pub const fn annex(self) -> Option<NationalAnnex> {
        match self {
            Self::PartialFactor(_, annex) => Some(annex),
            _ => None,
        }
    }

    /// The property with this key and annex: a partial factor needs its annex, and any other
    /// property takes none.
    #[must_use]
    pub fn from_key(key: &str, annex: Option<NationalAnnex>) -> Option<Self> {
        if let Some(factor) = Factor::from_key(key) {
            return annex.map(|annex| Self::PartialFactor(factor, annex));
        }
        if annex.is_some() {
            return None;
        }
        Self::ALL
            .into_iter()
            .find(|property| property.annex().is_none() && property.name() == key)
    }

    /// What the property measures, so a client can pick its unit.
    #[must_use]
    pub const fn quantity(self) -> Quantity {
        match self {
            Self::CharacteristicCompressiveStrength
            | Self::CharacteristicCubeStrength
            | Self::MeanCompressiveStrength
            | Self::MeanTensileStrength
            | Self::YieldStrength
            | Self::UltimateStrength
            | Self::TimberBendingStrength
            | Self::TimberTensileStrength
            | Self::TimberCompressiveStrength
            | Self::TimberShearStrength
            | Self::MasonryCompressiveStrength => Quantity::Stress,
            Self::YoungModulus
            | Self::ShearModulus
            | Self::TimberMeanModulus
            | Self::TimberFifthPercentileModulus
            | Self::TimberMeanShearModulus => Quantity::Modulus,
            Self::MassDensity | Self::CharacteristicDensity | Self::MeanDensity => {
                Quantity::Density
            }
            Self::StrainAtPeakStress
            | Self::UltimateCompressiveStrain
            | Self::BilinearStrainAtPeakStress
            | Self::BilinearUltimateCompressiveStrain
            | Self::UltimateStrain => Quantity::Strain,
            Self::ThermalConductivity => Quantity::ThermalConductivity,
            Self::SpecificHeat => Quantity::SpecificHeat,
            Self::PoissonRatio
            | Self::ThermalExpansionCoefficient
            | Self::ParabolaExponent
            | Self::HardeningRatio
            | Self::PartialFactor(_, _) => Quantity::Ratio,
        }
    }

    /// The values [`Property::accepts`] accepts, as bounds a client can show.
    #[must_use]
    pub const fn range(self) -> Range {
        const OPEN_ABOVE_ZERO: Range = Range::new(Some(0.0), false, None, false);
        match self {
            Self::PoissonRatio => Range::new(Some(0.0), true, Some(0.5), false),
            Self::StrainAtPeakStress
            | Self::UltimateCompressiveStrain
            | Self::BilinearStrainAtPeakStress
            | Self::BilinearUltimateCompressiveStrain
            | Self::UltimateStrain => Range::new(Some(0.0), false, Some(1.0), false),
            Self::HardeningRatio
            | Self::PartialFactor(
                Factor::GammaC
                | Factor::GammaS
                | Factor::GammaM0
                | Factor::GammaM1
                | Factor::GammaM2
                | Factor::GammaM,
                _,
            ) => Range::new(Some(1.0), true, None, false),
            Self::PartialFactor(Factor::AlphaCc, _) => {
                Range::new(Some(0.0), false, Some(1.0), true)
            }
            _ => OPEN_ABOVE_ZERO,
        }
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

    #[test]
    fn keys_and_annexes_round_trip() {
        for property in Property::ALL {
            let found = Property::from_key(property.key(), property.annex());
            assert_eq!(found, Some(property), "{property:?}");
        }
        let gamma_c = Property::PartialFactor(Factor::GammaC, NationalAnnex::France);
        assert_eq!(gamma_c.key(), "gamma_c");
        assert_eq!(gamma_c.annex(), Some(NationalAnnex::France));
        assert_eq!(Property::from_key("gamma_c", None), None);
        assert_eq!(Property::from_key("fck", Some(NationalAnnex::France)), None);
        assert_eq!(Property::from_key("gamma_c_fr", None), None);
        assert_eq!(Property::from_key("colour", None), None);
    }

    #[test]
    fn ranges_say_what_accepts_accepts() {
        let samples = [
            -1.0,
            0.0,
            1e-12,
            0.002,
            0.2,
            0.4999,
            0.5,
            0.9,
            1.0,
            1.15,
            1.5,
            7850.0,
            2.1e11,
            f64::NAN,
            f64::INFINITY,
        ];
        for property in Property::ALL {
            for value in samples {
                assert_eq!(
                    property.range().contains(value),
                    property.accepts(value),
                    "{property:?} {value}"
                );
            }
        }
        let nu = Property::PoissonRatio.range();
        assert_eq!(
            (nu.min, nu.min_inclusive, nu.max, nu.max_inclusive),
            (Some(0.0), true, Some(0.5), false)
        );
    }

    #[test]
    fn every_property_has_a_quantity_with_a_name() {
        assert_eq!(Property::YieldStrength.quantity().name(), "stress");
        assert_eq!(Property::ShearModulus.quantity().name(), "modulus");
        assert_eq!(Property::MeanDensity.quantity().name(), "density");
        assert_eq!(Property::UltimateStrain.quantity().name(), "strain");
        assert_eq!(Property::HardeningRatio.quantity().name(), "ratio");
        assert_eq!(
            Property::ThermalConductivity.quantity().name(),
            "thermal_conductivity"
        );
        assert_eq!(Property::SpecificHeat.quantity().name(), "specific_heat");
        for property in Property::ALL {
            assert!(!property.quantity().name().is_empty());
        }
    }
}
