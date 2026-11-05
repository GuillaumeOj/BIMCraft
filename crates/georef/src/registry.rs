//! The CRS registry: the EPSG definitions of the MVP set, embedded in the crate (GEOREF-02).
//!
//! The data is `epsg/epsg.tsv`, a subset of the EPSG Geodetic Parameter Dataset extracted from
//! PROJ's database by `epsg/extract.py` (see `epsg/README.md` for the source, the version and the
//! terms). It is embedded with `include_str!` and parsed once, on first use, into
//! [`ProjectedCrs`] and [`GeographicCrs`] records that each carry their ready [`Projection`], so
//! no CRS parameter is written in Rust and the crate reads no file (ARCH-02).
//!
//! A [`Georeference`] names its CRS by [`EpsgCode`]. [`ProjectedCrs::true_north`] and
//! [`ProjectedCrs::factors_at_base_point`] take the georeference and check that it is on that
//! CRS, so a projection can never be applied to a base point of another CRS.
//!
//! Deprecated codes stay in the data, flagged, so a project or an IFC file that names one still
//! opens: [`Registry::projected`] accepts them and [`Registry::list_projected`] leaves them out.

mod parse;

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

pub use parse::ParseError;
use parse::{HelmertRecord, MethodParameters, Record};

use crate::geodesy::wrap_angle;
use crate::{
    Ellipsoid, EpsgCode, Factors, GeodesyError, Georeference, Helmert, LambertConformalConic,
    Projection, TransverseMercator,
};

/// The embedded data.
const DATA: &str = include_str!("../epsg/epsg.tsv");

/// The code of WGS 84, the target of every datum shift in the data.
const WGS84: u32 = 4326;

static EMBEDDED: OnceLock<Result<Registry, ParseError>> = OnceLock::new();

/// Why a CRS could not be looked up. Messages never contain the code (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// The registry has no CRS with this code.
    #[error("unknown CRS")]
    UnknownCrs,
    /// The CRS exists but is not a projected CRS, so it cannot georeference a project.
    #[error("the CRS is not a projected CRS")]
    NotProjected,
}

/// Why the values derived at the base point of a georeference could not be computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BasePointError {
    /// The georeference is on another CRS than the one asked.
    #[error("the georeference is on another CRS")]
    CrsMismatch,
    /// The base point cannot be projected back.
    #[error(transparent)]
    Geodesy(#[from] GeodesyError),
}

/// Whether a CRS is current, or deprecated by EPSG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrsStatus {
    /// The CRS is current.
    Current,
    /// EPSG deprecated the CRS: it stays usable, so that older projects open.
    Deprecated {
        /// The CRS that replaces it, when EPSG names one.
        replacement: Option<EpsgCode>,
    },
}

/// The area where a CRS is meant to be used: a name and a bounding box.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AreaOfUse {
    /// The EPSG name, such as "France - onshore and offshore, mainland and Corsica".
    pub name: String,
    /// The southern limit, in degrees of latitude.
    pub south: f64,
    /// The northern limit, in degrees of latitude.
    pub north: f64,
    /// The western limit, in degrees of longitude. It is greater than `east` for an area that
    /// crosses the antimeridian.
    pub west: f64,
    /// The eastern limit, in degrees of longitude.
    pub east: f64,
}

/// An ellipsoid with its EPSG code and name.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct NamedEllipsoid {
    /// The EPSG code of the ellipsoid.
    pub code: EpsgCode,
    /// The EPSG name, such as "GRS 1980".
    pub name: String,
    /// The ellipsoid.
    pub ellipsoid: Ellipsoid,
}

/// A prime meridian.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PrimeMeridian {
    /// The EPSG code of the prime meridian.
    pub code: EpsgCode,
    /// The EPSG name, such as "Greenwich".
    pub name: String,
    /// The longitude from Greenwich, in radians.
    pub longitude: f64,
}

/// A geodetic datum: the reference frame a geographic CRS is expressed in.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Datum {
    /// The EPSG code of the datum.
    pub code: EpsgCode,
    /// The EPSG name, such as "Reseau Geodesique Francais 1993 v1".
    pub name: String,
    /// The ellipsoid of the datum.
    pub ellipsoid: NamedEllipsoid,
    /// The prime meridian of the datum.
    pub prime_meridian: PrimeMeridian,
}

/// A transformation from the datum of a geographic CRS to WGS 84.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DatumShift {
    /// The EPSG code of the transformation.
    pub code: EpsgCode,
    /// The EPSG name, such as "ETRS89 to WGS 84 (1)".
    pub name: String,
    /// The accuracy EPSG states, in metres.
    pub accuracy: Option<f64>,
    /// The seven-parameter transformation, from the CRS's datum to WGS 84.
    pub helmert: Helmert,
}

/// A geographic CRS: longitude and latitude on a datum.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct GeographicCrs {
    /// The EPSG code of the CRS.
    pub code: EpsgCode,
    /// The EPSG name, such as "RGF93 v1".
    pub name: String,
    /// The datum.
    pub datum: Datum,
    /// The area of use.
    pub area: AreaOfUse,
    /// The most accurate transformation to WGS 84 the data holds, if it holds one.
    pub to_wgs84: Option<DatumShift>,
}

