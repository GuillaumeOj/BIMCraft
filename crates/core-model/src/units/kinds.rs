//! The metric units of each dimension. Imperial units come later (ANA-11).

use std::f64::consts::PI;

use super::sealed::Sealed;
use super::{Dimension, ParseQuantityError, Unit, dms};

/// Defines the unit enum of one dimension: each unit's factor to SI, display symbol and parsing
/// aliases, and optionally the parser of a compound form (see [`Unit::parse_compound`]).
macro_rules! unit_kind {
    (
        $(#[$doc:meta])*
        $name:ident: $dimension:ident, SI = $si:ident $(, compound = $compound:path)?;
        $( $(#[$unit_doc:meta])* $unit:ident = $factor:expr, $symbol:literal $(| $alias:literal)*; )+
    ) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$unit_doc])* $unit, )+
        }

        impl Sealed for $name {}

        impl Unit for $name {
            const DIMENSION: Dimension = Dimension::$dimension;
            const SI: Self = Self::$si;
            const ALL: &'static [Self] = &[$(Self::$unit),+];

            fn si_factor(self) -> f64 {
                match self { $(Self::$unit => $factor),+ }
            }

            fn symbol(self) -> &'static str {
                match self { $(Self::$unit => $symbol),+ }
            }

            fn aliases(self) -> &'static [&'static str] {
                match self { $(Self::$unit => &[$($alias),*]),+ }
            }

            $(
                fn parse_compound(text: &str) -> Option<Result<f64, ParseQuantityError>> {
                    $compound(text)
                }
            )?
        }
    };
}

unit_kind! {
    /// Units of [`Length`](super::Length).
    LengthUnit: Length, SI = Metre;
    /// Metre (`m`), the SI unit.
    Metre = 1.0, "m";
    /// Centimetre (`cm`).
    Centimetre = 1e-2, "cm";
    /// Millimetre (`mm`).
    Millimetre = 1e-3, "mm";
}

unit_kind! {
    /// Units of [`Area`](super::Area).
    AreaUnit: Area, SI = SquareMetre;
    /// Square metre (`m²`), the SI unit.
    SquareMetre = 1.0, "m²" | "m2" | "m^2";
    /// Square centimetre (`cm²`).
    SquareCentimetre = 1e-4, "cm²" | "cm2" | "cm^2";
    /// Square millimetre (`mm²`).
    SquareMillimetre = 1e-6, "mm²" | "mm2" | "mm^2";
}

unit_kind! {
    /// Units of [`Volume`](super::Volume).
    VolumeUnit: Volume, SI = CubicMetre;
    /// Cubic metre (`m³`), the SI unit.
    CubicMetre = 1.0, "m³" | "m3" | "m^3";
    /// Litre (`L`).
    Litre = 1e-3, "L" | "l";
    /// Cubic centimetre (`cm³`).
    CubicCentimetre = 1e-6, "cm³" | "cm3" | "cm^3";
    /// Cubic millimetre (`mm³`).
    CubicMillimetre = 1e-9, "mm³" | "mm3" | "mm^3";
}

unit_kind! {
    /// Units of [`Angle`](super::Angle).
    AngleUnit: Angle, SI = Radian, compound = dms::parse_si;
    /// Radian (`rad`), the SI unit.
    Radian = 1.0, "rad";
    /// Degree (`°`).
    Degree = PI / 180.0, "°" | "deg";
}

unit_kind! {
    /// Units of [`Force`](super::Force).
    ForceUnit: Force, SI = Newton;
    /// Newton (`N`), the SI unit.
    Newton = 1.0, "N";
    /// Kilonewton (`kN`).
    Kilonewton = 1e3, "kN";
    /// Meganewton (`MN`).
    Meganewton = 1e6, "MN";
}

unit_kind! {
    /// Units of [`Moment`](super::Moment).
    MomentUnit: Moment, SI = NewtonMetre;
    /// Newton metre (`N·m`), the SI unit.
    NewtonMetre = 1.0, "N·m" | "N.m" | "N*m" | "Nm";
    /// Kilonewton metre (`kN·m`).
    KilonewtonMetre = 1e3, "kN·m" | "kN.m" | "kN*m" | "kNm";
}

