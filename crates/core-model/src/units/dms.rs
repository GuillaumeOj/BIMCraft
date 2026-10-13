//! Angles in degrees, minutes and seconds, such as `12°30′15.5″`, for surveying, bearings and
//! georeferencing.

use std::fmt;

use super::text::{POWERS_OF_TEN, write_padded};
use super::{Angle, AngleUnit, DisplayFormat, MAX_DECIMALS, ParseQuantityError, Unit};

/// Every integer up to 2⁵³ is exact in `f64`. Above it, counting display steps in `f64` would
/// lose units, so such angles are shown in decimal degrees instead.
const MAX_EXACT_STEPS: f64 = 9_007_199_254_740_992.0;

/// How a project shows angles: as a decimal value in one unit, or in degrees, minutes and
/// seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AngleFormat {
    /// A decimal number in a unit, such as `12.50°` or `0.218 rad`.
    Decimal(DisplayFormat<AngleUnit>),
    /// Degrees, minutes and seconds, such as `12°30′15.50″`.
    Dms {
        /// Digits after the decimal point of the seconds, capped at [`MAX_DECIMALS`].
        second_decimals: u8,
    },
}

/// An angle ready to display in an [`AngleFormat`]; see [`Angle::display_angle`].
#[derive(Clone, Copy, Debug)]
pub struct FormattedAngle {
    angle: Angle,
    format: AngleFormat,
}

impl Angle {
    /// Prepares the angle for display in `format`, e.g. `12.50°` or `12°30′15.50″`.
    #[must_use]
    pub const fn display_angle(self, format: AngleFormat) -> FormattedAngle {
        FormattedAngle {
            angle: self,
            format,
        }
    }
}

impl fmt::Display for FormattedAngle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.format {
            AngleFormat::Decimal(format) => fmt::Display::fmt(&self.angle.display(format), f),
            AngleFormat::Dms { second_decimals } => write_dms(f, self.angle, second_decimals),
        }
    }
}

/// Rounds the whole angle to a count of the smallest displayed step, then splits that count, so
/// rounding carries into minutes and degrees: 0°59′59.99″ with no decimals is `1°00′00″`.
fn write_dms(f: &mut fmt::Formatter<'_>, angle: Angle, decimals: u8) -> fmt::Result {
    let decimals = decimals.min(MAX_DECIMALS);
    let degrees = angle.get(AngleUnit::Degree);
    let scale = POWERS_OF_TEN[usize::from(decimals)];
    let steps_per_degree = 3600.0 * scale;
    let steps = (degrees.abs() * steps_per_degree).round();
    if steps.is_nan() || steps >= MAX_EXACT_STEPS {
        let decimal = DisplayFormat::new(AngleUnit::Degree, decimals);
        return fmt::Display::fmt(&angle.display(decimal), f);
    }
    // Each step below works on integers under 2⁵³, so every product and difference is exact
    // and each floored quotient is the true integer quotient.
    let whole_degrees = (steps / steps_per_degree).floor();
    let rest = steps - whole_degrees * steps_per_degree;
    let minutes = (rest / (60.0 * scale)).floor();
    let rest = rest - minutes * 60.0 * scale;
    // An exact integer over a power of ten, so printing it with `decimals` digits shows the
    // step count exactly.
    let seconds = rest / scale;
    // One leading sign for the whole angle, and none when it rounds to zero.
    let sign = if degrees < 0.0 && steps > 0.0 {
        "-"
    } else {
        ""
    };
    let decimals = usize::from(decimals);
    let width = if decimals > 0 { decimals + 3 } else { 2 };
    write_padded(
        f,
        format_args!("{sign}{whole_degrees}°{minutes:02}′{seconds:0width$.decimals$}″"),
    )
}

/// [`Unit::parse_compound`] for angles: reads `text` as degrees, minutes and seconds and
/// returns the angle in radians (SI), or `None` when [`is_dms`] finds no such form, so plain
/// values such as `90°` or `1.5 rad` keep the usual parsing.
///
/// The parts come in order (degrees, minutes, seconds), each optional but at least one, with
/// one optional leading sign for the whole angle. Only the last part may have decimals.
/// Degrees are marked `°`, `d` or `deg`, minutes `′` or `'`, seconds `″` or `"`.
pub(super) fn parse_si(text: &str) -> Option<Result<f64, ParseQuantityError>> {
    is_dms(text).then(|| parse_parts(text).map(|degrees| degrees * AngleUnit::Degree.si_factor()))
}

/// A minute or second mark, a `°` followed by more digits, or a `d` written after a number (not
/// the one ending `rad`) followed by digits or nothing.
fn is_dms(text: &str) -> bool {
    let digit = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit());
    let after = |i: usize| text[i..].trim_start().chars().next();
    text.char_indices().any(|(i, c)| match c {
        '′' | '\'' | '″' | '"' => true,
        '°' => digit(after(i + c.len_utf8())),
        'd' => {
            let next = after(i + 1);
            digit(text[..i].trim_end().chars().next_back()) && (next.is_none() || digit(next))
        }
        _ => false,
    })
}

