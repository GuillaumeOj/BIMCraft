//! The line format of the embedded EPSG data, `epsg/epsg.tsv`: one record per line, fields
//! separated by tabs, `#` lines are comments. `epsg/extract.py` documents the format and the
//! units. Every number is checked against a bound on the way in (ENG-02): a value that is not
//! finite, or outside the range any real definition has, is a typed error and never a panic.

use std::f64::consts::PI;
use std::ops::RangeInclusive;
use std::str::Split;

use super::{AreaOfUse, CrsStatus, DatumShift, NamedEllipsoid, PrimeMeridian};
use crate::{
    Ellipsoid, EpsgCode, Helmert, HelmertParameters, LambertConic1SpParameters,
    LambertConic2SpParameters, RotationConvention, TransverseMercatorParameters,
};

/// Degrees to radians, as everywhere in the crate (spec D2).
const DEG: f64 = PI / 180.0;

/// The longest name accepted, in bytes.
const MAX_NAME_BYTES: usize = 256;

/// The most projection parameters a record may carry: two standard parallels and the origin
/// make six at most.
const MAX_PARAMETERS: usize = 6;

/// Translation bound of a Helmert transformation, in metres (10 km).
const MAX_TRANSLATION: f64 = 10_000.0;

/// Rotation bound of a Helmert transformation, in arc-seconds.
const MAX_ROTATION: f64 = 60.0;

/// Scale difference bound of a Helmert transformation, in parts per million.
const MAX_SCALE_DIFFERENCE: f64 = 1_000.0;

/// Why a line of the EPSG data was rejected: the line and the field, never the value (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: invalid {field}")]
pub struct ParseError {
    /// The 1-based number of the line.
    pub line: usize,
    /// The name of the field that is missing, malformed or out of range.
    pub field: &'static str,
}

/// A record of the data.
pub(super) enum Record {
    Ellipsoid(NamedEllipsoid),
    PrimeMeridian(PrimeMeridian),
    Datum(DatumRecord),
    Geographic(GeographicRecord),
    Projected(ProjectedRecord),
    Helmert(Box<HelmertRecord>),
}

pub(super) struct DatumRecord {
    pub code: EpsgCode,
    pub name: String,
    pub ellipsoid: EpsgCode,
    pub prime_meridian: EpsgCode,
}

pub(super) struct GeographicRecord {
    pub code: EpsgCode,
    pub name: String,
    pub datum: EpsgCode,
    pub area: AreaOfUse,
}

pub(super) struct ProjectedRecord {
    pub code: EpsgCode,
    pub name: String,
    pub status: CrsStatus,
    pub base: EpsgCode,
    pub area: AreaOfUse,
    pub parameters: MethodParameters,
}

pub(super) struct HelmertRecord {
    pub shift: DatumShift,
    pub source: EpsgCode,
    pub target: EpsgCode,
}

/// The parameters of a projection method, in radians and metres.
pub(super) enum MethodParameters {
    TransverseMercator(TransverseMercatorParameters),
    LambertConic1Sp(LambertConic1SpParameters),
    LambertConic2Sp(LambertConic2SpParameters),
}

/// The records of `text`, with their 1-based line numbers.
pub(super) fn records(text: &str) -> Result<Vec<(usize, Record)>, ParseError> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(index, line)| Ok((index + 1, record(index + 1, line)?)))
        .collect()
}

fn record(line: usize, text: &str) -> Result<Record, ParseError> {
    let mut cursor = Cursor {
        line,
        fields: text.split('\t'),
    };
    let record = match cursor.text("record kind")? {
        "ellipsoid" => ellipsoid(&mut cursor)?,
        "prime_meridian" => prime_meridian(&mut cursor)?,
        "datum" => datum(&mut cursor)?,
        "geographic" => geographic(&mut cursor)?,
        "projected" => projected(&mut cursor)?,
        "helmert" => helmert(&mut cursor)?,
        _ => return Err(cursor.error("record kind")),
    };
    cursor.finish()?;
    Ok(record)
}

/// The fields of one line, read in order.
struct Cursor<'a> {
    line: usize,
    fields: Split<'a, char>,
}

impl<'a> Cursor<'a> {
    const fn error(&self, field: &'static str) -> ParseError {
        ParseError {
            line: self.line,
            field,
        }
    }

