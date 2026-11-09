## What and why

The material model: material objects in the project, with behaviour laws, partial factors per national annex, and values that keep their source (document and clause, or "entered by the user"). This is the base the library (#86) and the build-ups (#87) are built on. First of three stacked PRs for epic #84.

Closes #85
Refs #84, #244
Requirements: MAT-01, CORE-03, CORE-04, CORE-09, API-02, ENG-02, ENG-03

- Schema: a new `resource` kind, enumerations (`enum:`), and sourced values (`sourced = true`) stored with their source, so material values persist (save, then reopen, is tested in `crates/host-desktop/tests/material_persistence.rs`).
- New crate `material` (layer 3, critical, wasm). It has eight categories and these laws: linear elastic, concrete parabola-rectangle and bi-linear, steel bi-linear, timber elastic-brittle, and none. Each law reports its limit strains and initial modulus. Partial factors are given per annex (recommended, FR). A `MaterialOriginal` keeps the library value of an edited value, so it can be reset.
- `core-model`: refuses to delete an object a required reference still targets (`StillReferenced`), which the API reports as `in_use`.
- ADR 0018 (sourced values) and ADR 0019 (materials and layered build-ups).
- The API type generator handles enumerations and string bounds.

## User documentation

N/A: internal change. Users see materials in the next PR (#86).

## Known limits

- The annex-dependent partial factors are flat attributes (`gamma_c`, `gamma_c_fr`, …): adding another annex is a schema version.
- Enumerations are not checked across schema versions yet.

## Definition of Done (AGENTS.md §2)

- [x] Unit tests added; coverage ≥ 98 % in the critical crates (material 99 %)
- [x] E2E test: N/A (no UI in this PR)
- [x] Bug fixes include a regression test
- [x] Format, lint and type checks pass locally (taplo not run: it panics in the sandbox; CI runs it)
- [x] No new dependency
- [x] End-user docs: N/A, explained above
- [x] Code review and simplification pass done
- [ ] Commits signed off (DCO): six `.orchestrator/` planning and design commits (3b9df09, b6c0f19, 4c1debd, 13ead78, ba0d987, 81fcfca) have no sign-off; see the stack notes

🤖 Generated with [Claude Code](https://claude.com/claude-code)
