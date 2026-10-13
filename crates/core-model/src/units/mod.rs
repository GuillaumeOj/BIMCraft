//! Physical quantities stored in SI units, displayed and parsed in project units (CORE-09).
//!
//! A [`Quantity`] stores one `f64` in the SI unit of its dimension (metres, square metres,
//! newtons…). Its type parameter is the unit enum of that dimension, so a [`Length`] and an
//! [`Area`] are different types: adding them does not compile, and only physically meaningful
//! products and quotients are defined (`Length * Length` is an [`Area`], `Force / Area` a
//! [`Stress`]…).
//!
//! ```
//! use core_model::units::{Area, AreaUnit, Length, LengthUnit};
//!
//! let width = Length::new(3.0, LengthUnit::Metre);
//! let depth = Length::new(2500.0, LengthUnit::Millimetre);
//! let floor: Area = width * depth;
//! assert_eq!(floor.get(AreaUnit::SquareMetre), 7.5);
//! ```
//!
//! Mixing dimensions is a compile error:
//!
//! ```compile_fail
//! use core_model::units::{Area, Length};
//!
//! let _ = Length::ZERO + Area::ZERO;
//! ```
//!
//! ```compile_fail
//! use core_model::units::{Force, Length};
//!
//! let _: Length = Force::ZERO;
//! ```
//!
//! Text crosses the boundary in project units: [`Quantity::display`] formats with a
//! [`DisplayFormat`] (the [`ProjectUnits`] hold one per dimension) and [`Quantity::parse`] reads
//! user input, failing with a typed [`ParseQuantityError`] when the unit belongs to another
//! dimension.

mod kinds;
mod project;
mod text;

use std::cmp::Ordering;
use std::fmt;
use std::iter::Sum;
use std::marker::PhantomData;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

pub use kinds::{
    AngleUnit, AreaUnit, ForceUnit, LengthUnit, MassUnit, MomentUnit, StressUnit, VolumeUnit,
};
pub use project::ProjectUnits;
pub use text::{DisplayFormat, Formatted, MAX_DECIMALS, MAX_TEXT_LEN, ParseQuantityError};

/// A length, stored in metres.
pub type Length = Quantity<LengthUnit>;
/// An area, stored in square metres.
pub type Area = Quantity<AreaUnit>;
/// A volume, stored in cubic metres.
pub type Volume = Quantity<VolumeUnit>;
/// A plane angle, stored in radians.
pub type Angle = Quantity<AngleUnit>;
/// A force, stored in newtons.
pub type Force = Quantity<ForceUnit>;
/// A bending moment, stored in newton metres.
pub type Moment = Quantity<MomentUnit>;
/// A stress or pressure, stored in pascals.
pub type Stress = Quantity<StressUnit>;
/// A mass, stored in kilograms.
pub type Mass = Quantity<MassUnit>;

/// The physical dimension a unit measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dimension {
    /// Length.
    Length,
    /// Area.
    Area,
    /// Volume.
    Volume,
    /// Plane angle.
    Angle,
    /// Force.
    Force,
    /// Bending moment.
    Moment,
    /// Stress or pressure.
    Stress,
    /// Mass.
    Mass,
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Length => "length",
            Self::Area => "area",
            Self::Volume => "volume",
            Self::Angle => "angle",
            Self::Force => "force",
            Self::Moment => "moment",
            Self::Stress => "stress",
            Self::Mass => "mass",
        })
    }
}

mod sealed {
    /// Keeps [`Unit`](super::Unit) closed, so unit symbols stay unique across dimensions and
    /// parsing can name the dimension of a misplaced unit.
    pub trait Sealed {}
}

/// A unit of one dimension. Each dimension has its own unit enum, which also tags the
/// [`Quantity`] type of that dimension.
pub trait Unit: sealed::Sealed + Copy + Eq + fmt::Debug + 'static {
    /// The dimension this unit measures.
    const DIMENSION: Dimension;
    /// The SI unit of the dimension, in which quantities are stored.
    const SI: Self;
    /// Every unit of the dimension.
    const ALL: &'static [Self];

    /// How many SI units one of this unit is worth.
    fn si_factor(self) -> f64;

    /// The symbol used for display, such as `mm` or `kN·m`.
    fn symbol(self) -> &'static str;

    /// Other spellings accepted when parsing, such as `kNm` or `m2`.
    fn aliases(self) -> &'static [&'static str];
}

