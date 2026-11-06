//! Behaviour laws: the stress a material carries for a given strain (MAT-01).
//!
//! Strains and stresses are positive in tension and negative in compression. The laws hold no
//! partial factor: [`BehaviourLaw::with_strength_divided_by`] gives a design law from the
//! divisor the national annex in force sets (REP-05).

use core_model::units::{Quantity, Stress};
use thiserror::Error;

/// The kind of a behaviour law, as the `behaviour_law` attribute stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LawKind {
    /// Stress proportional to strain, in tension and compression.
    LinearElastic,
    /// The parabola-rectangle diagram for concrete in compression (EN 1992-1-1, 3.1.7).
    ConcreteParabolaRectangle,
    /// The bi-linear diagram for concrete in compression (EN 1992-1-1, 3.1.7(2)).
    ConcreteBilinear,
    /// Linear up to yield, then a hardening or horizontal branch (EN 1992-1-1, 3.2.7;
    /// EN 1993-1-1, 5.4.3).
    SteelBilinear,
    /// Linear up to the strength, brittle in tension and plastic in compression (EN 1995-1-1).
    TimberElasticBrittle,
    /// No structural behaviour: the material carries no stress in design.
    None,
}

impl LawKind {
    /// Every kind.
    pub const ALL: [Self; 6] = [
        Self::LinearElastic,
        Self::ConcreteParabolaRectangle,
        Self::ConcreteBilinear,
        Self::SteelBilinear,
        Self::TimberElasticBrittle,
        Self::None,
    ];

    /// The name stored in the model.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LinearElastic => "linear_elastic",
            Self::ConcreteParabolaRectangle => "concrete_parabola_rectangle",
            Self::ConcreteBilinear => "concrete_bilinear",
            Self::SteelBilinear => "steel_bilinear",
            Self::TimberElasticBrittle => "timber_elastic_brittle",
            Self::None => "none",
        }
    }

    /// The kind stored under `name`, if any.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// Why a law could not be built or evaluated. Messages never repeat values (SEC-03).
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum LawError {
    /// A law parameter is not finite, not positive, or out of order with another.
    #[error("the behaviour law parameter \"{0}\" is not valid")]
    InvalidParameter(&'static str),
    /// The strain is NaN or infinite.
    #[error("the strain is not a finite number")]
    NonFiniteStrain,
    /// The strain is beyond the ultimate strain of the law.
    #[error("the strain is beyond the ultimate strain of the behaviour law")]
    BeyondUltimateStrain,
    /// The material has no structural behaviour, so it carries no stress.
    #[error("the material has no structural behaviour law")]
    NoStructuralLaw,
}

/// A behaviour law with its parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BehaviourLaw {
    /// Stress equals the modulus times the strain.
    LinearElastic(LinearElastic),
    /// Concrete in compression; no tension.
    ConcreteParabolaRectangle(ConcreteParabolaRectangle),
    /// Concrete in compression, bi-linear; no tension.
    ConcreteBilinear(ConcreteBilinear),
    /// Steel, the same in tension and compression.
    SteelBilinear(SteelBilinear),
    /// Timber parallel to the grain: brittle in tension, plastic in compression.
    TimberElasticBrittle(TimberElasticBrittle),
    /// No structural behaviour.
    None,
}

impl BehaviourLaw {
    /// The law's kind.
    #[must_use]
    pub const fn kind(&self) -> LawKind {
        match self {
            Self::LinearElastic(_) => LawKind::LinearElastic,
            Self::ConcreteParabolaRectangle(_) => LawKind::ConcreteParabolaRectangle,
            Self::ConcreteBilinear(_) => LawKind::ConcreteBilinear,
            Self::SteelBilinear(_) => LawKind::SteelBilinear,
            Self::TimberElasticBrittle(_) => LawKind::TimberElasticBrittle,
            Self::None => LawKind::None,
        }
    }

