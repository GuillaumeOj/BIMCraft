# Library data

The grades shipped with BIMCraft (MAT-02), one JSON file per family, loaded by
`Library::builtin` (format in `crates/material/src/library.rs`).

| File | Grades | Documents (editions used) |
| --- | --- | --- |
| `en1992-1-1.json` | Concrete C12/15 to C50/60, reinforcement B500A, B500B, B500C | EN 1992-1-1:2004, NF EN 1992-1-1/NA:2016, EN 1991-1-1:2002 |
| `en1993-1-1.json` | Structural steel S235, S275, S355, S420, S460 (t ≤ 40 mm; fu of S235, S275, S355 to EN 10025-2, of S420 and S460 to EN 10025-3) | EN 1993-1-1:2005, NF EN 1993-1-1/NA:2007, EN 1991-1-1:2002 |
| `timber.json` | Solid C16 to C35, glulam GL24h, GL28h, GL32h | EN 338:2016, EN 14080:2013, EN 1995-1-1:2004, NF EN 1995-1-1/NA:2010 |
| `en1996-1-1.json` | Clay and concrete block masonry | EN 1996-1-1:2005 |
| `non-structural.json` | Plaster, render, EPS, mineral wool, wood-fibre board, brick, concrete block, softwood cladding | EN ISO 10456:2007 |

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

The thermal and density values of `non-structural.json` are typical tabulated design values and the
partial factors of the French annexes follow the national annexes as published; both should be
checked against the editions above before a release.