unit_kind! {
    /// Units of [`Stress`](super::Stress).
    StressUnit: Stress, SI = Pascal;
    /// Pascal (`Pa`), the SI unit.
    Pascal = 1.0, "Pa";
    /// Kilopascal (`kPa`).
    Kilopascal = 1e3, "kPa";
    /// Megapascal (`MPa`), equal to one newton per square millimetre.
    Megapascal = 1e6, "MPa" | "N/mm²" | "N/mm2";
    /// Gigapascal (`GPa`).
    Gigapascal = 1e9, "GPa";
}

unit_kind! {
    /// Units of [`Mass`](super::Mass).
    MassUnit: Mass, SI = Kilogram;
    /// Kilogram (`kg`), the SI unit.
    Kilogram = 1.0, "kg";
    /// Gram (`g`).
    Gram = 1e-3, "g";
    /// Tonne (`t`), 1000 kg.
    Tonne = 1e3, "t";
}

/// Instantiates the generic function `$f` for every unit kind, as an array of function
/// pointers. Adding a dimension here is enough for parsing and the tests to cover it; also add
/// it to `KINDS` in `fuzz/fuzz_targets/parse_quantity.rs`.
macro_rules! each_kind {
    ($f:ident) => {
        [
            $f::<LengthUnit>,
            $f::<AreaUnit>,
            $f::<VolumeUnit>,
            $f::<AngleUnit>,
            $f::<ForceUnit>,
            $f::<MomentUnit>,
            $f::<StressUnit>,
            $f::<MassUnit>,
        ]
    };
}
#[cfg(test)]
pub(super) use each_kind;

/// Finds the unit of `U` written `symbol`, by its symbol or an alias. Matching is exact:
/// `mN` and `MN` differ.
pub(super) fn unit_for<U: Unit>(symbol: &str) -> Option<U> {
    U::ALL
        .iter()
        .copied()
        .find(|unit| unit.symbol() == symbol || unit.aliases().contains(&symbol))
}

/// The dimension of whichever unit is written `symbol`, if any.
pub(super) fn dimension_of(symbol: &str) -> Option<Dimension> {
    fn lookup<U: Unit>(symbol: &str) -> Option<Dimension> {
        unit_for::<U>(symbol).map(|_| U::DIMENSION)
    }
    each_kind!(lookup)
        .into_iter()
        .find_map(|lookup| lookup(symbol))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::tests::close;
    use crate::units::{Angle, Moment, Quantity, Stress, Volume};

    /// Every spelling finds its own unit, and `dimension_of` (first match wins) finds this
    /// kind, so no spelling is shared with another dimension.
    fn check_kind<U: Unit>() {
        assert_eq!(U::SI.si_factor().to_bits(), 1.0_f64.to_bits());
        for &unit in U::ALL {
            assert!(unit.si_factor() > 0.0);
            for spelling in std::iter::once(unit.symbol()).chain(unit.aliases().iter().copied()) {
                assert_eq!(unit_for::<U>(spelling), Some(unit));
                assert_eq!(dimension_of(spelling), Some(U::DIMENSION));
            }
        }
    }

    #[test]
    fn every_kind_is_consistent() {
        for check in each_kind!(check_kind) {
            check();
        }
    }

    #[test]
    fn unknown_symbols_match_nothing() {
        assert_eq!(unit_for::<LengthUnit>("km"), None);
        assert_eq!(unit_for::<ForceUnit>("mN"), None);
        assert_eq!(dimension_of("furlong"), None);
    }

    #[test]
    fn known_conversions() {
        let cases = [
            (Volume::new(1.0, VolumeUnit::Litre).si(), 1e-3),
            (Volume::new(1e6, VolumeUnit::CubicCentimetre).si(), 1.0),
            (Angle::new(180.0, AngleUnit::Degree).si(), PI),
            (Moment::new(1.0, MomentUnit::KilonewtonMetre).si(), 1e3),
            (Stress::new(1.0, StressUnit::Megapascal).si(), 1e6),
            (Stress::new(210.0, StressUnit::Gigapascal).si(), 210e9),
            (Quantity::new(2.5, MassUnit::Tonne).si(), 2500.0),
        ];
        for (si, expected) in cases {
            assert!(close(si, expected, 1e-12), "{si} vs {expected}");
        }
    }
}