fn parse_parts(text: &str) -> Result<f64, ParseQuantityError> {
    let (negative, mut rest) = match text.strip_prefix(['-', '+']) {
        Some(unsigned) => (text.starts_with('-'), unsigned),
        None => (false, text),
    };
    // Degrees, minutes, seconds; an absent part is zero.
    let mut parts = [0.0; 3];
    let mut last = None;
    let mut fractional = false;
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let (number, after) = rest.split_at(digits);
        let (index, after_mark) = split_mark(after.trim_start())?;
        // Parts must increase, and a fraction is only allowed on the last one.
        if fractional || last.is_some_and(|last| index <= last) {
            return Err(ParseQuantityError::InvalidAngle);
        }
        let value: f64 = number
            .parse()
            .map_err(|_| ParseQuantityError::InvalidAngle)?;
        fractional = number.contains('.');
        parts[index] = value;
        last = Some(index);
        rest = after_mark.trim_start();
    }
    let [degrees, minutes, seconds] = parts;
    if minutes >= 60.0 {
        return Err(ParseQuantityError::MinutesOutOfRange);
    }
    if seconds >= 60.0 {
        return Err(ParseQuantityError::SecondsOutOfRange);
    }
    let total = degrees + minutes / 60.0 + seconds / 3600.0;
    Ok(if negative { -total } else { total })
}

