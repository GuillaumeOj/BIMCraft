//! National annexes and the partial factors that differ between them (MAT-01, REP-05).
//!
//! The design codes leave the partial factors to each country's national annex. A material keeps
//! one sourced value per factor and annex, so a project can be checked against the recommended
//! values or the French annex without the material changing.

use crate::material::Category;

/// A national annex whose values a material may give.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NationalAnnex {
    /// The values the Eurocode recommends.
    Recommended,
    /// The French national annex (NF EN …/NA).
    France,
}

impl NationalAnnex {
    /// Every annex.
    pub const ALL: [Self; 2] = [Self::Recommended, Self::France];

    /// The annex's name in the API.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Recommended => "recommended",
            Self::France => "france",
        }
    }

    /// The annex named `name`, if any.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|annex| annex.name() == name)
    }
}

/// A partial factor, or coefficient, that a national annex sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Factor {
    /// γc, for concrete.
    GammaC,
    /// αcc, the long-term coefficient on the concrete compressive strength.
    AlphaCc,
    /// γs, for reinforcing steel.
    GammaS,
    /// γM0, for the resistance of steel cross-sections.
    GammaM0,
    /// γM1, for the resistance of steel members to instability.
    GammaM1,
    /// γM2, for the resistance of steel cross-sections in tension to fracture.
    GammaM2,
    /// γM, for timber and masonry.
    GammaM,
}

impl Factor {
    /// Every factor.
    pub const ALL: [Self; 7] = [
        Self::GammaC,
        Self::AlphaCc,
        Self::GammaS,
        Self::GammaM0,
        Self::GammaM1,
        Self::GammaM2,
        Self::GammaM,
    ];

    /// The factor's key in the API, which is also the name of the attribute holding its
    /// recommended value.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::GammaC => "gamma_c",
            Self::AlphaCc => "alpha_cc",
            Self::GammaS => "gamma_s",
            Self::GammaM0 => "gamma_m0",
            Self::GammaM1 => "gamma_m1",
            Self::GammaM2 => "gamma_m2",
            Self::GammaM => "gamma_m",
        }
    }

    /// The name of the `Material` attribute holding the factor's value to `annex`: its key for
    /// the recommended value, with a `_fr` suffix for the French annex.
    #[must_use]
    pub const fn attribute(self, annex: NationalAnnex) -> &'static str {
        match annex {
            NationalAnnex::Recommended => self.key(),
            NationalAnnex::France => match self {
                Self::GammaC => "gamma_c_fr",
                Self::AlphaCc => "alpha_cc_fr",
                Self::GammaS => "gamma_s_fr",
                Self::GammaM0 => "gamma_m0_fr",
                Self::GammaM1 => "gamma_m1_fr",
                Self::GammaM2 => "gamma_m2_fr",
                Self::GammaM => "gamma_m_fr",
            },
        }
    }

    /// The factor with key `key`, if any.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|factor| factor.key() == key)
    }

    /// Whether materials of `category` use the factor.
    #[must_use]
    pub const fn applies_to(self, category: Category) -> bool {
        matches!(
            (self, category),
            (Self::GammaC | Self::AlphaCc, Category::Concrete)
                | (Self::GammaS, Category::Reinforcement)
                | (
                    Self::GammaM0 | Self::GammaM1 | Self::GammaM2,
                    Category::Steel
                )
                | (Self::GammaM, Category::Timber | Category::Masonry)
        )
    }

    /// Whether `value` is meaningful: a partial factor is at least 1, αcc lies in (0, 1].
    #[must_use]
    pub fn accepts(self, value: f64) -> bool {
        match self {
            Self::AlphaCc => value > 0.0 && value <= 1.0,
            _ => value.is_finite() && value >= 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annexes_and_factors_round_trip_through_their_names() {
        for annex in NationalAnnex::ALL {
            assert_eq!(NationalAnnex::from_name(annex.name()), Some(annex));
        }
        assert_eq!(NationalAnnex::from_name("germany"), None);
        for factor in Factor::ALL {
            assert_eq!(Factor::from_key(factor.key()), Some(factor));
        }
        assert_eq!(Factor::from_key("gamma_g"), None);
        for factor in Factor::ALL {
            assert_eq!(factor.attribute(NationalAnnex::Recommended), factor.key());
            assert_eq!(
                factor.attribute(NationalAnnex::France),
                format!("{}_fr", factor.key())
            );
        }
    }

    #[test]
    fn factors_apply_to_their_material_families() {
        assert!(Factor::GammaC.applies_to(Category::Concrete));
        assert!(Factor::AlphaCc.applies_to(Category::Concrete));
        assert!(!Factor::GammaS.applies_to(Category::Concrete));
        assert!(Factor::GammaS.applies_to(Category::Reinforcement));
        assert!(Factor::GammaM1.applies_to(Category::Steel));
        assert!(Factor::GammaM.applies_to(Category::Timber));
        assert!(Factor::GammaM.applies_to(Category::Masonry));
        for factor in Factor::ALL {
            for category in [Category::Insulation, Category::Finish, Category::Generic] {
                assert!(!factor.applies_to(category), "{factor:?} {category:?}");
            }
        }
    }

    #[test]
    fn factors_accept_only_meaningful_values() {
        assert!(Factor::GammaC.accepts(1.5));
        assert!(Factor::GammaM0.accepts(1.0));
        assert!(!Factor::GammaM0.accepts(0.99));
        assert!(!Factor::GammaC.accepts(f64::INFINITY));
        assert!(!Factor::GammaC.accepts(f64::NAN));
        assert!(Factor::AlphaCc.accepts(1.0));
        assert!(Factor::AlphaCc.accepts(0.85));
        assert!(!Factor::AlphaCc.accepts(0.0));
        assert!(!Factor::AlphaCc.accepts(1.01));
        assert!(!Factor::AlphaCc.accepts(f64::NAN));
    }
}