    /// The stress for `strain`.
    ///
    /// # Errors
    ///
    /// Fails when the strain is not finite or lies beyond the law's ultimate strain, and for a
    /// material without structural behaviour.
    pub fn stress(&self, strain: f64) -> Result<Stress, LawError> {
        if !strain.is_finite() {
            return Err(LawError::NonFiniteStrain);
        }
        match self {
            Self::LinearElastic(law) => Ok(law.modulus * strain),
            Self::ConcreteParabolaRectangle(law) => law.stress(strain),
            Self::ConcreteBilinear(law) => law.stress(strain),
            Self::SteelBilinear(law) => law.stress(strain),
            Self::TimberElasticBrittle(law) => law.stress(strain),
            Self::None => Err(LawError::NoStructuralLaw),
        }
    }

    /// The same law with its strength divided by `divisor`, such as γc / αcc for a design
    /// diagram. A linear elastic law has no strength and is returned unchanged, as is the law
    /// of a material without structural behaviour.
    ///
    /// # Errors
    ///
    /// Fails when `divisor` is not finite and positive, the divided strength is not finite and
    /// positive, or the steel yield strain would no longer lie below its ultimate strain.
    pub fn with_strength_divided_by(self, divisor: f64) -> Result<Self, LawError> {
        positive("divisor", divisor)?;
        Ok(match self {
            Self::LinearElastic(_) | Self::None => self,
            Self::ConcreteParabolaRectangle(law) => {
                Self::ConcreteParabolaRectangle(ConcreteParabolaRectangle::new(
                    law.strength / divisor,
                    law.strain_at_peak,
                    law.ultimate_strain,
                    law.exponent,
                )?)
            }
            Self::ConcreteBilinear(law) => Self::ConcreteBilinear(ConcreteBilinear::new(
                law.strength / divisor,
                law.strain_at_peak,
                law.ultimate_strain,
            )?),
            Self::SteelBilinear(law) => Self::SteelBilinear(SteelBilinear::new(
                law.yield_strength / divisor,
                law.modulus,
                law.hardening,
            )?),
            Self::TimberElasticBrittle(law) => {
                Self::TimberElasticBrittle(TimberElasticBrittle::new(
                    law.modulus,
                    law.tensile_strength / divisor,
                    law.compressive_strength / divisor,
                )?)
            }
        })
    }
}

/// A linear elastic law: stress equals the modulus times the strain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearElastic {
    modulus: Stress,
}

impl LinearElastic {
    /// A law with modulus of elasticity `modulus`.
    ///
    /// # Errors
    ///
    /// Fails when the modulus is not finite and positive.
    pub fn new(modulus: Stress) -> Result<Self, LawError> {
        positive("modulus", modulus.si())?;
        Ok(Self { modulus })
    }

    /// The modulus of elasticity.
    #[must_use]
    pub const fn modulus(&self) -> Stress {
        self.modulus
    }
}

/// The parabola-rectangle diagram for concrete (EN 1992-1-1, 3.1.7): a parabola up to the peak
/// strain, then constant stress up to the ultimate strain. Concrete carries no tension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConcreteParabolaRectangle {
    strength: Stress,
    strain_at_peak: f64,
    ultimate_strain: f64,
    exponent: f64,
}

impl ConcreteParabolaRectangle {
    /// A diagram reaching `strength` at compressive strain `strain_at_peak`, up to
    /// `ultimate_strain`, with parabola exponent `exponent`; strains are given as positive
    /// ratios.
    ///
    /// # Errors
    ///
    /// Fails when a parameter is not finite and positive, or the ultimate strain is below the
    /// peak strain.
    pub fn new(
        strength: Stress,
        strain_at_peak: f64,
        ultimate_strain: f64,
        exponent: f64,
    ) -> Result<Self, LawError> {
        positive("strength", strength.si())?;
        positive("strain_at_peak", strain_at_peak)?;
        positive("ultimate_strain", ultimate_strain)?;
        positive("exponent", exponent)?;
        if ultimate_strain < strain_at_peak {
            return Err(LawError::InvalidParameter("ultimate_strain"));
        }
        Ok(Self {
            strength,
            strain_at_peak,
            ultimate_strain,
            exponent,
        })
    }

