# 0009. IFC entity classes from the EXPRESS schema

- Status: Accepted
- Requirements: INT-03, INT-04 (also ENG-02, ARCH-01, ARCH-02, REP-05)
- Issues: [#40](https://github.com/GuillaumeOj/BIMCraft/issues/40) (epic [#39](https://github.com/GuillaumeOj/BIMCraft/issues/39))

## Context

IFC 4.3 import and export use "entity classes generated from the official EXPRESS schema" ([specification §11](../../specifications.md#11-interoperability)). The STEP reader ([#41](https://github.com/GuillaumeOj/BIMCraft/issues/41)) yields untyped records: an entity name and a list of parameters. Mapping them to the model (#139), writing them back (#42) and keeping unknown data (#43, INT-03) need typed values that follow the schema: about 900 entities, 440 types, inheritance, attribute redeclarations, selects and enumerations. Writing these by hand would be slow and drift from the standard.

The schema is published by buildingSMART as `IFC4X3_ADD2.exp` (IFC 4.3.2.0, ISO 16739-1:2024), written in EXPRESS (ISO 10303-11). No EXPRESS parser in the Rust ecosystem is maintained, covers the full language, and meets our rules for hostile input and exact versions.

## Decision

1. **Vendor the official schema unchanged** in `crates/interop-ifc/schema/IFC4X3_ADD2.exp`. buildingSMART publishes it under CC BY-ND 4.0: we may redistribute it with attribution but not modify it. `crates/interop-ifc/schema/README.md` records its source, licence and SHA-256, and `.gitattributes` keeps its bytes as they are. Generated code never copies text from the standard (REP-05): its documentation links to the buildingSMART page of each declaration.
2. **A build-time crate, `express`**, in the lowest layer next to `schema-gen`:
   - a lexer and a recursive-descent parser for the whole of EXPRESS that IFC uses, including functions, rules and every statement and expression, into a syntax tree (`express::ast`);
   - a resolver that checks names and builds a model (`express::model`): each entity's explicit attributes are flattened in STEP order, supertypes first, with redeclarations refining types and `DERIVE` redeclarations marking positions written `*`;
   - a Rust emitter (`express::emit_rust`).

   It is a critical crate with property-based tests and a fuzz target (`express_schema`). Input is bounded in size and nesting, and every problem is an error, never a panic.
3. **`interop-ifc/build.rs` generates the code into `OUT_DIR`**, and `interop_ifc::ifc` includes it. The emitted code is:
   - a newtype per defined type, an enum per enumeration (with an `ITEMS` table of STEP spellings), and an enum per select, with one variant per defined type or enumeration it can hold, plus `Instance(u64)` when it can hold entities;
   - for each entity that can be instantiated, a struct with its non-derived attributes and `from_record`; abstract entities are marker types for `Ref<T>`, a typed but unresolved instance reference;
   - `Entity`, with one boxed variant per entity and `from_record`/`from_instance`, which find the decoder by binary search on the STEP name;
   - tests that decode a sample of every entity, defined type, select member and enumeration item.
4. **Decoding only, with a hand-written runtime.** `interop_ifc::ifc::decode` holds what does not depend on the schema: the `Decode` trait, decoders for simple values and aggregates, the attribute reader and the errors. Decoding checks value kinds, `$` and `*`, aggregate sizes, enumeration items and select members. It does not evaluate `WHERE` rules, check `UNIQUE`, or resolve references. Encoding belongs to the STEP writer (#42). Complex instances (several records in one instance) are reported as `ComplexInstance`; the model maps none, so `interop_ifc::preserve` keeps them as read and writes them back on export (#43, INT-03).

## Consequences

- Upgrading IFC means replacing one file and its SHA-256 in the README: the build regenerates everything, and the generated tests check every declaration.
- The generated code is about 50,000 lines. It adds to `interop-ifc`'s build time, but not to the repository, and it changes only when the schema or the emitter does.
- The emitter is tested on a small hand-written schema (`crates/express/tests/fixtures/sample.exp`), whose output, formatted by rustfmt, is a reviewed snapshot that `interop-ifc`'s tests compile against the runtime. The full schema is tested for parsing, resolution and spot checks of known entities.
- Function and rule bodies are parsed but not used yet. They are available if validation (INT-02 reports) ever evaluates `WHERE` rules.
- The emitter does not format its output: `OUT_DIR` is compiled, not reviewed, and the sample snapshot is stored as rustfmt formats it.

## Alternatives considered

- **Commit the generated code.** Faster builds and visible diffs, but tens of thousands of generated lines in review, and a check that they are up to date. Rejected for the `schema-gen` pattern: generate in `build.rs`.
- **An existing EXPRESS parser or generator** (crates for EXPRESS, or IfcOpenShell's Python tooling). None meets the exact-version and hostile-input rules, covers the full language, and generates the types we need without a non-Rust toolchain at build time.
- **Parse declarations only, keep algorithm bodies as raw text.** Smaller, but a second parser would be needed the day rules are evaluated, and "parses the full schema" would only be half true.
- **Untyped access through the STEP records only.** No generation, but every mapping would repeat attribute positions and type checks, and errors would surface late.

## References

- [Specification §11](../../specifications.md#11-interoperability), INT-03, INT-04.
- ISO 10303-11 (EXPRESS), ISO 10303-21 (STEP), ISO 16739-1:2024 (IFC 4.3).
- buildingSMART, IFC 4.3.2.0 Annex A, computer-interpretable listings.
