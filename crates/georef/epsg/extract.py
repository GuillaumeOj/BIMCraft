"""Extract the EPSG definitions of the MVP CRS set from PROJ's proj.db (spec D5, #82).

Run from the repository root (needs network access once, to fetch pyproj, which bundles PROJ and
its proj.db, the EPSG dataset included):

    uv run --no-project --python 3.12 --with pyproj==3.7.1 \
        python crates/georef/epsg/extract.py > crates/georef/epsg/epsg.tsv

The output is the data file `crates/georef` embeds with `include_str!`. Its header records the
pyproj, PROJ and EPSG dataset versions, the dataset date, the SHA-256 of the proj.db read and the
command above. The script fails with a one-line error, and writes nothing, if a code is missing,
if a dependency (datum, ellipsoid, base CRS) is deprecated, if a unit or method is not one it knows, or if proj.db does not have the expected
schema.

Format: one record per line, fields separated by tabs, `#` lines are comments. Lengths are in
metres, angles in decimal degrees, Helmert rotations in arc-seconds and scale differences in parts
per million, whatever unit EPSG stores them in. Codes are EPSG codes.

    ellipsoid      <code> <name> <semi-major axis> <inverse flattening, inf for a sphere>
    prime_meridian <code> <name> <longitude from Greenwich>
    datum          <code> <name> <ellipsoid> <prime meridian>
    geographic     <code> <name> <datum> <south> <north> <west> <east> <area>
    projected      <code> <name> <status> <base geographic CRS> <method> <axes> <south> <north> <west> <east> <area> <parameter>=<value>...
    helmert        <code> <name> <source CRS> <target CRS> <method> <accuracy or -> <tx> <ty> <tz> <rx> <ry> <rz> <ds>

`<status>` is `-` for a current CRS. EPSG deprecates some codes of the set (such as 25838); those
are kept, so files that name them still read, with `deprecated` or `deprecated:<replacement code>`.
`<axes>` is the axis orientations of the projected CRS, such as `east,north`; its unit is always
the metre (checked). `<parameter>` is the EPSG parameter code, such as 8821 for the latitude of
false origin. The four numbers after the axes or datum are the bounding box of the area of use, in
degrees, and `<area>` is its EPSG name, such as "France - onshore and offshore, mainland and
Corsica". `helmert`
lists every non-deprecated EPSG Helmert transformation from a base geographic CRS of the set to
WGS 84 (EPSG:4326), with the method code (9603 geocentric translations: the rotations and scale
are 0; 9606, 1033, 1037 position vector; 9607, 1032, 1038 coordinate frame).
"""

import hashlib
import io
import sqlite3
import sys
from fractions import Fraction
from pathlib import Path

import pyproj
from pyproj.datadir import get_data_dir

COMMAND = (
    "uv run --no-project --python 3.12 --with pyproj==3.7.1 "
    "python crates/georef/epsg/extract.py > crates/georef/epsg/epsg.tsv"
)

WGS84 = 4326

# The MVP set (spec section 7, D5): WGS 84, Lambert-93, CC42-CC50, UTM on ETRS89 and on WGS 84.
PROJECTED = [
    2154,
    *range(3942, 3951),
    *range(25828, 25839),
    *range(32601, 32661),
    *range(32701, 32761),
]
GEOGRAPHIC = [WGS84]

# Projection methods the georef crate implements.
METHODS = {
    9807: "Transverse Mercator",
    9801: "Lambert Conic Conformal (1SP)",
    9802: "Lambert Conic Conformal (2SP)",
}

# Helmert methods, by EPSG code.
HELMERT_METHODS = {9603, 9606, 9607, 1032, 1033, 1037, 1038}

# EPSG unit codes handled specially.
SEXAGESIMAL_DMS = 9110
ARC_SECOND = 9104
PARTS_PER_MILLION = 9202
METRE = 9001

