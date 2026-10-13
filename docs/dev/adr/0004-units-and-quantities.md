# 0004. Units and quantities

- Status: Accepted
- Requirements: CORE-09, CORE-04, ANA-11, UI-04
- Issues: [#22](https://github.com/GuillaumeOj/BIMCraft/issues/22)

## Context

CORE-09 requires units to be explicit everywhere, SI for storage and project settings for display. Lengths, areas, forces and stresses flow between the model, the geometry kernel, the structural solvers and the UI; a bare `f64` lets a millimetre value reach code expecting metres, or a force be added to a length, without any error. Every later crate that stores or exchanges a physical value builds on this choice, so it is fixed before the property sets (CORE-04) and the analysis model use it.

The core crates must build for `wasm32-unknown-unknown` (ARCH-01) and new dependencies need a justification (AGENTS.md §4).

## Decision

- `core_model::units` provides `Quantity<U>`: one `f64` holding the value in the **SI unit** of its dimension. The type parameter is the dimension's **unit enum** (`LengthUnit`, `AreaUnit`…), so each dimension is a distinct type: `Length = Quantity<LengthUnit>`, `Area`, `Volume`, `Angle`, `Force`, `Moment`, `Stress`, `Mass`.
- Mixing dimensions is a **compile error**. Same-dimension addition, subtraction, scaling and ratios are defined for all quantities; products and quotients across dimensions are defined only for physically meaningful pairs (`Length * Length = Area`, `Force / Area = Stress`, `Force * Length = Moment`…). Further pairs are added when a feature needs them.
- Conversions happen only at the text boundary. `Quantity::new(value, unit)` and `Quantity::get(unit)` convert; `Quantity::display(DisplayFormat)` formats in a unit with a fixed number of decimals; `Quantity::parse(text, default_unit)` reads user input, using the project unit when the text has no symbol.
- Parse failures are typed (`ParseQuantityError`), including `WrongDimension` when the symbol belongs to another dimension (`"5 kN"` read as a length). Input is bounded to 64 bytes, and error messages never repeat it (SEC-03).
- `ProjectUnits` holds one `DisplayFormat` per dimension, with metric building defaults (millimetres, square and cubic metres, kilonewtons, megapascals).
- The `Unit` trait is sealed: symbols stay unique across dimensions, which parsing relies on.
- Only metric units exist for now. Imperial units (ANA-11, "Later") will be new variants of the same enums. Locale-specific number formatting (decimal comma on display, UI-04) belongs to the UI; parsing already accepts `.` or `,` as decimal separator.

## Consequences

- Units mistakes inside the model become compile errors, at no run-time cost.
- Code that needs a dimension not covered here (linear loads, densities, temperatures…) adds a unit enum and its products first.
- The geometry kernel keeps raw `f64` metres for speed and exact predicates (ADR 0001); conversions to `Length` happen at its API boundary.
- Persistence of `ProjectUnits` waits for the project file format.

## Alternatives considered

- **`uom` crate**: complete and type-safe, but a large generic dependency with heavy compile times and error messages, and its many units are more than the model needs. Our handful of dimensions is cheap to maintain.
- **Generic dimension exponents (const generics)**: any product type-checks automatically, but stable Rust cannot yet compute exponent sums in types without nightly features or heavy trait machinery.
- **Bare `f64` with a naming convention**: no protection, which is what CORE-09 exists to prevent.

## References

- Specification §5 (CORE-09), §9 (ANA-11).
- BIPM, *The International System of Units (SI)*, 9th edition, for unit symbols and spacing.
