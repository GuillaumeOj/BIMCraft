//! Formatting quantities in project units and parsing user input.

use std::fmt;

use super::kinds::{dimension_of, unit_for};
use super::{Dimension, Quantity, Unit};

/// Most decimals a [`DisplayFormat`] shows; larger settings are capped. Nine decimals of a
/// millimetre is already below `f64` resolution at building scale.
pub const MAX_DECIMALS: u8 = 9;

/// `10^n` for every allowed number of decimals. Each is exact in `f64`, and a table avoids
/// `powi`, whose precision depends on the platform (ENG-04).
const POWERS_OF_TEN: [f64; MAX_DECIMALS as usize + 1] =
    [1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9];

/// Longest text, in bytes, that [`Quantity::parse`] accepts. Anything longer is not a quantity
/// a person typed, so it is rejected before any work is done.
pub const MAX_TEXT_LEN: usize = 64;

/// How to show one dimension: the unit and the number of decimals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DisplayFormat<U> {
    /// Unit the value is converted to.
    pub unit: U,
    /// Digits after the decimal point, capped at [`MAX_DECIMALS`].
    pub decimals: u8,
}

impl<U: Unit> DisplayFormat<U> {
    /// Shows values in `unit` with `decimals` digits after the decimal point.
    #[must_use]
    pub const fn new(unit: U, decimals: u8) -> Self {
        Self { unit, decimals }
    }
}

/// A quantity ready to display in a [`DisplayFormat`]; see [`Quantity::display`].
#[derive(Clone, Copy, Debug)]
pub struct Formatted<U: Unit> {
    quantity: Quantity<U>,
    format: DisplayFormat<U>,
}

impl<U: Unit> fmt::Display for Formatted<U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let decimals = self.format.decimals.min(MAX_DECIMALS);
        let value = round_half_away_from_zero(self.quantity.get(self.format.unit), decimals);
        let decimals = usize::from(decimals);
        write_with_symbol(f, format_args!("{value:.decimals$}"), self.format.unit)
    }
}

/// Rounds ties away from zero, as drawings and schedules do. Formatting alone rounds ties to
/// even, which would show 2.5 mm as "2 mm" but 3.5 mm as "4 mm".
fn round_half_away_from_zero(value: f64, decimals: u8) -> f64 {
    let scale = POWERS_OF_TEN[usize::from(decimals.min(MAX_DECIMALS))];
    let scaled = value * scale;
    if scaled.is_finite() {
        // A small negative value rounds to -0.0, which would print as "-0.00"; adding +0.0
        // turns it into +0.0.
        scaled.round() / scale + 0.0
    } else {
        value
    }
}

/// Writes `number` followed by the unit symbol, honouring the formatter's width and alignment.
/// The degree sign follows the number directly, every other symbol after a space, as the SI
/// brochure writes them.
pub(super) fn write_with_symbol<U: Unit>(
    f: &mut fmt::Formatter<'_>,
    number: impl fmt::Display,
    unit: U,
) -> fmt::Result {
    let symbol = unit.symbol();
    let separator = if symbol == "°" { "" } else { " " };
    // Padding needs the full text first; schedules rarely pad, so skip the allocation otherwise.
    if f.width().is_some() {
        f.pad(&format!("{number}{separator}{symbol}"))
    } else {
        write!(f, "{number}{separator}{symbol}")
    }
}

/// Why a text is not a quantity of the expected dimension.
///
/// The messages never repeat the input, which may come from a user's model (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseQuantityError {
    /// The text is longer than [`MAX_TEXT_LEN`] bytes.
    #[error("a quantity has at most {max} bytes, got {len}", max = MAX_TEXT_LEN)]
    TooLong {
        /// Number of bytes received.
        len: usize,
    },
    /// The text is empty or only whitespace.
    #[error("no value given")]
    Empty,
    /// The number part is missing or malformed.
    #[error("the value is not a number")]
    InvalidNumber,
    /// The value overflows `f64`, in the given unit or once converted to SI.
    #[error("the value is too large")]
    NotFinite,
    /// The unit symbol is not a known unit of any dimension.
    #[error("unknown unit")]
    UnknownUnit,
    /// The unit is known but measures another dimension, such as `kN` for a length.
    #[error("expected a unit of {expected}, got a unit of {found}")]
    WrongDimension {
        /// The dimension being parsed.
        expected: Dimension,
        /// The dimension of the unit found in the text.
        found: Dimension,
    },
}