    fn text(&mut self, field: &'static str) -> Result<&'a str, ParseError> {
        self.fields.next().ok_or_else(|| self.error(field))
    }

    fn code(&mut self, field: &'static str) -> Result<EpsgCode, ParseError> {
        let text = self.text(field)?;
        text.parse()
            .ok()
            .and_then(|code| EpsgCode::new(code).ok())
            .ok_or_else(|| self.error(field))
    }

    fn name(&mut self, field: &'static str) -> Result<String, ParseError> {
        let text = self.text(field)?;
        if text.is_empty() || text.len() > MAX_NAME_BYTES || text.chars().any(char::is_control) {
            return Err(self.error(field));
        }
        Ok(text.to_owned())
    }

    /// A finite number within `range`.
    fn number(
        &mut self,
        field: &'static str,
        range: RangeInclusive<f64>,
    ) -> Result<f64, ParseError> {
        let text = self.text(field)?;
        text.parse()
            .ok()
            .filter(|value| range.contains(value))
            .ok_or_else(|| self.error(field))
    }

    /// The number of the next field, in `range`, or `none` for a missing value.
    fn optional_number(
        &mut self,
        field: &'static str,
        none: &str,
        range: RangeInclusive<f64>,
    ) -> Result<Option<f64>, ParseError> {
        let text = self.text(field)?;
        if text == none {
            return Ok(None);
        }
        text.parse()
            .ok()
            .filter(|value| range.contains(value))
            .map(Some)
            .ok_or_else(|| self.error(field))
    }

    /// No field is left.
    fn finish(mut self) -> Result<(), ParseError> {
        match self.fields.next() {
            None => Ok(()),
            Some(_) => Err(self.error("field count")),
        }
    }
}

fn ellipsoid(cursor: &mut Cursor<'_>) -> Result<Record, ParseError> {
    let code = cursor.code("code")?;
    let name = cursor.name("name")?;
    let semi_major_axis = cursor.number("semi-major axis", 1.0e6..=1.0e8)?;
    // A sphere has no flattening and is written `inf`. The projections' series need 1/f of 50 or
    // more, which every Earth ellipsoid in use has.
    let inverse_flattening = cursor
        .optional_number("inverse flattening", "inf", 50.0..=1.0e6)?
        .unwrap_or(f64::INFINITY);
    let ellipsoid = Ellipsoid::new(semi_major_axis, inverse_flattening)
        .map_err(|_| cursor.error("inverse flattening"))?;
    Ok(Record::Ellipsoid(NamedEllipsoid {
        code,
        name,
        ellipsoid,
    }))
}

fn prime_meridian(cursor: &mut Cursor<'_>) -> Result<Record, ParseError> {
    Ok(Record::PrimeMeridian(PrimeMeridian {
        code: cursor.code("code")?,
        name: cursor.name("name")?,
        longitude: cursor.number("longitude", -180.0..=180.0)? * DEG,
    }))
}

fn datum(cursor: &mut Cursor<'_>) -> Result<Record, ParseError> {
    Ok(Record::Datum(DatumRecord {
        code: cursor.code("code")?,
        name: cursor.name("name")?,
        ellipsoid: cursor.code("ellipsoid")?,
        prime_meridian: cursor.code("prime meridian")?,
    }))
}

fn geographic(cursor: &mut Cursor<'_>) -> Result<Record, ParseError> {
    Ok(Record::Geographic(GeographicRecord {
        code: cursor.code("code")?,
        name: cursor.name("name")?,
        datum: cursor.code("datum")?,
        area: area(cursor)?,
    }))
}

/// The bounding box in degrees, then the name, of an area of use.
fn area(cursor: &mut Cursor<'_>) -> Result<AreaOfUse, ParseError> {
    let south = cursor.number("south", -90.0..=90.0)?;
    let north = cursor.number("north", -90.0..=90.0)?;
    let west = cursor.number("west", -180.0..=180.0)?;
    let east = cursor.number("east", -180.0..=180.0)?;
    if south > north {
        return Err(cursor.error("south"));
    }
    Ok(AreaOfUse {
        name: cursor.name("area name")?,
        south,
        north,
        west,
        east,
    })
}

fn projected(cursor: &mut Cursor<'_>) -> Result<Record, ParseError> {
    let code = cursor.code("code")?;
    let name = cursor.name("name")?;
    let status = status(cursor)?;
    let base = cursor.code("base CRS")?;
    let method = cursor.code("method")?.get();
    // The axis order is east, north: the georeference and the map conversion rely on it.
    if cursor.text("axes")? != "east,north" {
        return Err(cursor.error("axes"));
    }
    let area = area(cursor)?;
    let pairs = parameters(cursor)?;
    let parameters = method_parameters(method, &pairs).map_err(|field| cursor.error(field))?;
    Ok(Record::Projected(ProjectedRecord {
        code,
        name,
        status,
        base,
        area,
        parameters,
    }))
}

fn status(cursor: &mut Cursor<'_>) -> Result<CrsStatus, ParseError> {
    match cursor.text("status")? {
        "-" => Ok(CrsStatus::Current),
        "deprecated" => Ok(CrsStatus::Deprecated { replacement: None }),
        text => text
            .strip_prefix("deprecated:")
            .and_then(|code| code.parse().ok())
            .and_then(|code| EpsgCode::new(code).ok())
            .map(|code| CrsStatus::Deprecated {
                replacement: Some(code),
            })
            .ok_or_else(|| cursor.error("status")),
    }
}

/// The `code=value` pairs left on the line.
fn parameters(cursor: &mut Cursor<'_>) -> Result<Vec<(u32, f64)>, ParseError> {
    let mut pairs = Vec::new();
    while let Some(text) = cursor.fields.next() {
        let pair = text.split_once('=').and_then(|(code, value)| {
            let code = code.parse().ok()?;
            let value = value.parse().ok()?;
            parameter_range(code)?
                .contains(&value)
                .then_some((code, value))
        });
        match pair {
            Some(pair) if pairs.len() < MAX_PARAMETERS => pairs.push(pair),
            _ => return Err(cursor.error("parameters")),
        }
    }
    Ok(pairs)
}

/// The range of the EPSG projection parameter `code`: degrees of latitude or longitude, a scale
/// factor, or a length in metres. `None` for a parameter no supported method uses.
fn parameter_range(code: u32) -> Option<RangeInclusive<f64>> {
    match code {
        // Latitudes of origin and of the standard parallels.
        8801 | 8821 | 8823 | 8824 => Some(-90.0..=90.0),
        // Longitudes of origin.
        8802 | 8822 => Some(-180.0..=180.0),
        // Scale factor.
        8805 => Some(0.0..=10.0),
        // False eastings and northings.
        8806 | 8807 | 8826 | 8827 => Some(-1.0e8..=1.0e8),
        _ => None,
    }
}

/// The parameters of the EPSG method `method` out of `pairs`: the field that is wrong otherwise.
fn method_parameters(method: u32, pairs: &[(u32, f64)]) -> Result<MethodParameters, &'static str> {
    match method {
        9807 => {
            let [latitude, longitude, scale, easting, northing] =
                exactly(pairs, [8801, 8802, 8805, 8806, 8807])?;
            Ok(MethodParameters::TransverseMercator(
                TransverseMercatorParameters {
                    latitude_of_origin: latitude * DEG,
                    longitude_of_origin: longitude * DEG,
                    scale_factor: scale,
                    false_easting: easting,
                    false_northing: northing,
                },
            ))
        }
        9801 => {
            let [latitude, longitude, scale, easting, northing] =
                exactly(pairs, [8801, 8802, 8805, 8806, 8807])?;
            Ok(MethodParameters::LambertConic1Sp(
                LambertConic1SpParameters {
                    latitude_of_origin: latitude * DEG,
                    longitude_of_origin: longitude * DEG,
                    scale_factor: scale,
                    false_easting: easting,
                    false_northing: northing,
                },
            ))
        }
        9802 => {
            let [latitude, longitude, first, second, easting, northing] =
                exactly(pairs, [8821, 8822, 8823, 8824, 8826, 8827])?;
            Ok(MethodParameters::LambertConic2Sp(
                LambertConic2SpParameters {
                    latitude_of_origin: latitude * DEG,
                    longitude_of_origin: longitude * DEG,
                    first_parallel: first * DEG,
                    second_parallel: second * DEG,
                    false_easting: easting,
                    false_northing: northing,
                },
            ))
        }
        _ => Err("method"),
    }
}