TABLES = {
    "metadata": ["key", "value"],
    "unit_of_measure": ["auth_name", "code", "type", "conv_factor"],
    "ellipsoid": ["auth_name", "code", "name", "semi_major_axis", "uom_auth_name", "uom_code", "inv_flattening", "semi_minor_axis", "deprecated"],
    "prime_meridian": ["auth_name", "code", "name", "longitude", "uom_auth_name", "uom_code", "deprecated"],
    "geodetic_datum": ["auth_name", "code", "name", "ellipsoid_auth_name", "ellipsoid_code", "prime_meridian_auth_name", "prime_meridian_code", "deprecated"],
    "geodetic_crs": ["auth_name", "code", "name", "type", "datum_auth_name", "datum_code", "deprecated"],
    "projected_crs": ["auth_name", "code", "name", "coordinate_system_auth_name", "coordinate_system_code", "geodetic_crs_auth_name", "geodetic_crs_code", "conversion_auth_name", "conversion_code", "deprecated"],
    "conversion": ["auth_name", "code", "method_auth_name", "method_code", "deprecated"]
    + [f"param{i}_{column}" for i in range(1, 8) for column in ("auth_name", "code", "value", "uom_auth_name", "uom_code")],
    "axis": ["coordinate_system_auth_name", "coordinate_system_code", "coordinate_system_order", "orientation", "uom_auth_name", "uom_code"],
    "helmert_transformation": ["auth_name", "code", "name", "method_auth_name", "method_code", "source_crs_auth_name", "source_crs_code", "target_crs_auth_name", "target_crs_code", "accuracy", "tx", "ty", "tz", "translation_uom_auth_name", "translation_uom_code", "rx", "ry", "rz", "rotation_uom_auth_name", "rotation_uom_code", "scale_difference", "scale_difference_uom_auth_name", "scale_difference_uom_code", "deprecated"],
    "usage": ["object_table_name", "object_auth_name", "object_code", "extent_auth_name", "extent_code"],
    "extent": ["auth_name", "code", "name", "south_lat", "north_lat", "west_lon", "east_lon"],
    "supersession": ["superseded_table_name", "superseded_auth_name", "superseded_code", "replacement_table_name", "replacement_auth_name", "replacement_code"],
}


class ExtractError(Exception):
    pass


def check_schema(db):
    for table, columns in TABLES.items():
        present = {row[1] for row in db.execute(f"PRAGMA table_info({table})")}
        missing = [column for column in columns if column not in present]
        if missing:
            raise ExtractError(f"proj.db: {table} lacks {', '.join(missing)}")


def one(db, sql, *args):
    rows = db.execute(sql, args).fetchall()
    if len(rows) != 1:
        raise ExtractError(f"expected one row, got {len(rows)}: {sql} {args}")
    return rows[0]


def text(value):
    """`value` as a field: Python's shortest round-trip repr for numbers, no tabs in names."""
    if isinstance(value, float):
        return repr(value)
    value = str(value)
    if "\t" in value or "\n" in value:
        raise ExtractError(f"a name holds a tab or a newline: {value!r}")
    return value


def dms_to_degrees(value):
    """EPSG's sexagesimal DMS (unit 9110), DDD.MMSSsss, to decimal degrees, exactly then rounded."""
    sign = -1 if value < 0 else 1
    digits = Fraction(repr(abs(value)))
    degrees = int(digits)
    rest = (digits - degrees) * 100
    minutes = int(rest)
    seconds = (rest - minutes) * 100
    return sign * float(degrees + Fraction(minutes, 60) + seconds / 3600)


class Units:
    def __init__(self, db):
        # proj.db stores codes as text.
        self.rows = {
            int(code): (kind, factor)
            for code, kind, factor in db.execute(
                "SELECT code, type, conv_factor FROM unit_of_measure WHERE auth_name = 'EPSG'"
            )
        }

    def _factor(self, code, kind):
        row = self.rows.get(int(code))
        if row is None or row[0] != kind or row[1] is None:
            raise ExtractError(f"unit EPSG:{code} is not a {kind} unit with a factor")
        return row[1]

    def metres(self, value, code):
        return value if int(code) == METRE else value * self._factor(code, "length")

    def degrees(self, value, code):
        code = int(code)
        if code == SEXAGESIMAL_DMS:
            return dms_to_degrees(value)
        if code in (9102, 9122):
            return value
        return value * self._factor(code, "angle") * 180 / 3.141592653589793

    def arc_seconds(self, value, code):
        if int(code) == ARC_SECOND:
            return value
        return value * self._factor(code, "angle") * 648000 / 3.141592653589793

    def ppm(self, value, code):
        if int(code) == PARTS_PER_MILLION:
            return value
        return value * self._factor(code, "scale") * 1e6

    def parameter(self, value, code):
        """A projection parameter in metres, degrees or as a plain scale factor."""
        kind = self.rows.get(int(code), (None,))[0]
        if kind == "length":
            return self.metres(value, code)
        if kind == "angle":
            return self.degrees(value, code)
        if kind == "scale":
            return value * self._factor(code, "scale")
        raise ExtractError(f"unit EPSG:{code} of a parameter is unknown")