/// A projected CRS, ready to convert: eastings and northings on a geographic CRS.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ProjectedCrs {
    /// The EPSG code of the CRS.
    pub code: EpsgCode,
    /// The EPSG name, such as "RGF93 v1 / Lambert-93".
    pub name: String,
    /// Whether EPSG deprecated the CRS.
    pub status: CrsStatus,
    /// The area of use.
    pub area: AreaOfUse,
    /// The geographic CRS the projection is defined on.
    pub base: Arc<GeographicCrs>,
    /// The projection, on the ellipsoid of the base CRS.
    pub projection: Projection,
}

impl ProjectedCrs {
    /// The meridian convergence and point scale factor at the base point of `georeference`.
    ///
    /// # Errors
    ///
    /// [`BasePointError::CrsMismatch`] when `georeference` is on another CRS, and
    /// [`BasePointError::Geodesy`] when its base point is outside the projection's domain.
    pub fn factors_at_base_point(
        &self,
        georeference: &Georeference,
    ) -> Result<Factors, BasePointError> {
        if georeference.crs() != self.code {
            return Err(BasePointError::CrsMismatch);
        }
        let base = self.projection.inverse(georeference.base_point())?;
        Ok(self.projection.factors(base)?)
    }

    /// The direction of true north at the base point of `georeference`, in radians,
    /// counter-clockwise from the local y-axis, in [−π, π]: γ − θ, with γ the meridian
    /// convergence there and θ the grid rotation (spec D2).
    ///
    /// # Errors
    ///
    /// As [`ProjectedCrs::factors_at_base_point`].
    pub fn true_north(&self, georeference: &Georeference) -> Result<f64, BasePointError> {
        let factors = self.factors_at_base_point(georeference)?;
        Ok(wrap_angle(
            factors.convergence - georeference.grid_rotation(),
        ))
    }
}

/// The CRS definitions, by EPSG code.
#[derive(Debug)]
pub struct Registry {
    geographic: BTreeMap<EpsgCode, Arc<GeographicCrs>>,
    projected: BTreeMap<EpsgCode, ProjectedCrs>,
}

impl Registry {
    /// The embedded registry, parsed on first use and kept.
    ///
    /// # Errors
    ///
    /// [`ParseError`] if the embedded data is invalid, which the crate's tests rule out.
    pub fn embedded() -> Result<&'static Self, ParseError> {
        EMBEDDED
            .get_or_init(|| Self::parse(DATA))
            .as_ref()
            .map_err(|error| *error)
    }

    /// The registry of `text`, in the format of `epsg/epsg.tsv`.
    ///
    /// # Errors
    ///
    /// [`ParseError`], with the line and the field, for a record that is malformed, has a value
    /// outside the bounds of real definitions, repeats a code or refers to a record that is not
    /// there.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut ellipsoids = BTreeMap::new();
        let mut meridians = BTreeMap::new();
        let mut datums = Vec::new();
        let mut geographic = Vec::new();
        let mut projected = Vec::new();
        let mut helmerts = Vec::new();
        for (line, record) in parse::records(text)? {
            match record {
                Record::Ellipsoid(record) => unique(&mut ellipsoids, record.code, record, line)?,
                Record::PrimeMeridian(record) => {
                    unique(&mut meridians, record.code, record, line)?;
                }
                Record::Datum(record) => datums.push((line, record)),
                Record::Geographic(record) => geographic.push((line, record)),
                Record::Projected(record) => projected.push((line, record)),
                Record::Helmert(record) => helmerts.push((line, *record)),
            }
        }

        let mut datum_map = BTreeMap::new();
        for (line, record) in datums {
            let prime_meridian = lookup(&meridians, record.prime_meridian, line, "prime meridian")?;
            // The projections and the datum shifts work on longitudes from Greenwich: a datum on
            // another meridian would be silently shifted by its longitude.
            if prime_meridian.longitude.abs() > 0.0 {
                return Err(ParseError {
                    line,
                    field: "prime meridian",
                });
            }
            let datum = Datum {
                code: record.code,
                name: record.name,
                ellipsoid: lookup(&ellipsoids, record.ellipsoid, line, "ellipsoid")?,
                prime_meridian,
            };
            unique(&mut datum_map, record.code, datum, line)?;
        }

        let sources: BTreeSet<EpsgCode> =
            geographic.iter().map(|(_, record)| record.code).collect();
        let shifts = best_shifts(helmerts, &sources)?;
        let mut geographic_map = BTreeMap::new();
        for (line, record) in geographic {
            let crs = GeographicCrs {
                code: record.code,
                name: record.name,
                datum: lookup(&datum_map, record.datum, line, "datum")?,
                area: record.area,
                to_wgs84: shifts.get(&record.code).map(|(_, shift)| shift.clone()),
            };
            unique(&mut geographic_map, record.code, Arc::new(crs), line)?;
        }

        let mut projected_map = BTreeMap::new();
        for (line, record) in projected {
            if geographic_map.contains_key(&record.code) {
                return Err(ParseError {
                    line,
                    field: "code",
                });
            }
            let base = lookup(&geographic_map, record.base, line, "base CRS")?;
            let projection = projection(&base.datum.ellipsoid.ellipsoid, &record.parameters)
                .map_err(|_| ParseError {
                    line,
                    field: "parameters",
                })?;
            let crs = ProjectedCrs {
                code: record.code,
                name: record.name,
                status: record.status,
                area: record.area,
                base,
                projection,
            };
            unique(&mut projected_map, record.code, crs, line)?;
        }
        Ok(Self {
            geographic: geographic_map,
            projected: projected_map,
        })
    }

    /// The projected CRS with the code `code`, deprecated or not.
    ///
    /// # Errors
    ///
    /// [`RegistryError::NotProjected`] for a geographic CRS, and [`RegistryError::UnknownCrs`]
    /// for a code the registry does not have.
    pub fn projected(&self, code: EpsgCode) -> Result<&ProjectedCrs, RegistryError> {
        self.projected.get(&code).ok_or_else(|| {
            if self.geographic.contains_key(&code) {
                RegistryError::NotProjected
            } else {
                RegistryError::UnknownCrs
            }
        })
    }

    /// The current projected CRSs, by EPSG code. Deprecated ones are left out.
    pub fn list_projected(&self) -> impl Iterator<Item = &ProjectedCrs> {
        self.projected
            .values()
            .filter(|crs| crs.status == CrsStatus::Current)
    }

    /// The geographic CRSs, by EPSG code.
    pub fn list_geographic(&self) -> impl Iterator<Item = &GeographicCrs> {
        self.geographic.values().map(|crs| &**crs)
    }
}

