# Library data

The grades shipped with BIMCraft (MAT-02), one JSON file per family, loaded by
`Library::builtin` (format in `crates/material/src/library.rs`).

| File | Grades | Documents (editions used) |
| --- | --- | --- |
| `en1992-1-1.json` | Concrete C12/15 to C50/60, reinforcement B500A, B500B, B500C | EN 1992-1-1:2004, NF EN 1992-1-1/NA:2016, EN 1991-1-1:2002 |
| `en1993-1-1.json` | Structural steel S235, S275, S355, S420, S460 (t ≤ 40 mm; fu of S235, S275, S355 to EN 10025-2, of S420 and S460 to EN 10025-3) | EN 1993-1-1:2005, NF EN 1993-1-1/NA:2007, EN 1991-1-1:2002 |
| `timber.json` | Solid C16 to C35, glulam GL24h, GL28h, GL32h | EN 338:2016, EN 14080:2013, EN 1995-1-1:2004, NF EN 1995-1-1/NA:2010 |
| `en1996-1-1.json` | Clay and concrete block masonry | EN 1996-1-1:2005 |
| `non-structural.json` | Plaster, render, EPS, mineral wool, clay brick, concrete block, softwood cladding | Règles Th-Bât, Fascicule Matériaux (2017) |

## Rules

- **Numbers and clause numbers only, no text** (REP-05): a source is a document with its edition
  and a clause or table number, never a quotation.
- **Every value has a source.** A value that no document gives is not shipped.
- Values are SI: pascals, kg/m³, W/(m·K), J/(kg·K); strains and ratios have no unit.
- Annex-dependent partial factors come twice: the recommended value (`gamma_c`) and the French
  annex (`gamma_c_fr`), each with its own document.
- A grade is known by its key, `<design code>/<name>`, such as `en1992-1-1/C25/30`; grades without
  a design code take `bimcraft/<name>`. Keys never change once shipped.
- The derived values the codes define by formula are checked by the tests in
  `src/library/tests.rs` (ENG-03).

## To verify

- The partial factors of the French annexes follow the national annexes as published; check them
  before a release.
- The structural values of the concrete, steel, masonry and glulam files were reviewed by a
  structural engineer from knowledge of the standards, not against the purchased texts. A final
  check against the editions is still recommended.
- Règles Th-Bât is dated 2017 and was written for RT 2012: confirm it is still the edition to cite.

### How the non-structural values are read

- The densities are the upper limits of the dry density bands of Règles Th-Bât, not point values.
- Concrete block takes the conductivity of its lightweight-aggregate concrete band (1600 to 1800
  kg/m³). It is a generic default: real masonry units are in another fascicule of the rules, which
  was not read.
- Mineral wool is one entry with the conservative conductivity of rock wool (25 to 40 kg/m³, 0.044).
  Glass wool (30 to 40 kg/m³) gives 0.039.
- Softwood cladding reads 500 kg/m³ as the density at 15 % moisture; a dry density of 500 would give
  a conductivity of 0.15.
- These are default values, for use when no declared product value exists. Wood-fibre board has no
  default in the rules, so it is not shipped.
