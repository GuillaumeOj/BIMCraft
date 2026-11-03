# EPSG definitions of the MVP CRS set

`epsg.tsv` holds the coordinate reference systems BIMCraft offers (GEOREF-02): WGS 84, ETRS89 and
RGF93 geographic CRSs, Lambert-93 (2154), the conic zones CC42 to CC50 (3942 to 3950), ETRS89 /
UTM zones 28N to 38N (25828 to 25838) and WGS 84 / UTM zones 1N to 60N and 1S to 60S (32601 to
32660, 32701 to 32760), with their datums, ellipsoids, prime meridian, areas of use and the
Helmert transformations to WGS 84. The `georef` crate embeds it with `include_str!` and parses it
once, on first use (`georef::Registry`).

## Source and attribution

- **Dataset:** EPSG Geodetic Parameter Dataset, © IOGP, version v11.022 (2024-11-05).
  <https://epsg.org>. Terms of use: <https://epsg.org/terms-of-use.html>.
- **Read through:** PROJ's `proj.db`, bundled by pyproj 3.7.1 (PROJ 9.5.1). The SHA-256 of the
  `proj.db` read is `80943a39a4b5d5aab3c59f38cdf429a00db11e8fcfbbfa0b594905e1593e8e4d`; it is also in
  the header of `epsg.tsv`.
- **Reformatting:** the values are converted to SI units (metres, and arc-seconds and parts per
  million for the Helmert parameters) and decimal degrees, and the file is a subset of the
  dataset. The user confirmed on 2026-11-03 that this is acceptable reformatting under the terms
  of use.

## Regenerate

`epsg.tsv` is generated; never edit it by hand. From the repository root (needs network access
once, to fetch pyproj):

```sh
uv run --no-project --python 3.12 --with pyproj==3.7.1 \
    python crates/georef/epsg/extract.py > crates/georef/epsg/epsg.tsv
```

The script writes nothing and exits with a one-line error when a code is missing, a dependency is
deprecated, or a unit or method is unknown. Its docstring documents the line format.

## Add a CRS to the MVP list

1. Add its EPSG code to `PROJECTED` in `extract.py`. The projection method must be Transverse
   Mercator (9807), Lambert Conic Conformal 1SP (9801) or 2SP (9802); another method needs a new
   projection in `georef` first.
2. Run the command above and review the diff of `epsg.tsv`.
3. Update the expected counts in `parses_the_embedded_data` (`src/registry.rs`), and add a
   fixture to `tests/proj/generate.py` if the CRS is a new kind, then regenerate
   `tests/proj/reference.txt`.
4. Run `cargo test -p georef`.

A code EPSG has deprecated stays in the data, flagged: files that name it still open, `get`
accepts it, and the listing leaves it out.