/// Adds `value` under `code`, unless the code is taken: a repeated code is an error on `line`.
fn unique<V>(
    map: &mut BTreeMap<EpsgCode, V>,
    code: EpsgCode,
    value: V,
    line: usize,
) -> Result<(), ParseError> {
    match map.entry(code) {
        Entry::Occupied(_) => Err(ParseError {
            line,
            field: "code",
        }),
        Entry::Vacant(entry) => {
            entry.insert(value);
            Ok(())
        }
    }
}

/// A copy of the record `code`, or an error naming `field` on `line` if there is none.
fn lookup<V: Clone>(
    map: &BTreeMap<EpsgCode, V>,
    code: EpsgCode,
    line: usize,
    field: &'static str,
) -> Result<V, ParseError> {
    map.get(&code).cloned().ok_or(ParseError { line, field })
}

/// The most accurate transformation to WGS 84 of each source CRS, with its line. Accuracy ties
/// go to the lower code; an unstated accuracy is the least accurate. Every transformation, the
/// ones not kept included, must start from a geographic CRS of `sources` and end at WGS 84.
fn best_shifts(
    helmerts: Vec<(usize, HelmertRecord)>,
    sources: &BTreeSet<EpsgCode>,
) -> Result<BTreeMap<EpsgCode, (usize, DatumShift)>, ParseError> {
    let mut codes = BTreeSet::new();
    let mut best: BTreeMap<EpsgCode, (usize, DatumShift)> = BTreeMap::new();
    for (line, record) in helmerts {
        let HelmertRecord {
            shift,
            source,
            target,
        } = record;
        if target.get() != WGS84 {
            return Err(ParseError {
                line,
                field: "target CRS",
            });
        }
        if !sources.contains(&source) {
            return Err(ParseError {
                line,
                field: "source CRS",
            });
        }
        if !codes.insert(shift.code) {
            return Err(ParseError {
                line,
                field: "code",
            });
        }
        let accuracy = shift.accuracy.unwrap_or(f64::INFINITY);
        let better = best.get(&source).is_none_or(|(_, held)| {
            let held_accuracy = held.accuracy.unwrap_or(f64::INFINITY);
            accuracy < held_accuracy
                || (accuracy.total_cmp(&held_accuracy).is_eq() && shift.code < held.code)
        });
        if better {
            best.insert(source, (line, shift));
        }
    }
    Ok(best)
}