def area(db, table, code):
    """The area of use of `code`: its bounding box in degrees, then its name."""
    rows = db.execute(
        "SELECT e.south_lat, e.north_lat, e.west_lon, e.east_lon, e.name FROM usage u "
        "JOIN extent e ON e.auth_name = u.extent_auth_name AND e.code = u.extent_code "
        "WHERE u.object_table_name = ? AND u.object_auth_name = 'EPSG' AND u.object_code = ? "
        "ORDER BY u.extent_code",
        (table, str(code)),
    ).fetchall()
    if not rows:
        raise ExtractError(f"EPSG:{code} has no area of use")
    return rows[0]


def status(db, table, code, deprecated):
    """`-` for a current object, else `deprecated`, with its EPSG replacement when there is one."""
    if not deprecated:
        return "-"
    rows = db.execute(
        "SELECT replacement_code FROM supersession WHERE superseded_table_name = ? "
        "AND superseded_auth_name = 'EPSG' AND superseded_code = ? "
        "AND replacement_table_name = ? AND replacement_auth_name = 'EPSG' ORDER BY replacement_code",
        (table, str(code), table),
    ).fetchall()
    return f"deprecated:{rows[0][0]}" if rows else "deprecated"


def not_deprecated(row, what):
    if row[-1]:
        raise ExtractError(f"{what} is deprecated")