    /// The peak compressive stress.
    #[must_use]
    pub const fn strength(&self) -> Stress {
        self.strength
    }

    fn stress(&self, strain: f64) -> Result<Stress, LawError> {
        let compression = -strain;
        if compression <= 0.0 {
            return Ok(Quantity::from_si(0.0));
        }
        if compression > self.ultimate_strain {
            return Err(LawError::BeyondUltimateStrain);
        }
        let ratio = if compression < self.strain_at_peak {
            1.0 - libm::pow(1.0 - compression / self.strain_at_peak, self.exponent)
        } else {
            1.0
        };
        Ok(-(self.strength * ratio))
    }
}

/// The bi-linear diagram for concrete (EN 1992-1-1, 3.1.7(2)): stress grows linearly to the
/// strength at the peak strain, then stays constant up to the ultimate strain. Concrete carries
/// no tension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConcreteBilinear {
    strength: Stress,
    strain_at_peak: f64,
    ultimate_strain: f64,
}

impl ConcreteBilinear {
    /// A diagram reaching `strength` at compressive strain `strain_at_peak` (εc3), up to
    /// `ultimate_strain` (εcu3); strains are given as positive ratios.
    ///
    /// # Errors
    ///
    /// Fails when a parameter is not finite and positive, or the ultimate strain is below the
    /// peak strain.
    pub fn new(
        strength: Stress,
        strain_at_peak: f64,
        ultimate_strain: f64,
    ) -> Result<Self, LawError> {
        positive("strength", strength.si())?;
        positive("strain_at_peak", strain_at_peak)?;
        positive("ultimate_strain", ultimate_strain)?;
        if ultimate_strain < strain_at_peak {
            return Err(LawError::InvalidParameter("ultimate_strain"));
        }
        Ok(Self {
            strength,
            strain_at_peak,
            ultimate_strain,
        })
    }

    /// The peak compressive stress.
    #[must_use]
    pub const fn strength(&self) -> Stress {
        self.strength
    }

    fn stress(&self, strain: f64) -> Result<Stress, LawError> {
        let compression = -strain;
        if compression <= 0.0 {
            return Ok(Quantity::from_si(0.0));
        }
        if compression > self.ultimate_strain {
            return Err(LawError::BeyondUltimateStrain);
        }
        let ratio = (compression / self.strain_at_peak).min(1.0);
        Ok(-(self.strength * ratio))
    }
}

/// Timber parallel to the grain: linear elastic up to the tensile strength, where it breaks,
/// and up to the compressive strength, beyond which it stays at that stress (plastic).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimberElasticBrittle {
    modulus: Stress,
    tensile_strength: Stress,
    compressive_strength: Stress,
}

impl TimberElasticBrittle {
    /// A law with modulus `modulus` (E0,mean), breaking at `tensile_strength` (ft,0) and
    /// yielding at `compressive_strength` (fc,0), both given as positive stresses.
    ///
    /// # Errors
    ///
    /// Fails when a parameter is not finite and positive.
    pub fn new(
        modulus: Stress,
        tensile_strength: Stress,
        compressive_strength: Stress,
    ) -> Result<Self, LawError> {
        positive("modulus", modulus.si())?;
        positive("tensile_strength", tensile_strength.si())?;
        positive("compressive_strength", compressive_strength.si())?;
        Ok(Self {
            modulus,
            tensile_strength,
            compressive_strength,
        })
    }

    /// The tensile strength, where the law breaks.
    #[must_use]
    pub const fn tensile_strength(&self) -> Stress {
        self.tensile_strength
    }