/// A value of the dimension measured by `U`, stored in its SI unit.
pub struct Quantity<U> {
    si: f64,
    unit: PhantomData<U>,
}

impl<U: Unit> Quantity<U> {
    /// Zero in any unit.
    pub const ZERO: Self = Self::from_si(0.0);

    /// Wraps a value already expressed in the SI unit of the dimension.
    #[must_use]
    pub const fn from_si(si: f64) -> Self {
        Self {
            si,
            unit: PhantomData,
        }
    }

    /// Converts `value`, expressed in `unit`, to a quantity.
    #[must_use]
    pub fn new(value: f64, unit: U) -> Self {
        Self::from_si(value * unit.si_factor())
    }

    /// Returns the value in the SI unit of the dimension.
    #[must_use]
    pub const fn si(self) -> f64 {
        self.si
    }

    /// Returns the value expressed in `unit`.
    #[must_use]
    pub fn get(self, unit: U) -> f64 {
        self.si / unit.si_factor()
    }
}

impl<U> Clone for Quantity<U> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<U> Copy for Quantity<U> {}

impl<U> PartialEq for Quantity<U> {
    fn eq(&self, other: &Self) -> bool {
        self.si == other.si
    }
}

impl<U> PartialOrd for Quantity<U> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.si.partial_cmp(&other.si)
    }
}

impl<U: Unit> Default for Quantity<U> {
    fn default() -> Self {
        Self::ZERO
    }
}

impl<U: Unit> fmt::Debug for Quantity<U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Shows the SI value at full precision, e.g. `0.25 m`. Use [`Quantity::display`] for project
/// units.
impl<U: Unit> fmt::Display for Quantity<U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        text::write_with_symbol(f, self.si, U::SI)
    }
}

impl<U: Unit> Add for Quantity<U> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::from_si(self.si + rhs.si)
    }
}

impl<U: Unit> Sub for Quantity<U> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::from_si(self.si - rhs.si)
    }
}

impl<U: Unit> AddAssign for Quantity<U> {
    fn add_assign(&mut self, rhs: Self) {
        self.si += rhs.si;
    }
}

impl<U: Unit> SubAssign for Quantity<U> {
    fn sub_assign(&mut self, rhs: Self) {
        self.si -= rhs.si;
    }
}

impl<U: Unit> Neg for Quantity<U> {
    type Output = Self;

    fn neg(self) -> Self {
        Self::from_si(-self.si)
    }
}

impl<U: Unit> Mul<f64> for Quantity<U> {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self {
        Self::from_si(self.si * rhs)
    }
}

impl<U: Unit> Mul<Quantity<U>> for f64 {
    type Output = Quantity<U>;

    fn mul(self, rhs: Quantity<U>) -> Quantity<U> {
        rhs * self
    }
}

impl<U: Unit> Div<f64> for Quantity<U> {
    type Output = Self;

    fn div(self, rhs: f64) -> Self {
        Self::from_si(self.si / rhs)
    }
}

/// The ratio of two quantities of the same dimension is a plain number.
impl<U: Unit> Div for Quantity<U> {
    type Output = f64;

    fn div(self, rhs: Self) -> f64 {
        self.si / rhs.si
    }
}

impl<U: Unit> Sum for Quantity<U> {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self::from_si(iter.map(Self::si).sum())
    }
}

/// Defines one operator between two dimensions. SI is coherent, so the SI values combine
/// directly.
macro_rules! derived {
    ($lhs:ty, $trait:ident :: $method:ident, $rhs:ty => $out:ty, $op:tt) => {
        impl $trait<$rhs> for $lhs {
            type Output = $out;

            fn $method(self, rhs: $rhs) -> $out {
                Quantity::from_si(self.si $op rhs.si)
            }
        }
    };
}

/// Defines `a * b = c` in both orders and the two quotients `c / a = b` and `c / b = a`.
macro_rules! product {
    ($a:ident * $b:ident = $c:ident) => {
        derived!($a, Mul::mul, $b => $c, *);
        derived!($b, Mul::mul, $a => $c, *);
        derived!($c, Div::div, $a => $b, /);
        derived!($c, Div::div, $b => $a, /);
    };
}

