# 0028. Rename to Fabrilo

- Status: Accepted
- Requirements: none (product-name decision, specification §16.2); PERS-01 and UI-04 are affected
- Issues: epic [#380](https://github.com/GuillaumeOj/BIMCraft/issues/380), [#381](https://github.com/GuillaumeOj/BIMCraft/issues/381) to [#389](https://github.com/GuillaumeOj/BIMCraft/issues/389)

## Context

"BIMCraft" is already used by other products. A naming study and a trademark screening chose **Fabrilo** (fa-BREE-lo) on 2026-11-17. The name comes from the Latin *fabrilis*, "of the craftsman", and from the Esperanto *fabr-ilo*, "making tool".

No version has been released: there is no tag, no release, and no project file or IFC export in users' hands. The repository mentions the old name about 310 times in about 135 files.

## Decision

The product is renamed to Fabrilo. Each identifier that carries the old name changes as follows.

| Use | Old | New |
| --- | --- | --- |
| Product name | BIMCraft | Fabrilo |
| Lowercase id (crates, packages, paths, thread names) | `bimcraft` | `fabrilo` |
| Environment variables | `BIMCRAFT_*` (for example `BIMCRAFT_BLESS`) | `FABRILO_*` |
| Schema and API URNs | `urn:bimcraft:…` | `urn:fabrilo:…` |
| Desktop bundle identifier | `org.bimcraft.desktop` | `org.fabrilo.desktop` |
| IFC property set | `BIMCraft_SurveyPoint` | `Fabrilo_SurveyPoint` |
| Other IFC property sets and material properties (planned in ADR 0018 and 0019) | `BIMCraft_*` (`BIMCraft_Material*`, `BIMCraft_LibraryValues`) | `Fabrilo_*` |
| Built-in material source | `bimcraft/<name>` | `fabrilo/<name>` |
| Browser storage keys | `bimcraft.*` (`bimcraft.language`) | `fabrilo.*` (`fabrilo.language`) |
| Project file header (SQLite `application_id`) | `BIMC` (`0x4249_4D43`, 1112100163) | `FABR` (`0x4641_4252`, 1178681938) |

**Casing.** The product name is written `Fabrilo` in prose and in mixed-case identifiers (`Fabrilo_SurveyPoint`), `fabrilo` in lowercase identifiers and `FABRILO` in upper-case ones. The four-character file header is `FABR`, the ASCII bytes `0x46 0x41 0x42 0x52`. It fits a positive `i32`, as the SQLite `application_id` needs.

**No compatibility.** Nothing reads or writes the old identifiers after the rename. There is no alias for `bimcraft/` material keys, no import of `BIMCraft_SurveyPoint`, no acceptance of the `BIMC` header, no migration of the `bimcraft.language` key and no old environment variable. Nothing was released, so no file, export, script or browser holds them. PERS-01 (open files from earlier versions) is not broken: the only `BIMC` files are developers' scratch files. A developer's saved language choice resets to the default once. The project file format version (`user_version`) does not change, because the header marks the application, not the layout.

**Earlier ADRs.** Renaming an identifier in an earlier record is an editorial change, not a change of decision. Issue #388 updates the name in records 0001 to 0024 in place, and they keep their status. This is the one exception to "accepted records are not rewritten". This record keeps the old name, as the record of the change.

**Where the old name may remain.** This record, the §16.2 entry in the specification, and the allowlist of the guard in issue #389. Links to `github.com/GuillaumeOj/BIMCraft` stay until the owner renames the repository, because GitHub redirects them. Issues #388 and #389 handle them.

## Consequences

There is one rename sub-issue per area: #382 core and host crates, #383 IFC, #384 schema and API, #385 desktop, #386 web UI, #387 user guide, #388 repository files and developer documentation, #389 the guard. They can run in parallel once this record is merged.

Pull request #370 (3D viewport) merged before this record and added mentions of the old name, which the area sub-issues rename. Open pull request #378 adopts the new identifiers, or is renamed after it merges.

The owner's tasks outside the code are listed in #380, not here: reserving names, the trademark filing, renaming the repository, the project board and the design file.

## Alternatives considered

- **Keep BIMCraft.** Other products already use it, and the trademark risk remains.
- **Read the old identifiers alongside the new ones for one release.** No release exists, so this would be dead code to maintain and test.
- **Bump the file format version instead of changing the header.** The header identifies the application. A version bump would hide the rename.
- **Supersede every ADR that names an old identifier.** That would supersede 24 records for one word. It adds noise, and the decisions themselves do not change.

## References

- Issues [#380](https://github.com/GuillaumeOj/BIMCraft/issues/380) and [#381](https://github.com/GuillaumeOj/BIMCraft/issues/381).
- [Specification §16.2](../../specifications.md#16-risks-open-questions-and-glossary).
- [ADR 0006](0006-sqlite-binding.md): SQLite binding and `application_id`.
- [ADR 0016](0016-project-file-layout.md): project file layout.
- [ADR 0017](0017-georeference-storage-and-ifc-mapping.md): georeference storage and IFC mapping (survey point property set).
- [ADR 0018](0018-sourced-values.md) (sourced values) and [ADR 0019](0019-materials-and-layered-build-ups.md): material property sets.