/// The projection of `parameters` on `ellipsoid`.
fn projection(
    ellipsoid: &Ellipsoid,
    parameters: &MethodParameters,
) -> Result<Projection, GeodesyError> {
    Ok(match parameters {
        MethodParameters::TransverseMercator(parameters) => {
            TransverseMercator::new(ellipsoid, parameters)?.into()
        }
        MethodParameters::LambertConic1Sp(parameters) => {
            LambertConformalConic::one_standard_parallel(ellipsoid, parameters)?.into()
        }
        MethodParameters::LambertConic2Sp(parameters) => {
            LambertConformalConic::two_standard_parallels(ellipsoid, parameters)?.into()
        }
    })
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use proptest::prelude::*;

    use super::*;
    use crate::test_support::DEG;
    use crate::{GeographicPoint, HelmertParameters, ProjectedPoint, RotationConvention};

    const ELLIPSOID: &str = "ellipsoid\t7019\tGRS 1980\t6378137.0\t298.257222101";
    const MERIDIAN: &str = "prime_meridian\t8901\tGreenwich\t0.0";
    const DATUM: &str = "datum\t6171\tRGF93 v1 datum\t7019\t8901";
    const GEOGRAPHIC: &str = "geographic\t4171\tRGF93 v1\t6171\t41.15\t51.56\t-9.86\t10.38\tFrance";
    const HELMERT: &str = "helmert\t1671\tRGF93 v1 to WGS 84 (1)\t4171\t4326\t9603\t1.0\t0.0\t0.0\t0.0\t0.0\t0.0\t0.0\t0.0";
    const LAMBERT93: &str = "projected\t2154\tRGF93 v1 / Lambert-93\t-\t4171\t9802\teast,north\t41.15\t51.56\t-9.86\t10.38\tFrance\t8821=46.5\t8822=3.0\t8823=49.0\t8824=44.0\t8826=700000.0\t8827=6600000.0";
    const BASE: [&str; 6] = [ELLIPSOID, MERIDIAN, DATUM, GEOGRAPHIC, HELMERT, LAMBERT93];

    fn code(code: u32) -> EpsgCode {
        EpsgCode::new(code).unwrap()
    }

    fn document(lines: &[&str]) -> String {
        lines.join("\n") + "\n"
    }

    /// The error of the base document with the line at `index` replaced by `line`.
    fn error_with(index: usize, line: &str) -> ParseError {
        let mut lines = BASE;
        lines[index] = line;
        Registry::parse(&document(&lines)).unwrap_err()
    }

    fn assert_rejected(index: usize, line: &str, field: &str) {
        let error = error_with(index, line);
        assert_eq!(error.line, index + 1, "{line}");
        assert_eq!(error.field, field, "{line}");
    }

    fn lambert93_projection() -> Projection {
        Registry::parse(&document(&BASE))
            .unwrap()
            .projected(code(2154))
            .unwrap()
            .projection
            .clone()
    }

    #[test]
    fn parses_a_registry() {
        let registry = Registry::parse(&document(&BASE)).unwrap();
        let lambert = registry.projected(code(2154)).unwrap();
        assert_eq!(lambert.name, "RGF93 v1 / Lambert-93");
        assert_eq!(lambert.status, CrsStatus::Current);
        assert_eq!(lambert.area.name, "France");
        assert_eq!((lambert.area.south, lambert.area.east), (41.15, 10.38));
        assert_eq!(lambert.base.code, code(4171));
        assert_eq!(lambert.base.datum.name, "RGF93 v1 datum");
        assert_eq!(lambert.base.datum.ellipsoid.name, "GRS 1980");
        assert_eq!(lambert.base.datum.ellipsoid.code, code(7019));
        assert_eq!(lambert.base.datum.prime_meridian.name, "Greenwich");
        assert!(lambert.base.datum.prime_meridian.longitude.abs() < f64::EPSILON);
        let shift = lambert.base.to_wgs84.as_ref().unwrap();
        assert_eq!((shift.code, shift.accuracy), (code(1671), Some(1.0)));
        assert_eq!(
            registry
                .list_geographic()
                .map(|crs| crs.code)
                .collect::<Vec<_>>(),
            [code(4171)]
        );
    }

    #[test]
    fn builds_the_projection_of_each_method() {
        let ellipsoid = Ellipsoid::new(6_378_137.0, 298.257_222_101).unwrap();
        let tm = "projected\t32631\tUTM 31N\t-\t4171\t9807\teast,north\t0.0\t84.0\t0.0\t6.0\tZone\t8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0\t8807=0.0";
        let one = "projected\t3000\tOne\t-\t4171\t9801\teast,north\t0.0\t84.0\t0.0\t6.0\tZone\t8801=18.0\t8802=-77.0\t8805=1.0\t8806=250000.0\t8807=150000.0";
        let registry = Registry::parse(&document(&[
            ELLIPSOID, MERIDIAN, DATUM, GEOGRAPHIC, LAMBERT93, tm, one,
        ]))
        .unwrap();
        let expected = |code_: u32| registry.projected(code(code_)).unwrap().projection.clone();
        let transverse = TransverseMercator::new(
            &ellipsoid,
            &crate::TransverseMercatorParameters {
                latitude_of_origin: 0.0,
                longitude_of_origin: 3.0 * DEG,
                scale_factor: 0.9996,
                false_easting: 500_000.0,
                false_northing: 0.0,
            },
        )
        .unwrap();
        assert_eq!(expected(32631), Projection::from(transverse));
        let lambert = LambertConformalConic::one_standard_parallel(
            &ellipsoid,
            &crate::LambertConic1SpParameters {
                latitude_of_origin: 18.0 * DEG,
                longitude_of_origin: -77.0 * DEG,
                scale_factor: 1.0,
                false_easting: 250_000.0,
                false_northing: 150_000.0,
            },
        )
        .unwrap();
        assert_eq!(expected(3000), Projection::from(lambert));
        assert!(matches!(
            expected(2154),
            Projection::LambertConformalConic(_)
        ));
    }

    #[test]
    fn looks_codes_up_by_kind() {
        let registry = Registry::parse(&document(&BASE)).unwrap();
        assert_eq!(
            registry.projected(code(4171)).unwrap_err(),
            RegistryError::NotProjected
        );
        assert_eq!(
            registry.projected(code(9999)).unwrap_err(),
            RegistryError::UnknownCrs
        );
        assert_eq!(RegistryError::UnknownCrs.to_string(), "unknown CRS");
        assert_eq!(
            RegistryError::NotProjected.to_string(),
            "the CRS is not a projected CRS"
        );
    }

    #[test]
    fn keeps_deprecated_codes_out_of_the_listing_only() {
        let deprecated = LAMBERT93
            .replace("\t-\t", "\tdeprecated:3944\t")
            .replace("2154", "2155");
        let bare = LAMBERT93
            .replace("\t-\t", "\tdeprecated\t")
            .replace("2154", "2156");
        let registry = Registry::parse(&document(&[
            ELLIPSOID,
            MERIDIAN,
            DATUM,
            GEOGRAPHIC,
            LAMBERT93,
            &bare,
            &deprecated,
        ]))
        .unwrap();
        assert_eq!(
            registry.projected(code(2155)).unwrap().status,
            CrsStatus::Deprecated {
                replacement: Some(code(3944))
            }
        );
        assert_eq!(
            registry.projected(code(2156)).unwrap().status,
            CrsStatus::Deprecated { replacement: None }
        );
        let listed: Vec<_> = registry.list_projected().map(|crs| crs.code).collect();
        assert_eq!(listed, [code(2154)]);
    }

    #[test]
    fn lists_projected_crss_by_code() {
        let second = LAMBERT93.replace("2154", "2000");
        let registry = Registry::parse(&document(&[
            ELLIPSOID, MERIDIAN, DATUM, GEOGRAPHIC, LAMBERT93, &second,
        ]))
        .unwrap();
        let listed: Vec<_> = registry
            .list_projected()
            .map(|crs| crs.code.get())
            .collect();
        assert_eq!(listed, [2000, 2154]);
    }

    #[test]
    fn keeps_the_most_accurate_shift() {
        let shift = |code_: u32, accuracy: &str| {
            format!(
                "helmert\t{code_}\tShift\t4171\t4326\t9603\t{accuracy}\t1.0\t2.0\t3.0\t0.0\t0.0\t0.0\t0.0"
            )
        };
        let chosen = |shifts: &[(u32, &str)]| {
            let lines: Vec<String> = shifts.iter().map(|(c, a)| shift(*c, a)).collect();
            let mut all = vec![ELLIPSOID, MERIDIAN, DATUM, GEOGRAPHIC];
            all.extend(lines.iter().map(String::as_str));
            let registry = Registry::parse(&document(&all)).unwrap();
            registry
                .list_geographic()
                .next()
                .unwrap()
                .to_wgs84
                .as_ref()
                .unwrap()
                .code
                .get()
        };
        assert_eq!(chosen(&[(2000, "2.0"), (3000, "1.0")]), 3000);
        assert_eq!(chosen(&[(3000, "1.0"), (2000, "2.0")]), 3000);
        assert_eq!(chosen(&[(3000, "-"), (2000, "2.0")]), 2000);
        assert_eq!(chosen(&[(2000, "2.0"), (3000, "-")]), 2000);
        assert_eq!(chosen(&[(3000, "1.0"), (2000, "1.0")]), 2000);
        assert_eq!(chosen(&[(2000, "1.0"), (3000, "1.0")]), 2000);
        assert_eq!(chosen(&[(3000, "-"), (2000, "-")]), 2000);
    }

    #[test]
    fn reads_the_rotation_convention_of_each_method() {
        for (method, convention) in [
            (9606, RotationConvention::PositionVector),
            (1033, RotationConvention::PositionVector),
            (9607, RotationConvention::CoordinateFrame),
            (1032, RotationConvention::CoordinateFrame),
        ] {
            let line = format!(
                "helmert\t1671\tShift\t4171\t4326\t{method}\t2.5\t446.448\t-125.157\t542.06\t0.15\t0.247\t0.842\t-20.489"
            );
            let registry =
                Registry::parse(&document(&[ELLIPSOID, MERIDIAN, DATUM, GEOGRAPHIC, &line]))
                    .unwrap();
            let shift = registry
                .list_geographic()
                .next()
                .unwrap()
                .to_wgs84
                .clone()
                .unwrap();
            let expected = Helmert::new(&HelmertParameters {
                translation: [446.448, -125.157, 542.06],
                rotation: [0.15, 0.247, 0.842],
                scale_difference: -20.489,
                convention,
            })
            .unwrap();
            assert_eq!(shift.helmert, expected, "{method}");
            assert_eq!(shift.accuracy, Some(2.5));
        }
        let translations = HELMERT.replace("9603", "1031");
        assert!(
            Registry::parse(&document(&[
                ELLIPSOID,
                MERIDIAN,
                DATUM,
                GEOGRAPHIC,
                &translations
            ]))
            .is_ok()
        );
    }

    #[test]
    fn rejects_a_prime_meridian_other_than_greenwich() {
        let paris = MERIDIAN.replace("8901\tGreenwich\t0.0", "8903\tParis\t2.5969213");
        let datum = DATUM.replace("8901", "8903");
        let error = Registry::parse(&document(&[ELLIPSOID, &paris, &datum])).unwrap_err();
        assert_eq!((error.line, error.field), (3, "prime meridian"));
    }

    #[test]
    fn rejects_a_shift_that_is_not_kept_from_an_unknown_source() {
        let unknown = HELMERT
            .replace("1671", "1672")
            .replace("\t4171\t", "\t4999\t");
        let error = Registry::parse(&document(&[
            ELLIPSOID, MERIDIAN, DATUM, GEOGRAPHIC, HELMERT, &unknown,
        ]))
        .unwrap_err();
        assert_eq!((error.line, error.field), (6, "source CRS"));
    }

    #[test]
    fn accepts_a_sphere() {
        let sphere = ELLIPSOID.replace("298.257222101", "inf");
        assert!(Registry::parse(&sphere).is_ok());
    }

    #[test]
    fn rejects_malformed_ellipsoids() {
        let line = |semi_major: &str, inverse_flattening: &str| {
            format!("ellipsoid\t7019\tGRS 1980\t{semi_major}\t{inverse_flattening}")
        };
        assert_rejected(0, "ellipsoid\t0\tGRS 1980\t6378137.0\t298.25", "code");
        assert_rejected(0, "ellipsoid\tx\tGRS 1980\t6378137.0\t298.25", "code");
        assert_rejected(0, "ellipsoid", "code");
        assert_rejected(0, "ellipsoid\t7019\t\t6378137.0\t298.25", "name");
        assert_rejected(0, "ellipsoid\t7019\tbell\u{7}\t6378137.0\t298.25", "name");
        let long = format!("ellipsoid\t7019\t{}\t6378137.0\t298.25", "x".repeat(257));
        assert_rejected(0, &long, "name");
        for semi_major in ["1.0", "1e9", "nan", "inf", "-6378137.0", "abc", ""] {
            assert_rejected(0, &line(semi_major, "298.25"), "semi-major axis");
        }
        for inverse_flattening in ["49.9", "1e7", "nan", "-298.25", "Inf", "abc", ""] {
            assert_rejected(
                0,
                &line("6378137.0", inverse_flattening),
                "inverse flattening",
            );
        }
        assert_rejected(
            0,
            "ellipsoid\t7019\tGRS 1980\t6378137.0",
            "inverse flattening",
        );
        assert_rejected(0, &(line("6378137.0", "298.25") + "\textra"), "field count");
    }

    #[test]
    fn rejects_malformed_records() {
        assert_rejected(0, "bogus\t1", "record kind");
        assert_rejected(0, " ", "record kind");
        assert_rejected(1, "prime_meridian\t8901\tGreenwich\t181.0", "longitude");
        assert_rejected(1, "prime_meridian\t8901\tGreenwich\tnan", "longitude");
        assert_rejected(2, "datum\t6171\tRGF93\t9999\t8901", "ellipsoid");
        assert_rejected(2, "datum\t6171\tRGF93\t7019\t9999", "prime meridian");
        assert_rejected(2, "datum\t6171\tRGF93\t7019", "prime meridian");
        assert_rejected(
            3,
            "geographic\t4171\tRGF93\t9999\t41.15\t51.56\t-9.86\t10.38\tFrance",
            "datum",
        );
        let area = |south: &str, north: &str, west: &str, east: &str| {
            format!("geographic\t4171\tRGF93\t6171\t{south}\t{north}\t{west}\t{east}\tFrance")
        };
        assert_rejected(3, &area("52", "51", "-9", "10"), "south");
        assert_rejected(3, &area("-91", "51", "-9", "10"), "south");
        assert_rejected(3, &area("41", "91", "-9", "10"), "north");
        assert_rejected(3, &area("41", "51", "-181", "10"), "west");
        assert_rejected(3, &area("41", "51", "-9", "181"), "east");
        assert_rejected(
            3,
            "geographic\t4171\tRGF93\t6171\t41\t51\t-9\t10",
            "area name",
        );
    }

    #[test]
    fn crosses_the_antimeridian() {
        let fiji = GEOGRAPHIC.replace("-9.86\t10.38", "177.0\t-178.0");
        let registry = Registry::parse(&document(&[ELLIPSOID, MERIDIAN, DATUM, &fiji])).unwrap();
        let area = &registry.list_geographic().next().unwrap().area;
        assert!(area.west > area.east);
    }

    #[test]
    fn rejects_malformed_helmert_records() {
        let line = |method: &str, accuracy: &str, translation: f64, rotation: f64, scale: f64| {
            format!(
                "helmert\t1671\tShift\t4171\t4326\t{method}\t{accuracy}\t{translation}\t{translation}\t{translation}\t{rotation}\t{rotation}\t{rotation}\t{scale}"
            )
        };
        assert!(
            Registry::parse(&document(&[
                ELLIPSOID,
                MERIDIAN,
                DATUM,
                GEOGRAPHIC,
                &line("9606", "1.0", 1.0, 1.0, 1.0)
            ]))
            .is_ok()
        );
        let cases = [
            (line("9999", "1.0", 1.0, 1.0, 1.0), "method"),
            (line("9606", "-1.0", 1.0, 1.0, 1.0), "accuracy"),
            (line("9606", "x", 1.0, 1.0, 1.0), "accuracy"),
            (line("9606", "1.0", 10_000.5, 1.0, 1.0), "translation"),
            (line("9606", "1.0", f64::NAN, 1.0, 1.0), "translation"),
            (line("9606", "1.0", 1.0, 60.5, 1.0), "rotation"),
            (line("9606", "1.0", 1.0, 1.0, 1000.5), "scale difference"),
            (
                line("9606", "1.0", 1.0, 1.0, f64::INFINITY),
                "scale difference",
            ),
            // A translations-only method with a rotation or a scale is inconsistent.
            (line("9603", "1.0", 1.0, 0.5, 0.0), "rotation"),
            (line("1031", "1.0", 1.0, 0.0, 0.5), "rotation"),
        ];
        for (text, field) in cases {
            assert_rejected(4, &text, field);
        }
        assert_rejected(4, &HELMERT.replace("4326", "4258"), "target CRS");
        assert_rejected(4, &HELMERT.replace("\t4171\t", "\t4999\t"), "source CRS");
        assert_rejected(4, &HELMERT[..HELMERT.len() - 4], "scale difference");
    }

    #[test]
    fn rejects_malformed_projected_records() {
        let tm = |method: &str, parameters: &str| {
            format!(
                "projected\t2154\tUTM\t-\t4171\t{method}\teast,north\t0.0\t84.0\t0.0\t6.0\tZone\t{parameters}"
            )
        };
        let good = "8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0\t8807=0.0";
        assert!(
            Registry::parse(&document(&[
                ELLIPSOID,
                MERIDIAN,
                DATUM,
                GEOGRAPHIC,
                &tm("9807", good)
            ]))
            .is_ok()
        );
        let cases = [
            (tm("9809", good), "method"),
            (tm("9807", ""), "parameters"),
            (
                tm("9807", "8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0"),
                "parameters",
            ),
            (
                tm(
                    "9807",
                    "8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0\t8806=1.0",
                ),
                "parameters",
            ),
            (
                tm(
                    "9807",
                    "8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0\t8999=0.0",
                ),
                "parameters",
            ),
            (
                tm(
                    "9807",
                    "8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0\t8807",
                ),
                "parameters",
            ),
            (
                tm(
                    "9807",
                    "8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0\t8807=x",
                ),
                "parameters",
            ),
            (
                tm(
                    "9807",
                    "8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0\tx=0.0",
                ),
                "parameters",
            ),
        ];
        for (text, field) in cases {
            assert_rejected(4, &text.replace("2154", "2155"), field);
        }
    }

    #[test]
    fn rejects_out_of_range_and_inconsistent_parameters() {
        let tm = |method: &str, parameters: &str| {
            format!(
                "projected\t2155\tUTM\t-\t4171\t{method}\teast,north\t0.0\t84.0\t0.0\t6.0\tZone\t{parameters}"
            )
        };
        let good = "8801=0.0\t8802=3.0\t8805=0.9996\t8806=500000.0\t8807=0.0";
        let cases = [
            (
                tm(
                    "9807",
                    "8801=91.0\t8802=3.0\t8805=0.9996\t8806=500000.0\t8807=0.0",
                ),
                "parameters",
            ),
            (
                tm(
                    "9807",
                    "8801=0.0\t8802=3.0\t8805=11.0\t8806=500000.0\t8807=0.0",
                ),
                "parameters",
            ),
            (
                tm(
                    "9807",
                    "8801=0.0\t8802=3.0\t8805=0.9996\t8806=1e9\t8807=0.0",
                ),
                "parameters",
            ),
            (
                tm("9807", &format!("{good}\t8801=0.0\t8802=3.0\t8805=1.0")),
                "parameters",
            ),
            // The series need an ellipsoid; the constructor rejects a zero scale factor.
            (
                tm(
                    "9807",
                    "8801=0.0\t8802=3.0\t8805=0.0\t8806=500000.0\t8807=0.0",
                ),
                "parameters",
            ),
            (
                tm("9801", "8801=0.0\t8802=3.0\t8805=1.0\t8806=0.0\t8807=0.0"),
                "parameters",
            ),
            (
                tm(
                    "9802",
                    "8821=46.5\t8822=3.0\t8823=30.0\t8824=-30.0\t8826=0.0\t8827=0.0",
                ),
                "parameters",
            ),
            (tm("9802", good), "parameters"),
        ];
        for (text, field) in cases {
            assert_rejected(4, &text.replace("2154", "2155"), field);
        }
        let lines = [
            LAMBERT93.replace("\t-\t", "\tdeprecated:x\t"),
            LAMBERT93.replace("\t-\t", "\tgone\t"),
        ];
        for line in lines {
            assert_rejected(5, &line, "status");
        }
        assert_rejected(5, &LAMBERT93.replace("east,north", "north,east"), "axes");
        assert_rejected(5, &LAMBERT93.replace("\t4171\t", "\t4999\t"), "base CRS");
        assert_rejected(5, &LAMBERT93.replace("\t9802\t", "\t9802\t\t"), "axes");
        assert_rejected(5, &LAMBERT93.replace("2154", "4171"), "code");
    }

    #[test]
    fn rejects_repeated_codes() {
        for (index, line) in [
            (0, ELLIPSOID),
            (1, MERIDIAN),
            (2, DATUM),
            (3, GEOGRAPHIC),
            (4, HELMERT),
            (5, LAMBERT93),
        ] {
            let mut lines = BASE.to_vec();
            lines.insert(index + 1, line);
            let error = Registry::parse(&document(&lines)).unwrap_err();
            assert_eq!((error.line, error.field), (index + 2, "code"), "{line}");
        }
    }

    #[test]
    fn counts_comments_and_blank_lines_in_line_numbers() {
        let text = format!("# comment\n\n{ELLIPSOID}\n# another\nbogus\n");
        let error = Registry::parse(&text).unwrap_err();
        assert_eq!((error.line, error.field), (5, "record kind"));
        assert_eq!(error.to_string(), "line 5: invalid record kind");
    }

    #[test]
    fn parses_the_embedded_data() {
        let registry = Registry::embedded().unwrap();
        assert!(std::ptr::eq(registry, Registry::embedded().unwrap()));
        // The MVP set: Lambert-93, CC42 to CC50, ETRS89 UTM 28N to 38N, WGS 84 UTM north and south.
        assert_eq!(registry.projected.len(), 1 + 9 + 11 + 60 + 60);
        assert_eq!(registry.list_geographic().count(), 3);
        let records = parse::records(DATA).unwrap();
        let datum_shifts = records
            .iter()
            .filter(|(_, record)| matches!(record, Record::Helmert(_)))
            .count();
        assert_eq!(datum_shifts, 2);
        let wgs84 = registry.projected(code(32631)).unwrap();
        assert_eq!(wgs84.base.code.get(), 4326);
        assert!(wgs84.base.to_wgs84.is_none());
        let etrs89 = registry.projected(code(25831)).unwrap();
        assert_eq!(etrs89.base.code.get(), 4258);
        assert_eq!(etrs89.base.to_wgs84.as_ref().unwrap().code.get(), 1149);
    }

    #[test]
    fn lists_current_crss_and_accepts_deprecated_ones() {
        let registry = Registry::embedded().unwrap();
        let deprecated = registry.projected(code(25838)).unwrap();
        assert_eq!(
            deprecated.status,
            CrsStatus::Deprecated { replacement: None }
        );
        assert_eq!(deprecated.name, "ETRS89 / UTM zone 38N");
        let listed: Vec<_> = registry
            .list_projected()
            .map(|crs| crs.code.get())
            .collect();
        assert!(!listed.contains(&25838));
        assert_eq!(listed.len(), registry.projected.len() - 1);
        assert!(listed.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            registry
                .list_projected()
                .all(|crs| crs.status == CrsStatus::Current)
        );
        assert_eq!(
            registry.projected(code(4326)).unwrap_err(),
            RegistryError::NotProjected
        );
        assert_eq!(
            registry.projected(code(27700)).unwrap_err(),
            RegistryError::UnknownCrs
        );
    }

    fn georeference(crs: u32, base: ProjectedPoint, grid_rotation: f64) -> Georeference {
        Georeference::new(code(crs), base, grid_rotation, 1.0, None).unwrap()
    }

    #[test]
    fn derives_true_north_from_the_grid_rotation() {
        let registry = Registry::embedded().unwrap();
        let lambert = registry.projected(code(2154)).unwrap();
        // East of the central meridian, grid north is clockwise of true north, so true north is
        // counter-clockwise of the local y-axis when the grid is not rotated.
        let point = GeographicPoint::new(7.75 * DEG, 48.58 * DEG, 0.0);
        let strasbourg = lambert.projection.forward(point).unwrap();
        let convergence = 0.725_607_765_053_267 * 4.75 * DEG;
        let factors = lambert
            .factors_at_base_point(&georeference(2154, strasbourg, 0.0))
            .unwrap();
        assert!((factors.convergence - convergence).abs() < 1e-12);
        assert!((factors.scale - lambert.projection.factors(point).unwrap().scale).abs() < 1e-15);
        for (rotation, expected) in [
            (0.0, convergence),
            (30.0 * DEG, convergence - 30.0 * DEG),
            (-179.0 * DEG, convergence + 179.0 * DEG - 2.0 * PI),
        ] {
            let north = lambert
                .true_north(&georeference(2154, strasbourg, rotation))
                .unwrap();
            assert!((north - expected).abs() < 1e-12, "{rotation}");
        }
    }

    #[test]
    fn refuses_a_georeference_on_another_crs() {
        let registry = Registry::embedded().unwrap();
        let utm = registry.projected(code(32631)).unwrap();
        let base = ProjectedPoint::new(500_000.0, 5_000_000.0, 0.0);
        assert_eq!(
            utm.true_north(&georeference(2154, base, 0.0)),
            Err(BasePointError::CrsMismatch)
        );
        assert_eq!(
            utm.factors_at_base_point(&georeference(25831, base, 0.0)),
            Err(BasePointError::CrsMismatch)
        );
        assert!(utm.true_north(&georeference(32631, base, 0.0)).is_ok());
        assert_eq!(
            BasePointError::CrsMismatch.to_string(),
            "the georeference is on another CRS"
        );
    }

    #[test]
    fn reports_a_base_point_outside_the_domain() {
        let registry = Registry::embedded().unwrap();
        let utm = registry.projected(code(32631)).unwrap();
        let far = georeference(32631, ProjectedPoint::new(3e7, 0.0, 0.0), 0.0);
        assert_eq!(
            utm.true_north(&far),
            Err(BasePointError::Geodesy(GeodesyError::OutsideDomain))
        );
    }

    #[test]
    fn builds_the_same_projection_from_a_text_and_a_registry() {
        // The registry's Lambert-93 is the one the PROJ comparison checks.
        let lambert = lambert93_projection();
        let origin = lambert
            .forward(GeographicPoint::new(3.0 * DEG, 46.5 * DEG, 0.0))
            .unwrap();
        assert!((origin.eastings - 700_000.0).abs() < 1e-9);
        assert!((origin.northings - 6_600_000.0).abs() < 1e-9);
    }

    /// A number as the data writes it, or the strings a hostile file would use instead.
    fn number() -> impl Strategy<Value = String> {
        prop_oneof![
            any::<f64>().prop_map(|value| value.to_string()),
            (-1e3..1e3_f64).prop_map(|value| value.to_string()),
            Just("NaN".to_owned()),
            Just("inf".to_owned()),
            Just("-1e999".to_owned()),
            Just(String::new()),
        ]
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn never_panics_on_arbitrary_text(text in any::<String>()) {
            let _ = Registry::parse(&text);
        }

        #[test]
        fn never_panics_on_records_with_arbitrary_numbers(
            numbers in proptest::collection::vec(number(), 14),
            index in 0..BASE.len(),
        ) {
            // Every tab-separated field after the kind is replaced in turn by a hostile number.
            let mut lines = BASE.map(str::to_owned);
            let fields: Vec<&str> = BASE[index].split('\t').collect();
            for position in 2..fields.len() {
                let mut mutated = fields.clone();
                mutated[position] = &numbers[position % numbers.len()];
                lines[index] = mutated.join("\t");
                let _ = Registry::parse(&(lines.join("\n") + "\n"));
            }
        }

        #[test]
        fn accepts_what_it_bounds(longitude in -180.0..=180.0_f64, latitude in -89.0..=89.0_f64) {
            let line = format!(
                "geographic\t4171\tRGF93 v1\t6171\t{latitude}\t{latitude}\t{longitude}\t{longitude}\tAnywhere"
            );
            let mut lines = BASE;
            lines[3] = &line;
            prop_assert!(Registry::parse(&document(&lines)).is_ok());
        }
    }
}
