# Record the Fabrilo rename in an ADR and the specification

Issue [#381](https://github.com/GuillaumeOj/BIMCraft/issues/381), first sub-issue of the epic [#380](https://github.com/GuillaumeOj/BIMCraft/issues/380) "Rename BIMCraft to Fabrilo". Requirement IDs: none (product-name decision, `docs/specifications.md` §16.2). Milestone: Phase 1 – MVP.

This feature changes documentation only: no code, no tests, no UI. It sets the identifiers that the other rename sub-issues (#382 to #389) apply. Those sub-issues are **out of scope**: this branch does not rename anything in code, tooling, the user guide or other ADRs.

## 1. Deliverables

| File | Change |
| --- | --- |
| `docs/dev/adr/0025-rename-to-fabrilo.md` | New ADR, from `docs/dev/adr/_template.md`, status Accepted |
| `docs/dev/adr/README.md` | Index row `0025 · Rename to Fabrilo · Accepted` |
| `docs/specifications.md` | Old name replaced (section 3) |

## 2. ADR 0025 content

Header: `- Status: Accepted`, `- Requirements: none (product-name decision, specification §16.2); PERS-01 and UI-04 are affected`, `- Issues: epic #380, #381 to #389` (links).

**Context.** "BIMCraft" is already used by other products. A naming study and a trademark screening chose **Fabrilo** (fa-BREE-lo) on 2026-11-17: Latin *fabrilis*, "of the craftsman", and Esperanto *fabr-ilo*, "making tool". No version has been released: no tag, no release, no project file or IFC export in users' hands. The repository mentions the old name about 310 times in about 135 files.

**Decision**, with the identifier table. It contains every row of the epic's table, plus three rows found in the code and earlier ADRs (marked †):

| Use | Old | New |
| --- | --- | --- |
| Product name | BIMCraft | Fabrilo |
| Lowercase id (crates, packages, paths, thread names) | `bimcraft` | `fabrilo` |
| Environment variables | `BIMCRAFT_*` (e.g. `BIMCRAFT_BLESS`) | `FABRILO_*` |
| Schema and API URNs | `urn:bimcraft:…` | `urn:fabrilo:…` |
| Desktop bundle identifier | `org.bimcraft.desktop` | `org.fabrilo.desktop` |
| IFC property set | `BIMCraft_SurveyPoint` | `Fabrilo_SurveyPoint` |
| † Other IFC property sets and material properties (planned in ADR 0018 and 0019) | `BIMCraft_*` (`BIMCraft_Material*`, `BIMCraft_LibraryValues`) | `Fabrilo_*` |
| Built-in material source | `bimcraft/<name>` | `fabrilo/<name>` |
| † Browser storage keys | `bimcraft.*` (`bimcraft.language`) | `fabrilo.*` (`fabrilo.language`) |
| Project file header (SQLite `application_id`) | `BIMC` (`0x4249_4D43`, 1112100163) | `FABR` (`0x4641_4252`, 1178681938) |

Rules stated in the ADR:

- **Casing.** The product name is written `Fabrilo` in prose and in mixed-case identifiers (`Fabrilo_SurveyPoint`), `fabrilo` in lowercase identifiers, `FABRILO` in upper-case ones. The four-character file header is `FABR`, the ASCII bytes `0x46 0x41 0x42 0x52`; it fits a positive `i32`, as SQLite's `application_id` needs.
- **No compatibility.** Nothing reads or writes the old identifiers after the rename: no alias for `bimcraft/` material keys, no import of `BIMCraft_SurveyPoint`, no acceptance of the `BIMC` header, no migration of the `bimcraft.language` key, no old environment variable. Reason: nothing was released, so no file, export, script or browser holds them. PERS-01 (open files from earlier versions) is not broken: the only `BIMC` files are developers' scratch files. A developer's saved language choice resets to the default once. The project file format version (`user_version`) does not change: the header marks the application, not the layout.
- **Earlier ADRs.** Renaming an identifier in an earlier record is editorial, not a change of decision: #388 updates the name in records 0001 to 0024 in place and they keep their status. This is the one exception to "accepted records are not rewritten". ADR 0025 itself keeps the old name, as the record of the change.
- **Where the old name may remain.** ADR 0025, the §16.2 record in the specification, and the allowlist of the guard sub-issue #389. Links to `github.com/GuillaumeOj/BIMCraft` stay until the owner renames the repository (GitHub redirects them); #388 and #389 handle them.

**Consequences.** One rename sub-issue per area (#382 core and host crates, #383 IFC, #384 schema and API, #385 desktop, #386 web UI, #387 user guide, #388 repository files and dev docs, #389 the guard). They can run in parallel after this ADR merges. Open PRs #370 and #378 adopt the new identifiers or are renamed after they merge. The owner's tasks outside the code (reserving names, the trademark filing, renaming the repository, the project board and the design file) are listed in #380, not in the ADR.

**Alternatives considered.** Keep BIMCraft (already used by others, trademark risk). Read the old identifiers alongside the new ones for one release (no release exists, so it would be dead code to maintain and test). Bump the file format version instead of changing the header (the header identifies the application; a version bump would hide the rename). Supersede every ADR that names an old identifier (24 records superseded for a word: noise, and the decisions themselves do not change).

**References.** #380, #381, specification §16.2, ADR 0006 (SQLite binding, `application_id`), ADR 0016 (project file layout), ADR 0017 (survey point property set), ADR 0018 and 0019 (material property sets).

## 3. Specification changes (`docs/specifications.md`)

Every current mention of BIMCraft (seven lines):

| Line | Now | After |
| --- | --- | --- |
| 1, title | `# BIMCraft — Specifications v0.1` | `# Fabrilo — Specifications v0.1` |
| 7, introduction | "BIMCraft is an open-source…" | "Fabrilo is an open-source…" |
| 396, RVT row | "the BIMCraft native format" | "the Fabrilo native format" |
| 413, INT-10 | "the cardinal points BIMCraft does not support" | "…Fabrilo does not support" |
| 415, RVT position | "the BIMCraft native format" | "the Fabrilo native format" |
| 558, §16.2 product name | "decided: BIMCraft, open source…" | "decided: Fabrilo, open source under the MIT licence. Changed from BIMCraft on 2026-11-17, see [ADR 0025](dev/adr/0025-rename-to-fabrilo.md) for the reasons and the identifiers." |
| 560, §16.2 Revit add-in | "the BIMCraft native format" (twice) | "the Fabrilo native format" |
| 564, §16.2 trademark | "Register the BIMCraft trademark…" | "Register the Fabrilo trademark (word mark, classes 9 and 42) before public launch (deferred in the specialist review)." |

The status line and version stay (`v0.1`, the date of the last content revision). After the change, `grep -n -i bimcraft docs/specifications.md` returns only line 558.

## 4. Acceptance

- ADR 0025 lists every identifier of the epic's table (section 2), the `FABR` value `0x4641_4252`, and the no-compatibility decision naming `bimcraft/` keys, `BIMCraft_SurveyPoint`, `BIMC` and `bimcraft.language`.
- The ADR index lists 0025. The specification mentions BIMCraft only in the §16.2 record.
- `scripts/check-all.sh repo` passes (typos, links, formatting). If `typos` flags `Fabrilo` or `FABR`, add them to `_typos.toml` `[default.extend-words]` with a comment, which is in scope.
- Commits: Conventional Commits, signed off, `Refs #381` (e.g. `docs(dev): record the rename to Fabrilo in ADR 0025 (Refs #381)` and `docs(spec): name the product Fabrilo (Refs #381)`). No `wip` subjects.
- Pull request states "User docs: N/A (specification and ADR only)".

## 5. Tasks

One task, owner `back`: write the ADR, the index row and the specification changes; run `scripts/check-all.sh repo`. Review by `front`. No `ux-brief.md` or `api-contract.md`: there is no design or API work.

## 6. Risks

- Another open PR (#370, #378) could add ADR 0025 first. Before the PR, the lead checks the number is still free on `main` and renumbers if not.