/// Splits the mark that starts `text` off, returning which part it ends (0 for degrees, 1 for
/// minutes, 2 for seconds) and the text after it.
fn split_mark(text: &str) -> Result<(usize, &str), ParseQuantityError> {
    // `deg` is also accepted for plain degrees, so it must work here too.
    if let Some(after) = text.strip_prefix("deg") {
        return Ok((0, after));
    }
    let mut chars = text.chars();
    let index = match chars.next() {
        Some('°' | 'd') => 0,
        Some('′' | '\'') => 1,
        Some('″' | '"') => 2,
        _ => return Err(ParseQuantityError::InvalidAngle),
    };
    Ok((index, chars.as_str()))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::units::tests::close;
    use crate::units::{Length, LengthUnit, MAX_TEXT_LEN};

    fn dms(decimals: u8) -> AngleFormat {
        AngleFormat::Dms {
            second_decimals: decimals,
        }
    }

    fn degrees(value: f64) -> Angle {
        Angle::new(value, AngleUnit::Degree)
    }

    fn show(value: f64, decimals: u8) -> String {
        degrees(value).display_angle(dms(decimals)).to_string()
    }

    fn parse(text: &str) -> Result<Angle, ParseQuantityError> {
        Angle::parse(text, AngleUnit::Radian)
    }

    /// The parsed angle, in degrees.
    fn parsed(text: &str) -> f64 {
        parse(text).map_or(f64::NAN, |angle| angle.get(AngleUnit::Degree))
    }

    #[test]
    fn displays_degrees_minutes_and_seconds() {
        let angle = 12.0 + 30.0 / 60.0 + 15.5 / 3600.0;
        assert_eq!(show(angle, 1), "12°30′15.5″");
        assert_eq!(show(angle, 3), "12°30′15.500″");
        assert_eq!(show(angle, 0), "12°30′16″");
        assert_eq!(show(0.0, 2), "0°00′00.00″");
        assert_eq!(show(5.0 + 1.0 / 60.0 + 2.0 / 3600.0, 0), "5°01′02″");
    }

    #[test]
    fn rounding_carries_into_minutes_and_degrees() {
        let just_below_one = 59.0 / 60.0 + 59.99 / 3600.0;
        assert_eq!(show(just_below_one, 0), "1°00′00″");
        assert_eq!(show(just_below_one, 1), "1°00′00.0″");
        assert_eq!(show(just_below_one, 2), "0°59′59.99″");
        assert_eq!(
            show(359.0 + 59.0 / 60.0 + 59.999 / 3600.0, 2),
            "360°00′00.00″"
        );
        assert_eq!(show(10.0 + 59.5 / 3600.0, 0), "10°01′00″");
    }

    #[test]
    fn negative_angles_keep_one_leading_sign() {
        assert_eq!(show(-0.5, 0), "-0°30′00″");
        assert_eq!(show(-12.5, 1), "-12°30′00.0″");
        // Rounds to zero: no sign, as for other quantities.
        assert_eq!(show(-1e-9, 2), "0°00′00.00″");
        assert_eq!(show(-0.0, 0), "0°00′00″");
    }

    #[test]
    fn caps_decimals_and_honours_width() {
        assert_eq!(show(1.0, 200), "1°00′00.000000000″");
        let angle = degrees(1.5).display_angle(dms(0));
        assert_eq!(format!("{angle:>10}|"), "  1°30′00″|");
        assert_eq!(format!("{angle:<10}|"), "1°30′00″  |");
    }

    #[test]
    fn huge_or_non_finite_angles_fall_back_to_decimal_degrees() {
        assert_eq!(show(1e12, 2), "1000000000000.00°");
        assert_eq!(show(f64::NAN, 0), "NaN°");
        assert_eq!(show(f64::INFINITY, 0), "inf°");
    }

    #[test]
    fn decimal_format_matches_display() {
        let format = DisplayFormat::new(AngleUnit::Radian, 3);
        let angle = Angle::from_si(0.218_25);
        assert_eq!(
            angle
                .display_angle(AngleFormat::Decimal(format))
                .to_string(),
            angle.display(format).to_string()
        );
        assert_eq!(
            format!("{:>9}", angle.display_angle(AngleFormat::Decimal(format))),
            "0.218 rad"
        );
    }

    #[test]
    fn parses_full_and_partial_forms() {
        let full = 12.0 + 30.0 / 60.0 + 15.5 / 3600.0;
        for text in [
            "12°30′15.5″",
            "12d30'15.5\"",
            " 12° 30′ 15.5″ ",
            "+12°30′15.5″",
            "12 d 30 ' 15.5 \"",
            "12deg30'15.5\"",
            "12 deg 30′ 15.5″",
        ] {
            assert!(close(parsed(text), full, 1e-12), "{text}");
        }
        let cases = [
            ("12°30′", 12.5),
            ("12°30.5′", 12.0 + 30.5 / 60.0),
            ("30′", 0.5),
            ("36″", 0.01),
            ("12°15″", 12.0 + 15.0 / 3600.0),
            ("12d", 12.0),
            ("12.25d", 12.25),
            ("-0°30′00″", -0.5),
            ("-12°30′", -12.5),
        ];
        for (text, expected) in cases {
            assert!(close(parsed(text), expected, 1e-12), "{text}");
        }
    }

    #[test]
    fn plain_values_keep_the_usual_parsing() {
        assert!(close(parsed("90°"), 90.0, 1e-12));
        assert!(close(parsed("90 deg"), 90.0, 1e-12));
        assert!(close(parsed("90.5 °"), 90.5, 1e-12));
        assert_eq!(parse("1.5 rad"), Ok(Angle::from_si(1.5)));
        assert_eq!(parse("1.5"), Ok(Angle::from_si(1.5)));
        assert_eq!(parse_si("90°"), None);
        assert_eq!(parse_si("90 deg"), None);
        assert_eq!(parse_si("1.5 rad"), None);
    }

    #[test]
    fn other_dimensions_ignore_angle_marks() {
        assert_eq!(
            Length::parse("12'", LengthUnit::Millimetre),
            Err(ParseQuantityError::UnknownUnit)
        );
    }

    #[test]
    fn rejects_malformed_angles() {
        for text in [
            "30′12°",        // wrong order
            "12°30′15″5″",   // repeated part
            "12°-30′",       // inner sign
            "--12°30′",      // double sign
            "12.5°30′",      // fraction before the last part
            "12°30′15.5.5″", // malformed number
            "12°30′ 7",      // trailing number without a mark
            "12°30′x",       // unknown mark
            "°30′",          // missing degrees value
            "12°,5′",        // comma
            "1e2°30′",       // exponent
        ] {
            assert_eq!(parse(text), Err(ParseQuantityError::InvalidAngle), "{text}");
        }
    }

    #[test]
    fn rejects_minutes_and_seconds_from_sixty() {
        assert_eq!(parse("12°60′"), Err(ParseQuantityError::MinutesOutOfRange));
        assert_eq!(parse("90′"), Err(ParseQuantityError::MinutesOutOfRange));
        assert_eq!(
            parse("12°30′60″"),
            Err(ParseQuantityError::SecondsOutOfRange)
        );
        assert_eq!(parse("12°59.99′"), Ok(degrees(12.0 + 59.99 / 60.0)));
        assert!(parse("12°30′59.999″").is_ok());
    }

    #[test]
    fn too_long_text_is_rejected_first() {
        let text = format!("{}°", "1".repeat(MAX_TEXT_LEN));
        assert_eq!(
            parse(&text),
            Err(ParseQuantityError::TooLong { len: text.len() })
        );
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn displayed_dms_parses_back_within_its_precision(
            value in -1e3..1e3_f64,
            decimals in 0..=MAX_DECIMALS,
        ) {
            let text = show(value, decimals);
            let half_step = 0.5 / POWERS_OF_TEN[usize::from(decimals)] / 3600.0;
            let error = (parsed(&text) - value).abs();
            prop_assert!(error <= half_step * (1.0 + 1e-6) + 1e-12, "{text}: {error}");
        }
    }
}