impl<U: Unit> Quantity<U> {
    /// Prepares the quantity for display in `format`, e.g. `1250 mm` or `90.00°`.
    #[must_use]
    pub const fn display(self, format: DisplayFormat<U>) -> Formatted<U> {
        Formatted {
            quantity: self,
            format,
        }
    }

    /// Reads a quantity typed by a user: a number, optionally followed by a unit symbol such as
    /// `mm` or `kN·m`. Without a symbol the number is in `default_unit`, normally the project
    /// unit. The decimal separator is `.` and there are no thousands separators: the UI converts
    /// text written in the user's locale before calling this (UI-04).
    ///
    /// # Errors
    ///
    /// Returns a [`ParseQuantityError`] when the text is too long or empty, the number is
    /// malformed or overflows, or the unit is unknown or measures another dimension.
    pub fn parse(text: &str, default_unit: U) -> Result<Self, ParseQuantityError> {
        if text.len() > MAX_TEXT_LEN {
            return Err(ParseQuantityError::TooLong { len: text.len() });
        }
        let text = text.trim();
        if text.is_empty() {
            return Err(ParseQuantityError::Empty);
        }
        // No unit symbol starts with a character a number can contain, so the first other
        // character starts the unit. A comma stays in the number so that "1,5" or "1,000" is
        // rejected as a malformed number rather than read with the wrong magnitude.
        let (number, symbol) = text.split_at(
            text.find(|c: char| !matches!(c, '0'..='9' | '+' | '-' | '.' | ',' | 'e' | 'E'))
                .unwrap_or(text.len()),
        );
        let value: f64 = number
            .parse()
            .map_err(|_| ParseQuantityError::InvalidNumber)?;
        let symbol = symbol.trim_start();
        let unit = if symbol.is_empty() {
            default_unit
        } else {
            unit_for(symbol).ok_or_else(|| match dimension_of(symbol) {
                Some(found) => ParseQuantityError::WrongDimension {
                    expected: U::DIMENSION,
                    found,
                },
                None => ParseQuantityError::UnknownUnit,
            })?
        };
        let quantity = Self::new(value, unit);
        if quantity.si().is_finite() {
            Ok(quantity)
        } else {
            Err(ParseQuantityError::NotFinite)
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::units::kinds::each_kind;
    use crate::units::tests::close;
    use crate::units::{
        Angle, AngleUnit, Area, AreaUnit, Force, ForceUnit, Length, LengthUnit, Mass, MassUnit,
        Moment, MomentUnit, Stress, StressUnit, Volume, VolumeUnit,
    };

    fn mm(decimals: u8) -> DisplayFormat<LengthUnit> {
        DisplayFormat::new(LengthUnit::Millimetre, decimals)
    }

    fn length(text: &str) -> Result<Length, ParseQuantityError> {
        Length::parse(text, LengthUnit::Millimetre)
    }

    #[test]
    fn displays_in_the_chosen_unit_and_precision() {
        let quantity = Length::from_si(1.234_56);
        assert_eq!(quantity.display(mm(0)).to_string(), "1235 mm");
        assert_eq!(quantity.display(mm(2)).to_string(), "1234.56 mm");
        assert_eq!(
            quantity
                .display(DisplayFormat::new(LengthUnit::Metre, 3))
                .to_string(),
            "1.235 m"
        );
        assert_eq!(
            Length::from_si(-0.5).display(mm(1)).to_string(),
            "-500.0 mm"
        );
        assert_eq!(
            Moment::new(12.5, MomentUnit::KilonewtonMetre)
                .display(DisplayFormat::new(MomentUnit::KilonewtonMetre, 1))
                .to_string(),
            "12.5 kN·m"
        );
    }

    #[test]
    fn rounds_ties_away_from_zero() {
        assert_eq!(Length::from_si(0.0025).display(mm(0)).to_string(), "3 mm");
        assert_eq!(Length::from_si(-0.0025).display(mm(0)).to_string(), "-3 mm");
        let metres = DisplayFormat::new(LengthUnit::Metre, 1);
        assert_eq!(Length::from_si(0.25).display(metres).to_string(), "0.3 m");
        // 1e308 mm cannot be scaled by 100 without overflowing; it is shown unrounded.
        let huge = Length::from_si(1e305).display(mm(2)).to_string();
        assert!(huge.ends_with(".00 mm") && !huge.contains("inf"));
    }

    #[test]
    fn honours_width_and_alignment() {
        let quantity = Length::from_si(1.25);
        assert_eq!(format!("{:>9}", quantity.display(mm(0))), "  1250 mm");
        assert_eq!(format!("{quantity:<8}|"), "1.25 m  |");
    }

    #[test]
    fn caps_decimals() {
        assert_eq!(
            Length::from_si(0.001).display(mm(200)).to_string(),
            "1.000000000 mm"
        );
    }

    #[test]
    fn small_negative_values_round_to_plain_zero() {
        assert_eq!(Length::from_si(-1e-6).display(mm(2)).to_string(), "0.00 mm");
        assert_eq!(Length::from_si(-1e-6).display(mm(0)).to_string(), "0 mm");
    }

    #[test]
    fn degrees_follow_the_number_directly() {
        let format = DisplayFormat::new(AngleUnit::Degree, 1);
        assert_eq!(
            Angle::new(45.0, AngleUnit::Degree)
                .display(format)
                .to_string(),
            "45.0°"
        );
    }

    #[test]
    fn parses_numbers_with_and_without_units() {
        assert_eq!(length("1250"), Ok(Length::from_si(1.25)));
        assert_eq!(length(" 2.5 m "), Ok(Length::from_si(2.5)));
        assert_eq!(length("2.5m"), Ok(Length::from_si(2.5)));
        assert_eq!(length("-30 cm"), Ok(Length::from_si(-0.3)));
        assert_eq!(length("+1e3 mm"), Ok(Length::from_si(1.0)));
        assert_eq!(length(".5 m"), Ok(Length::from_si(0.5)));
        assert_eq!(
            Area::parse("12 m2", AreaUnit::SquareMillimetre),
            Ok(Area::from_si(12.0))
        );
        assert_eq!(
            Stress::parse("25 N/mm²", StressUnit::Pascal),
            Ok(Stress::from_si(25e6))
        );
        assert_eq!(
            Moment::parse("3 kNm", MomentUnit::NewtonMetre),
            Ok(Moment::from_si(3e3))
        );
        assert_eq!(
            Volume::parse("1 L", VolumeUnit::CubicMetre),
            Ok(Volume::from_si(1e-3))
        );
        assert_eq!(
            Force::parse("4 kN", ForceUnit::Newton),
            Ok(Force::from_si(4e3))
        );
        assert_eq!(
            Mass::parse("2 t", MassUnit::Kilogram),
            Ok(Mass::from_si(2e3))
        );
        assert_eq!(
            Angle::parse("90°", AngleUnit::Radian),
            Ok(Angle::new(90.0, AngleUnit::Degree))
        );
    }

    #[test]
    fn rejects_bad_input_with_typed_errors() {
        let long = "1".repeat(MAX_TEXT_LEN + 1);
        assert_eq!(
            length(&long),
            Err(ParseQuantityError::TooLong { len: long.len() })
        );
        assert_eq!(length(""), Err(ParseQuantityError::Empty));
        assert_eq!(length("   "), Err(ParseQuantityError::Empty));
        for text in [
            "mm", "1.2.3 mm", "2,5 m", "1,000", "1,000.5", "--1", "e", "inf", "NaN",
        ] {
            assert_eq!(
                length(text),
                Err(ParseQuantityError::InvalidNumber),
                "{text}"
            );
        }
        assert_eq!(length("1e400"), Err(ParseQuantityError::NotFinite));
        assert_eq!(
            Mass::parse("1e306 t", MassUnit::Kilogram),
            Err(ParseQuantityError::NotFinite)
        );
        for text in ["3 km", "3 MM", "3 m m", "3 mm²x"] {
            assert_eq!(length(text), Err(ParseQuantityError::UnknownUnit), "{text}");
        }
        assert_eq!(
            length("5 kN"),
            Err(ParseQuantityError::WrongDimension {
                expected: Dimension::Length,
                found: Dimension::Force,
            })
        );
        assert_eq!(
            Area::parse("5 m", AreaUnit::SquareMetre),
            Err(ParseQuantityError::WrongDimension {
                expected: Dimension::Area,
                found: Dimension::Length,
            })
        );
    }

    #[test]
    fn errors_describe_themselves() {
        let cases = [
            (
                ParseQuantityError::TooLong { len: 70 },
                "a quantity has at most 64 bytes, got 70",
            ),
            (ParseQuantityError::Empty, "no value given"),
            (
                ParseQuantityError::InvalidNumber,
                "the value is not a number",
            ),
            (ParseQuantityError::NotFinite, "the value is too large"),
            (ParseQuantityError::UnknownUnit, "unknown unit"),
            (
                ParseQuantityError::WrongDimension {
                    expected: Dimension::Length,
                    found: Dimension::Force,
                },
                "expected a unit of length, got a unit of force",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    /// Writes `si` in every unit of `U` at full precision and checks it parses back.
    fn full_precision_round_trip<U: Unit>(si: f64) -> Result<(), TestCaseError> {
        let quantity = Quantity::<U>::from_si(si);
        for &unit in U::ALL {
            let value = quantity.get(unit);
            prop_assert!(close(
                Quantity::new(value, unit).si(),
                si,
                4.0 * f64::EPSILON
            ));
            let text = format!("{value} {}", unit.symbol());
            let parsed = Quantity::parse(&text, U::SI);
            prop_assert!(parsed.is_ok_and(|parsed| close(parsed.si(), si, 4.0 * f64::EPSILON)));
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn every_unit_round_trips_at_full_precision(si in -1e9..1e9_f64) {
            for round_trip in each_kind!(full_precision_round_trip) {
                round_trip(si)?;
            }
        }

        #[test]
        fn displayed_text_parses_back_within_its_precision(
            si in -1e3..1e3_f64,
            decimals in 0..=MAX_DECIMALS,
            unit in prop::sample::select(LengthUnit::ALL),
        ) {
            let quantity = Length::from_si(si);
            let text = quantity.display(DisplayFormat::new(unit, decimals)).to_string();
            let parsed = Length::parse(&text, LengthUnit::Metre).map(|parsed| parsed.get(unit));
            let half_step = 0.5 / POWERS_OF_TEN[usize::from(decimals)];
            let within = |parsed: f64| (parsed - quantity.get(unit)).abs() <= half_step * (1.0 + 1e-6);
            prop_assert!(parsed.is_ok_and(within), "{text}");
        }

        #[test]
        fn addition_and_subtraction_cancel(a in -1e6..1e6_f64, b in -1e6..1e6_f64) {
            let (a, b) = (Length::from_si(a), Length::from_si(b));
            prop_assert!(((a + b) - b - a).si().abs() <= 1e-9);
        }

        #[test]
        fn parsing_any_string_never_panics(text in "\\PC{0,80}") {
            let _ = Length::parse(&text, LengthUnit::Millimetre);
        }
    }
}