/// The values of exactly the parameters `codes`, in that order: none missing, none extra.
fn exactly<const N: usize>(
    pairs: &[(u32, f64)],
    codes: [u32; N],
) -> Result<[f64; N], &'static str> {
    if pairs.len() != N {
        return Err("parameters");
    }
    let mut values = [0.0; N];
    for (value, code) in values.iter_mut().zip(codes) {
        *value = pairs
            .iter()
            .find(|(candidate, _)| *candidate == code)
            .map(|(_, value)| *value)
            .ok_or("parameters")?;
    }
    Ok(values)
}

fn helmert(cursor: &mut Cursor<'_>) -> Result<Record, ParseError> {
    let code = cursor.code("code")?;
    let name = cursor.name("name")?;
    let source = cursor.code("source CRS")?;
    let target = cursor.code("target CRS")?;
    let method = cursor.code("method")?.get();
    let accuracy = cursor.optional_number("accuracy", "-", 0.0..=1.0e6)?;
    let mut translation = [0.0; 3];
    for value in &mut translation {
        *value = cursor.number("translation", -MAX_TRANSLATION..=MAX_TRANSLATION)?;
    }
    let mut rotation = [0.0; 3];
    for value in &mut rotation {
        *value = cursor.number("rotation", -MAX_ROTATION..=MAX_ROTATION)?;
    }
    let scale_difference = cursor.number(
        "scale difference",
        -MAX_SCALE_DIFFERENCE..=MAX_SCALE_DIFFERENCE,
    )?;
    // The methods EPSG publishes as the geocentric translations (9603 and 1031) have no rotation
    // or scale: a record that has one is inconsistent.
    let (convention, translations_only) = match method {
        9603 | 1031 => (RotationConvention::PositionVector, true),
        9606 | 1033 => (RotationConvention::PositionVector, false),
        9607 | 1032 => (RotationConvention::CoordinateFrame, false),
        _ => return Err(cursor.error("method")),
    };
    if translations_only
        && rotation
            .iter()
            .chain([&scale_difference])
            .any(|v| v.abs() > 0.0)
    {
        return Err(cursor.error("rotation"));
    }
    let helmert = Helmert::new(&HelmertParameters {
        translation,
        rotation,
        scale_difference,
        convention,
    })
    .map_err(|_| cursor.error("scale difference"))?;
    Ok(Record::Helmert(Box::new(HelmertRecord {
        shift: DatumShift {
            code,
            name,
            accuracy,
            helmert,
        },
        source,
        target,
    })))
}
