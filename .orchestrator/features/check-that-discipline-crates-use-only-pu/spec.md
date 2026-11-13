# Discipline crates use only the public extension interfaces (ARCH-03, #6)

Parent epic: #2. Requirement: ARCH-03 — *discipline crates MUST depend only on the public extension interfaces of the core, the same ones offered to plugins.* Spec §8: each discipline registers its entities, tools and rules through the same extension interfaces as third-party plugins.

No `disc-*` crate exists yet; the check must be in place before the first one lands.

## Problem

`scripts/check-architecture.ts` already enforces layering (ARCH-04): `disc-*` (layer 3) may depend on anything in layers 0–2 (`schema`, `core-model`, `core-txn`, `geom`, `persist`, `material`, `georef`, `interop-*`, `analysis`, `docgen`…). That is far wider than what a plugin can reach: a discipline could use `core_model::Store`, `persist`, the IFC writer or the undo history directly, and plugins could never do the same. Nothing names the extension surface today.

## Decision (recommended): one façade crate, checked at manifest level

1. **New crate `core-ext`** — "the public extension interfaces of the core". It contains no logic: it re-exports, by name, the items a discipline or plugin may use, grouped in topic modules (for instance `core_ext::schema`, `core_ext::model`, `core_ext::command`, `core_ext::geometry`, `core_ext::material`, `core_ext::units`). Its rustdoc is the identified surface (acceptance criterion 1).
   - Selection principle: what a third-party plugin could legitimately touch — entity definitions and their registration, IDs, units and quantities, sourced values, model commands and transactions (`ModelCommand`, `Transaction`, `Origin`…), geometry value types, materials and build-ups, georeference read types, and the `platform` traits.
   - Excluded: storage internals (`Store`, attribute tables), history and replay (`Document`, `replay`), `persist`, `interop-*`, `analysis`, `docgen`, `api`, `render`, hosts. The exact list is settled in the implementation task and recorded in the ADR; growing it later is a reviewed edit of one file.
   - Re-exports name items explicitly: no glob (`pub use x::*`) and no whole-crate (`pub use core_model;`) re-export, otherwise the façade leaks everything.
   - Layer: a new layer line between the current layer 2 (`georef material …`) and the discipline layer, holding only `core-ext`. It is a core crate (`wasm-crates.txt`, `#![forbid(...)]` line) and a critical crate (`critical-crates.txt`).
   - A test in `crates/core-ext/tests/` names every re-exported item, so removing one from the surface fails a test and is a deliberate change.

2. **Check, in `scripts/check-architecture.ts`** (already run by `scripts/check-all.sh repo`, hence CI):
   - For every crate whose package name starts with `disc-`, every local (path) dependency, in every section (normal, dev, build, target-specific), resolved through `[workspace.dependencies]` and renames as `localDependencies` already does, must be in the allowed list, read from `scripts/extension-surface.txt` (initially: `core-ext`, `platform`). Anything else fails with `disc-x depends on core-model, which is not a public extension interface; use core-ext (ARCH-03)`.
   - A `disc-*` crate may not depend on another `disc-*` crate (a plugin cannot link a discipline either).
   - Rust only lets a crate import what its manifest declares, so a manifest check covers every import; no fragile source parsing of `use` paths.
   - Every crate named in `scripts/extension-surface.txt` must exist in `crates/` (keeps the list honest).
   - The `core-ext` crate root must not contain a glob or whole-crate `pub use` (regex on the source, comments stripped).
   - Registry dependencies (serde, thiserror…) are out of scope: ARCH-03 is about the core; `cargo deny` covers the rest.

3. **Docs**: ADR `docs/dev/adr/0022-discipline-extension-surface.md` (context, decision, alternatives), indexed in the ADR README; an ARCH-03 paragraph in `docs/dev/quality.md` next to the ARCH-04 one; a `core-ext` row in the crate table of `docs/specifications.md` §4. User docs: N/A (tooling), stated in the PR.

### Alternative considered: allowlist of crates and modules, no façade

`scripts/extension-surface.txt` would list allowed crates and, per crate, allowed modules or items; the check would scan `disc-*` sources for `crate::path` uses. No new crate, but it needs a Rust path parser in TypeScript (braced groups, `as` aliases, `self`/`super`, macros, re-exports such as `material` re-exporting `core_model::Source`) that is easy to fool, and the surface would not be the thing a plugin host binds to. A crate-level-only allowlist (`schema`, `core-model`, `core-txn`, `geom`, `material`, `platform`) is trivial but exposes `Store` and everything else public in those crates, which does not meet "the same ones offered to plugins".

## Acceptance criteria

- [ ] The public extension surface is identified: `core-ext` and its documented re-exports, plus `platform`, listed in `scripts/extension-surface.txt` and explained in ADR 0022.
- [ ] `bun scripts/check-architecture.ts` fails when a `disc-*` crate depends (any section, any rename) on a core crate other than those listed, or on another `disc-*` crate; unit tests in `scripts/check-architecture.test.ts` cover each case and the passing case, with fixture repos as the existing tests do.
- [ ] It runs in `scripts/check-all.sh repo`, hence CI; no new CI wiring is needed beyond what `repo` runs.
- [ ] `core-ext` builds for `wasm32-unknown-unknown`, passes clippy with `-D warnings`, `cargo deny`, and the 98 % critical-crate coverage (or the report states why a re-export-only crate cannot be measured and how it is handled).
- [ ] `scripts/check-all.sh repo` and `CHECK_CRATES="core-ext" scripts/check-all.sh rust` pass with zero warnings.
- [ ] Commits: Conventional Commits, signed off, `ARCH-03` and `Refs #6`; PR says "User docs: N/A (tooling)".

## Out of scope

- Creating any `disc-*` crate.
- How disciplines register commands with `api::Registry` (above their layer): settled when the first discipline lands; `core-ext` will grow then.
- The Python plugin SDK and `plugin-host`.

## Team and tasks

Backend only; no UI, UX or frontend work.

1. **back** (opus): `core-ext` crate, `scripts/extension-surface.txt`, the ARCH-03 check and its tests in `check-architecture.ts`, layer/wasm/critical lists, ADR 0022, `quality.md` and spec table updates.