    /// The compressive strength, where the law turns plastic.
    #[must_use]
    pub const fn compressive_strength(&self) -> Stress {
        self.compressive_strength
    }

    fn stress(&self, strain: f64) -> Result<Stress, LawError> {
        if strain > self.tensile_strength / self.modulus {
            return Err(LawError::BeyondUltimateStrain);
        }
        let stress = self.modulus * strain;
        Ok(if stress < -self.compressive_strength {
            -self.compressive_strength
        } else {
            stress
        })
    }
}

/// A bilinear steel law: elastic up to the yield strength, then a straight branch up to the
/// tensile strength at the ultimate strain, or a horizontal branch without limit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SteelBilinear {
    yield_strength: Stress,
    modulus: Stress,
    hardening: Option<Hardening>,
}

/// The inclined branch of a bilinear steel law.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hardening {
    /// The ratio of tensile strength to yield strength, at least 1.
    pub ratio: f64,
    /// The strain at which the tensile strength is reached, the limit of the law.
    pub ultimate_strain: f64,
}

impl SteelBilinear {
    /// A law yielding at `yield_strength` with modulus `modulus`, hardening as `hardening` says,
    /// or perfectly plastic without limit when it is `None`.
    ///
    /// # Errors
    ///
    /// Fails when a parameter is not finite and positive, the hardening ratio is below 1, or the
    /// ultimate strain is not beyond the yield strain.
    pub fn new(
        yield_strength: Stress,
        modulus: Stress,
        hardening: Option<Hardening>,
    ) -> Result<Self, LawError> {
        positive("yield_strength", yield_strength.si())?;
        positive("modulus", modulus.si())?;
        if let Some(hardening) = hardening {
            if !(hardening.ratio.is_finite() && hardening.ratio >= 1.0) {
                return Err(LawError::InvalidParameter("ratio"));
            }
            let yield_strain = yield_strength / modulus;
            if !(hardening.ultimate_strain.is_finite() && hardening.ultimate_strain > yield_strain)
            {
                return Err(LawError::InvalidParameter("ultimate_strain"));
            }
        }
        Ok(Self {
            yield_strength,
            modulus,
            hardening,
        })
    }

    /// The yield strength.
    #[must_use]
    pub const fn yield_strength(&self) -> Stress {
        self.yield_strength
    }

    fn stress(&self, strain: f64) -> Result<Stress, LawError> {
        let magnitude = strain.abs();
        let yield_strain = self.yield_strength / self.modulus;
        let stress = if magnitude <= yield_strain {
            self.modulus * magnitude
        } else if let Some(hardening) = self.hardening {
            if magnitude > hardening.ultimate_strain {
                return Err(LawError::BeyondUltimateStrain);
            }
            let progress = (magnitude - yield_strain) / (hardening.ultimate_strain - yield_strain);
            self.yield_strength * (1.0 + (hardening.ratio - 1.0) * progress)
        } else {
            self.yield_strength
        };
        Ok(stress * strain.signum())
    }
}

fn positive(name: &'static str, value: f64) -> Result<(), LawError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(LawError::InvalidParameter(name))
    }
}

#[cfg(test)]
mod tests {
    use core_model::units::StressUnit::Megapascal;
    use proptest::prelude::*;

    use super::*;

    fn mpa(value: f64) -> Stress {
        Quantity::new(value, Megapascal)
    }

    fn assert_mpa(actual: Result<Stress, LawError>, expected: f64) {
        let actual = actual.unwrap().get(Megapascal);
        assert!(
            (actual - expected).abs() < 1e-9,
            "{actual} MPa, expected {expected} MPa"
        );
    }

    fn concrete() -> BehaviourLaw {
        BehaviourLaw::ConcreteParabolaRectangle(
            ConcreteParabolaRectangle::new(mpa(30.0), 0.002, 0.0035, 2.0).unwrap(),
        )
    }

