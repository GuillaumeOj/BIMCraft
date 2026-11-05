//! Any text, read as EPSG registry data, gives a registry or a typed error with a line and a
//! field: parsing never panics, and every CRS of an accepted registry converts a point of its
//! area of use without panicking (ENG-02).
//!
//! The seeds are the embedded data, a minimal registry and records with out-of-range values.

#![no_main]

use georef::{GeographicPoint, Registry};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(registry) = Registry::parse(text) else {
        return;
    };
    for crs in registry.list_projected() {
        let centre = GeographicPoint::new(
            f64::midpoint(crs.area.west, crs.area.east).to_radians(),
            f64::midpoint(crs.area.south, crs.area.north).to_radians(),
            0.0,
        );
        if let Ok(projected) = crs.projection.forward(centre) {
            let _ = crs.projection.inverse(projected);
        }
        let _ = crs.projection.factors(centre);
        let _ = registry.projected(crs.code);
    }
    let _ = registry.list_geographic().count();
});