def extract(db, out):
    units = Units(db)
    lines = {"ellipsoid": {}, "prime_meridian": {}, "datum": {}, "geographic": {}, "projected": {}, "helmert": {}}

    def ellipsoid(code):
        if code in lines["ellipsoid"]:
            return
        name, a, unit, inv_f, b, deprecated = one(
            db,
            "SELECT name, semi_major_axis, uom_code, inv_flattening, semi_minor_axis, deprecated "
            "FROM ellipsoid WHERE auth_name = 'EPSG' AND code = ?",
            code,
        )
        not_deprecated((deprecated,), f"ellipsoid EPSG:{code}")
        a_metres = units.metres(a, unit)
        if inv_f is None:
            inv_f = float("inf") if b == a else a / (a - b)
        lines["ellipsoid"][code] = ["ellipsoid", code, name, a_metres, inv_f]

    def prime_meridian(code):
        if code in lines["prime_meridian"]:
            return
        name, longitude, unit, deprecated = one(
            db,
            "SELECT name, longitude, uom_code, deprecated FROM prime_meridian "
            "WHERE auth_name = 'EPSG' AND code = ?",
            code,
        )
        not_deprecated((deprecated,), f"prime meridian EPSG:{code}")
        lines["prime_meridian"][code] = ["prime_meridian", code, name, units.degrees(longitude, unit)]

    def datum(code):
        if code in lines["datum"]:
            return
        name, ellipsoid_code, meridian_code, deprecated = one(
            db,
            "SELECT name, ellipsoid_code, prime_meridian_code, deprecated FROM geodetic_datum "
            "WHERE auth_name = 'EPSG' AND code = ? AND ellipsoid_auth_name = 'EPSG' "
            "AND prime_meridian_auth_name = 'EPSG'",
            code,
        )
        not_deprecated((deprecated,), f"datum EPSG:{code}")
        ellipsoid(ellipsoid_code)
        prime_meridian(meridian_code)
        lines["datum"][code] = ["datum", code, name, ellipsoid_code, meridian_code]

    def geographic(code):
        code = str(code)
        if code in lines["geographic"]:
            return
        name, kind, datum_code, deprecated = one(
            db,
            "SELECT name, type, datum_code, deprecated FROM geodetic_crs "
            "WHERE auth_name = 'EPSG' AND code = ? AND datum_auth_name = 'EPSG'",
            code,
        )
        not_deprecated((deprecated,), f"geographic CRS EPSG:{code}")
        if kind != "geographic 2D":
            raise ExtractError(f"EPSG:{code} is a {kind} CRS, not geographic 2D")
        datum(datum_code)
        lines["geographic"][code] = ["geographic", code, name, datum_code, *area(db, "geodetic_crs", code)]

    def axes(cs_code):
        rows = db.execute(
            "SELECT orientation, uom_auth_name, uom_code FROM axis "
            "WHERE coordinate_system_auth_name = 'EPSG' AND coordinate_system_code = ? "
            "ORDER BY coordinate_system_order",
            (cs_code,),
        ).fetchall()
        if len(rows) != 2 or any(auth != "EPSG" or int(unit) != METRE for _, auth, unit in rows):
            raise ExtractError(f"coordinate system EPSG:{cs_code} is not two axes in metres")
        return ",".join(orientation.lower() for orientation, _, _ in rows)

    def projected(code):
        code = str(code)
        name, cs_code, base, conversion, deprecated = one(
            db,
            "SELECT name, coordinate_system_code, geodetic_crs_code, conversion_code, deprecated "
            "FROM projected_crs WHERE auth_name = 'EPSG' AND code = ? "
            "AND coordinate_system_auth_name = 'EPSG' AND geodetic_crs_auth_name = 'EPSG' "
            "AND conversion_auth_name = 'EPSG'",
            code,
        )
        crs_status = status(db, "projected_crs", code, deprecated)
        columns = ["method_auth_name", "method_code", "deprecated"] + [
            f"param{i}_{column}" for i in range(1, 8) for column in ("auth_name", "code", "value", "uom_code")
        ]
        row = one(
            db,
            f"SELECT {', '.join(columns)} FROM conversion WHERE auth_name = 'EPSG' AND code = ?",
            conversion,
        )
        method_auth, method, conversion_deprecated = row[:3]
        # A deprecated CRS may keep a deprecated conversion; a current one may not.
        if not deprecated:
            not_deprecated((conversion_deprecated,), f"conversion EPSG:{conversion}")
        if method_auth != "EPSG" or int(method) not in METHODS:
            raise ExtractError(f"EPSG:{code} uses method {method_auth}:{method}, not implemented")
        parameters = []
        for i in range(7):
            auth, parameter, value, unit = row[3 + 4 * i : 7 + 4 * i]
            if parameter is None:
                continue
            if auth != "EPSG":
                raise ExtractError(f"EPSG:{code}: parameter {auth}:{parameter} is not EPSG")
            parameters.append(f"{parameter}={text(float(units.parameter(value, unit)))}")
        geographic(base)
        lines["projected"][code] = [
            "projected", code, name, crs_status, base, method, axes(cs_code), *area(db, "projected_crs", code), *parameters,
        ]

    def helmerts(source):
        rows = db.execute(
            "SELECT code, name, method_auth_name, method_code, accuracy, tx, ty, tz, translation_uom_code, "
            "rx, ry, rz, rotation_uom_code, scale_difference, scale_difference_uom_code "
            "FROM helmert_transformation WHERE auth_name = 'EPSG' AND deprecated = 0 "
            "AND source_crs_auth_name = 'EPSG' AND source_crs_code = ? "
            "AND target_crs_auth_name = 'EPSG' AND target_crs_code = ? ORDER BY code",
            (source, str(WGS84)),
        ).fetchall()
        for code, name, method_auth, method, accuracy, tx, ty, tz, t_unit, rx, ry, rz, r_unit, ds, s_unit in rows:
            if method_auth != "EPSG" or int(method) not in HELMERT_METHODS:
                continue
            translation = [units.metres(v, t_unit) for v in (tx, ty, tz)]
            if int(method) == 9603:
                rotation, scale = [0.0, 0.0, 0.0], 0.0
            else:
                rotation = [units.arc_seconds(v, r_unit) for v in (rx, ry, rz)]
                scale = units.ppm(ds, s_unit)
            lines["helmert"][code] = [
                "helmert", code, name, source, WGS84, method,
                "-" if accuracy is None else float(accuracy), *map(float, translation), *map(float, rotation), float(scale),
            ]

    for code in GEOGRAPHIC:
        geographic(code)
    for code in PROJECTED:
        projected(code)
    for code in sorted(lines["geographic"]):
        if int(code) != WGS84:
            helmerts(code)

    for kind in ("ellipsoid", "prime_meridian", "datum", "geographic", "projected", "helmert"):
        for key in sorted(lines[kind], key=int):
            out.write("\t".join(text(field) for field in lines[kind][key]) + "\n")


def main(out):
    path = Path(get_data_dir()) / "proj.db"
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    check_schema(db)
    metadata = dict(db.execute("SELECT key, value FROM metadata"))
    out.write("# EPSG definitions of the BIMCraft MVP CRS set (spec D5). Do not edit.\n")
    out.write(f"# Generated by crates/georef/epsg/extract.py with:\n#   {COMMAND}\n")
    out.write(
        f"# pyproj {pyproj.__version__}, PROJ {pyproj.proj_version_str}, "
        f"EPSG dataset {metadata.get('EPSG.VERSION')} ({metadata.get('EPSG.DATE')}), "
        f"proj.db SHA-256 {digest}\n"
    )
    out.write("# The EPSG Dataset is owned by IOGP and used under its Terms of Use: https://epsg.org/terms-of-use.html\n")
    extract(db, out)


if __name__ == "__main__":
    # Buffered, so a failure leaves the redirected file empty rather than half written.
    buffer = io.StringIO()
    try:
        main(buffer)
    except ExtractError as error:
        sys.exit(f"extract.py: {error}")
    sys.stdout.write(buffer.getvalue())