// A square has one order and one quotient, so it cannot go through `product!`.
derived!(Length, Mul::mul, Length => Area, *);
derived!(Area, Div::div, Length => Length, /);
product!(Length * Area = Volume);
product!(Force * Length = Moment);
product!(Stress * Area = Force);

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Equal up to `tolerance` relative to the larger magnitude.
    pub(in crate::units) fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance * a.abs().max(b.abs())
    }

    fn metres(value: f64) -> Length {
        Length::new(value, LengthUnit::Metre)
    }

    #[test]
    fn stores_si_and_converts_back() {
        let length = Length::new(1250.0, LengthUnit::Millimetre);
        assert_eq!(length.si().to_bits(), 1.25_f64.to_bits());
        assert_eq!(
            length.get(LengthUnit::Centimetre).to_bits(),
            125.0_f64.to_bits()
        );
        assert_eq!(Length::from_si(2.0), metres(2.0));
        assert_eq!(Length::default(), Length::ZERO);
        assert_eq!(Clone::clone(&length), length);
    }

    #[test]
    fn same_dimension_arithmetic() {
        let mut length = metres(2.0) + metres(0.5) - metres(1.0);
        assert_eq!(length, metres(1.5));
        length += metres(1.0);
        length -= metres(0.5);
        assert_eq!(length, metres(2.0));
        assert_eq!(-length, metres(-2.0));
        assert_eq!(length * 3.0, metres(6.0));
        assert_eq!(3.0 * length, metres(6.0));
        assert_eq!(length / 4.0, metres(0.5));
        assert_eq!((metres(3.0) / metres(1.5)).to_bits(), 2.0_f64.to_bits());
        assert_eq!(
            [metres(1.0), metres(2.5)].into_iter().sum::<Length>(),
            metres(3.5)
        );
    }

    #[test]
    fn compares_by_value() {
        assert!(metres(1.0) < Length::new(1001.0, LengthUnit::Millimetre));
        assert!(metres(2.0) > metres(1.0));
        assert_eq!(Length::from_si(f64::NAN).partial_cmp(&metres(1.0)), None);
    }

    #[test]
    fn derived_dimensions() {
        let area = Area::from_si(6.0);
        let volume = Volume::from_si(24.0);
        let force = Force::from_si(12.0);
        let stress = Stress::from_si(4.0);
        let moment = Moment::from_si(36.0);

        assert_eq!(metres(2.0) * metres(3.0), area);
        assert_eq!(metres(4.0) * area, volume);
        assert_eq!(area * metres(4.0), volume);
        assert_eq!(area / metres(3.0), metres(2.0));
        assert_eq!(volume / metres(4.0), area);
        assert_eq!(volume / area, metres(4.0));
        assert_eq!(force * metres(3.0), moment);
        assert_eq!(metres(3.0) * force, moment);
        assert_eq!(moment / metres(3.0), force);
        assert_eq!(moment / force, metres(3.0));
        assert_eq!(Force::from_si(24.0) / area, stress);
        assert_eq!(Force::from_si(24.0) / stress, area);
        assert_eq!(stress * area, Force::from_si(24.0));
        assert_eq!(area * stress, Force::from_si(24.0));
    }

    #[test]
    fn debug_and_display_show_si() {
        assert_eq!(
            Length::new(250.0, LengthUnit::Millimetre).to_string(),
            "0.25 m"
        );
        assert_eq!(format!("{:?}", Area::from_si(2.0)), "2 m²");
        assert_eq!(Angle::from_si(1.5).to_string(), "1.5 rad");
    }

    #[test]
    fn dimensions_have_names() {
        let names = [
            (Dimension::Length, "length"),
            (Dimension::Area, "area"),
            (Dimension::Volume, "volume"),
            (Dimension::Angle, "angle"),
            (Dimension::Force, "force"),
            (Dimension::Moment, "moment"),
            (Dimension::Stress, "stress"),
            (Dimension::Mass, "mass"),
        ];
        for (dimension, name) in names {
            assert_eq!(dimension.to_string(), name);
        }
    }
}