    fn steel(hardening: Option<Hardening>) -> BehaviourLaw {
        BehaviourLaw::SteelBilinear(
            SteelBilinear::new(mpa(500.0), mpa(200_000.0), hardening).unwrap(),
        )
    }

    const B500B_HARDENING: Hardening = Hardening {
        ratio: 1.08,
        ultimate_strain: 0.05,
    };

    #[test]
    fn kinds_round_trip_through_their_names() {
        for kind in LawKind::ALL {
            assert_eq!(LawKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(LawKind::from_name("plastic"), None);
        assert_eq!(concrete().kind(), LawKind::ConcreteParabolaRectangle);
        assert_eq!(steel(None).kind(), LawKind::SteelBilinear);
        let elastic = BehaviourLaw::LinearElastic(LinearElastic::new(mpa(33_000.0)).unwrap());
        assert_eq!(elastic.kind(), LawKind::LinearElastic);
    }

    // Hand calculations (ENG-03): E = 33 000 MPa, so 1e-4 gives 3.3 MPa either way.
    #[test]
    fn a_linear_elastic_law_is_proportional() {
        let elastic = LinearElastic::new(mpa(33_000.0)).unwrap();
        assert_mpa(Ok(elastic.modulus()), 33_000.0);
        let law = BehaviourLaw::LinearElastic(elastic);
        assert_mpa(law.stress(1e-4), 3.3);
        assert_mpa(law.stress(-1e-4), -3.3);
        assert_eq!(law.with_strength_divided_by(1.5), Ok(law));
    }

    // fc = 30 MPa, εc2 = 2 ‰, εcu2 = 3.5 ‰, n = 2. At 1 ‰: 30 × (1 − (1 − 0.5)²) = 22.5 MPa.
    #[test]
    fn concrete_follows_the_parabola_then_the_rectangle() {
        let law = concrete();
        assert_mpa(law.stress(0.001), 0.0);
        assert_mpa(law.stress(0.0), 0.0);
        assert_mpa(law.stress(-0.001), -22.5);
        assert_mpa(law.stress(-0.002), -30.0);
        assert_mpa(law.stress(-0.003), -30.0);
        assert_mpa(law.stress(-0.0035), -30.0);
        assert_eq!(law.stress(-0.0036), Err(LawError::BeyondUltimateStrain));
        assert_eq!(law.stress(f64::NAN), Err(LawError::NonFiniteStrain));
    }

    // fcd = 30 / 1.5 = 20 MPa.
    #[test]
    fn a_design_law_divides_the_strength() {
        let design = concrete().with_strength_divided_by(1.5).unwrap();
        assert_mpa(design.stress(-0.002), -20.0);
        let BehaviourLaw::ConcreteParabolaRectangle(diagram) = design else {
            panic!("the kind changed");
        };
        assert_mpa(Ok(diagram.strength()), 20.0);
        let design = steel(Some(B500B_HARDENING))
            .with_strength_divided_by(1.15)
            .unwrap();
        assert_mpa(design.stress(0.04), {
            // fyd = 434.78 MPa; εyd = 2.174 ‰; then linear to 1.08 fyd at 5 %.
            let fyd = 500.0 / 1.15;
            let eyd = fyd / 200_000.0;
            fyd * (1.0 + 0.08 * (0.04 - eyd) / (0.05 - eyd))
        });
        // A tiny divisor overflows the strength to infinity.
        assert_eq!(
            concrete().with_strength_divided_by(1e-310),
            Err(LawError::InvalidParameter("strength"))
        );
        // A divisor below 1 can raise the yield strain past the ultimate strain.
        assert_eq!(
            steel(Some(B500B_HARDENING)).with_strength_divided_by(0.01),
            Err(LawError::InvalidParameter("ultimate_strain"))
        );
        for divisor in [0.0, -1.0, f64::NAN] {
            assert_eq!(
                concrete().with_strength_divided_by(divisor),
                Err(LawError::InvalidParameter("divisor"))
            );
        }
    }

    // fy = 500 MPa, Es = 200 000 MPa, so εy = 2.5 ‰; k = 1.08 at 5 %: halfway gives 520 MPa.
    #[test]
    fn steel_hardens_up_to_its_ultimate_strain() {
        let law = steel(Some(B500B_HARDENING));
        assert_mpa(law.stress(0.001), 200.0);
        assert_mpa(law.stress(-0.001), -200.0);
        assert_mpa(law.stress(0.0025), 500.0);
        assert_mpa(law.stress(0.026_25), 520.0);
        assert_mpa(law.stress(-0.05), -540.0);
        assert_eq!(law.stress(0.06), Err(LawError::BeyondUltimateStrain));
        let BehaviourLaw::SteelBilinear(bilinear) = law else {
            panic!("not steel");
        };
        assert_mpa(Ok(bilinear.yield_strength()), 500.0);
    }

    #[test]
    fn perfectly_plastic_steel_has_no_limit() {
        let law = steel(None);
        assert_mpa(law.stress(0.01), 500.0);
        assert_mpa(law.stress(-0.5), -500.0);
    }

    #[test]
    fn invalid_parameters_are_named() {
        let invalid = LawError::InvalidParameter;
        assert_eq!(
            ConcreteParabolaRectangle::new(mpa(0.0), 0.002, 0.0035, 2.0),
            Err(invalid("strength"))
        );
        assert_eq!(
            ConcreteParabolaRectangle::new(mpa(30.0), f64::NAN, 0.0035, 2.0),
            Err(invalid("strain_at_peak"))
        );
        assert_eq!(
            ConcreteParabolaRectangle::new(mpa(30.0), 0.002, -1.0, 2.0),
            Err(invalid("ultimate_strain"))
        );
        assert_eq!(
            ConcreteParabolaRectangle::new(mpa(30.0), 0.002, 0.0035, 0.0),
            Err(invalid("exponent"))
        );
        assert_eq!(
            ConcreteParabolaRectangle::new(mpa(30.0), 0.002, 0.001, 2.0),
            Err(invalid("ultimate_strain"))
        );
        assert_eq!(LinearElastic::new(mpa(f64::NAN)), Err(invalid("modulus")));
        assert_eq!(
            SteelBilinear::new(mpa(-1.0), mpa(1.0), None),
            Err(invalid("yield_strength"))
        );
        assert_eq!(
            SteelBilinear::new(mpa(1.0), mpa(0.0), None),
            Err(invalid("modulus"))
        );
        let soft = Hardening {
            ratio: 0.9,
            ultimate_strain: 0.05,
        };
        assert_eq!(
            SteelBilinear::new(mpa(500.0), mpa(200_000.0), Some(soft)),
            Err(invalid("ratio"))
        );
        let short = Hardening {
            ratio: 1.08,
            ultimate_strain: 0.002,
        };
        assert_eq!(
            SteelBilinear::new(mpa(500.0), mpa(200_000.0), Some(short)),
            Err(invalid("ultimate_strain"))
        );
        assert_eq!(
            invalid("modulus").to_string(),
            "the behaviour law parameter \"modulus\" is not valid"
        );
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn concrete_stress_never_decreases_and_stays_within_strength(
            a in -0.0035f64..0.01,
            b in -0.0035f64..0.01,
        ) {
            let law = concrete();
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            let (low, high) = (law.stress(low).unwrap(), law.stress(high).unwrap());
            prop_assert!(low <= high);
            prop_assert!(low.get(Megapascal) >= -30.0 - 1e-9);
        }

        #[test]
        fn steel_stress_never_decreases_and_stays_within_tensile_strength(
            a in -0.05f64..0.05,
            b in -0.05f64..0.05,
        ) {
            let law = steel(Some(B500B_HARDENING));
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            let (low, high) = (law.stress(low).unwrap(), law.stress(high).unwrap());
            prop_assert!(low <= high);
            prop_assert!(high.get(Megapascal) <= 540.0 + 1e-9);
        }
    }

    fn concrete_bilinear() -> BehaviourLaw {
        BehaviourLaw::ConcreteBilinear(ConcreteBilinear::new(mpa(30.0), 0.001_75, 0.0035).unwrap())
    }

    // GL24h parallel to the grain: E0,mean = 11 500 MPa, ft,0,k = 19.2 MPa, fc,0,k = 24 MPa.
    fn timber() -> BehaviourLaw {
        BehaviourLaw::TimberElasticBrittle(
            TimberElasticBrittle::new(mpa(11_500.0), mpa(19.2), mpa(24.0)).unwrap(),
        )
    }

    // Hand calculations (ENG-03): fc = 30 MPa, εc3 = 1.75 ‰, εcu3 = 3.5 ‰.
    // Halfway up the rising branch, at 0.875 ‰: 30 × 0.875 / 1.75 = 15 MPa.
    #[test]
    fn bilinear_concrete_rises_linearly_then_stays_at_its_strength() {
        let law = concrete_bilinear();
        assert_eq!(law.kind(), LawKind::ConcreteBilinear);
        assert_mpa(law.stress(0.001), 0.0);
        assert_mpa(law.stress(0.0), 0.0);
        assert_mpa(law.stress(-0.000_875), -15.0);
        assert_mpa(law.stress(-0.001_75), -30.0);
        assert_mpa(law.stress(-0.003), -30.0);
        assert_mpa(law.stress(-0.0035), -30.0);
        assert_eq!(law.stress(-0.0036), Err(LawError::BeyondUltimateStrain));
        // fcd = 30 / 1.5 = 20 MPa, reached at the same εc3.
        let design = law.with_strength_divided_by(1.5).unwrap();
        assert_mpa(design.stress(-0.002), -20.0);
        assert_mpa(design.stress(-0.000_875), -10.0);
        let BehaviourLaw::ConcreteBilinear(diagram) = design else {
            panic!("the kind changed");
        };
        assert_mpa(Ok(diagram.strength()), 20.0);
    }

    // Tension breaks at ft / E = 19.2 / 11 500 = 1.6696 ‰; compression turns plastic at
    // fc / E = 24 / 11 500 = 2.0870 ‰. At 1 ‰ either way: 11 500 × 0.001 = 11.5 MPa.
    #[test]
    fn timber_is_brittle_in_tension_and_plastic_in_compression() {
        let law = timber();
        assert_eq!(law.kind(), LawKind::TimberElasticBrittle);
        assert_mpa(law.stress(0.0), 0.0);
        assert_mpa(law.stress(0.001), 11.5);
        assert_mpa(law.stress(0.0016), 18.4);
        assert_mpa(law.stress(19.2 / 11_500.0), 19.2);
        assert_eq!(law.stress(0.0017), Err(LawError::BeyondUltimateStrain));
        assert_mpa(law.stress(-0.001), -11.5);
        assert_mpa(law.stress(-24.0 / 11_500.0), -24.0);
        assert_mpa(law.stress(-0.003), -24.0);
        assert_mpa(law.stress(-0.1), -24.0);
        // γM = 1.25: ft,d = 15.36 MPa and fc,d = 19.2 MPa, with the same modulus.
        let design = law.with_strength_divided_by(1.25).unwrap();
        assert_mpa(design.stress(-0.003), -19.2);
        assert_mpa(design.stress(0.001), 11.5);
        assert_eq!(design.stress(0.0014), Err(LawError::BeyondUltimateStrain));
        let BehaviourLaw::TimberElasticBrittle(diagram) = design else {
            panic!("the kind changed");
        };
        assert_mpa(Ok(diagram.tensile_strength()), 15.36);
        assert_mpa(Ok(diagram.compressive_strength()), 19.2);
    }

    #[test]
    fn a_material_without_structural_law_carries_no_stress() {
        let law = BehaviourLaw::None;
        assert_eq!(law.kind(), LawKind::None);
        assert_eq!(law.stress(0.0), Err(LawError::NoStructuralLaw));
        assert_eq!(law.stress(f64::NAN), Err(LawError::NonFiniteStrain));
        assert_eq!(law.with_strength_divided_by(1.5), Ok(BehaviourLaw::None));
        assert_eq!(
            LawError::NoStructuralLaw.to_string(),
            "the material has no structural behaviour law"
        );
    }

    #[test]
    fn invalid_parameters_of_the_new_laws_are_named() {
        let invalid = LawError::InvalidParameter;
        assert_eq!(
            ConcreteBilinear::new(mpa(0.0), 0.001_75, 0.0035),
            Err(invalid("strength"))
        );
        assert_eq!(
            ConcreteBilinear::new(mpa(30.0), -1.0, 0.0035),
            Err(invalid("strain_at_peak"))
        );
        assert_eq!(
            ConcreteBilinear::new(mpa(30.0), 0.001_75, f64::NAN),
            Err(invalid("ultimate_strain"))
        );
        assert_eq!(
            ConcreteBilinear::new(mpa(30.0), 0.001_75, 0.001),
            Err(invalid("ultimate_strain"))
        );
        assert_eq!(
            TimberElasticBrittle::new(mpa(0.0), mpa(19.2), mpa(24.0)),
            Err(invalid("modulus"))
        );
        assert_eq!(
            TimberElasticBrittle::new(mpa(11_500.0), mpa(-1.0), mpa(24.0)),
            Err(invalid("tensile_strength"))
        );
        assert_eq!(
            TimberElasticBrittle::new(mpa(11_500.0), mpa(19.2), mpa(f64::INFINITY)),
            Err(invalid("compressive_strength"))
        );
        assert_eq!(
            concrete_bilinear().with_strength_divided_by(1e-310),
            Err(invalid("strength"))
        );
        assert_eq!(
            timber().with_strength_divided_by(1e-310),
            Err(invalid("tensile_strength"))
        );
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn bilinear_concrete_never_decreases_and_stays_within_strength(
            a in -0.0035f64..0.01,
            b in -0.0035f64..0.01,
        ) {
            let law = concrete_bilinear();
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            let (low, high) = (law.stress(low).unwrap(), law.stress(high).unwrap());
            prop_assert!(low <= high);
            prop_assert!(low.get(Megapascal) >= -30.0 - 1e-9);
            prop_assert!(high.get(Megapascal) <= 0.0);
        }

        #[test]
        fn timber_never_decreases_and_stays_within_its_strengths(
            a in -0.1f64..0.001_669,
            b in -0.1f64..0.001_669,
        ) {
            let law = timber();
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            let (low, high) = (law.stress(low).unwrap(), law.stress(high).unwrap());
            prop_assert!(low <= high);
            prop_assert!(low.get(Megapascal) >= -24.0 - 1e-9);
            prop_assert!(high.get(Megapascal) <= 19.2 + 1e-9);
        }

        // The stress never jumps: across any strain step δ it changes by at most the steepest
        // slope times δ (fc / εc3 for concrete, E for timber), branch ends included.
        #[test]
        fn the_new_laws_are_continuous(strain in -0.0034f64..0.001_6, step in 0.0f64..1e-5) {
            let concrete = concrete_bilinear();
            let jump = (concrete.stress(strain + step).unwrap()
                - concrete.stress(strain).unwrap()).get(Megapascal).abs();
            prop_assert!(jump <= 30.0 / 0.001_75 * step + 1e-9);
            let timber = timber();
            let jump = (timber.stress(strain + step).unwrap()
                - timber.stress(strain).unwrap()).get(Megapascal).abs();
            prop_assert!(jump <= 11_500.0 * step + 1e-9);
        }
    }
}
