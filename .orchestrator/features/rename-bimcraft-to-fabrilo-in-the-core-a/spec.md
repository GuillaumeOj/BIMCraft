# Rename BIMCraft to Fabrilo in the core and host crates (#382)

Parent: epic #380. Decision record: [ADR 0028](../../../docs/dev/adr/0028-rename-to-fabrilo.md) on `main`
(issue #381, merged as PR #393). Requirement IDs: none (PERS-01 is affected, not broken).

## Goal

The Rust core crates, the desktop host and the fuzz crate stop using the old name. After this
feature, `git grep -i bimcraft -- crates/ fuzz/` matches only `interop-ifc`, `api` (outside its
material-key tests), `schema-gen` and `desktop-app`, which other sub-issues rename (#383, #384, #385).

Identifiers (ADR 0028): `bimcraft` → `fabrilo`, `BIMCRAFT_*` → `FABRILO_*`, `bimcraft/<name>` →
`fabrilo/<name>`, file header `BIMC` (`0x4249_4D43`, 1112100163) → `FABR` (`0x4641_4252`, 1178681938).
Prose writes `Fabrilo`; rustdoc keeps the backticks the crates already use (`` `Fabrilo` ``, clippy's
`doc_markdown`).

**No compatibility** (ADR 0028): nothing reads the old identifiers. No alias for `bimcraft/` keys,
no acceptance of the `BIMC` header, no fallback to `BIMCRAFT_BLESS`.

## Base branch

The integration branch was cut at `3d79cab`, 144 commits behind `main`. ADR 0028, the `viewport`
crate and the `api` build-up tests exist only on `main`. The lead rebases the integration branch onto
`main` before creating the tasks, so both tasks start from a base that has every file in scope.

## Changes

### A. Persistence, desktop host, materials (node `back`)

- `persist`
  - `APPLICATION_ID` becomes `0x4641_4252`; its doc says "FABR" in ASCII.
  - `NotAProject` message: "the file is not a Fabrilo project" (and its unit test). The user
    guide quotes it: in `docs/user/save-and-open-a-project.md`, update the troubleshooting entry
    (the quoted message and the sentence under it) so it matches. The rest of that page is #387.
  - Crate docs (`lib.rs` line 1) and the temporary-file names in `tests/crash_safety.rs`,
    `tests/migrations.rs`, `tests/native_file.rs` (`fabrilo-…`).
  - `tests/crash_safety.rs`: the raw `PRAGMA application_id = 1112100163` becomes 1178681938.
  - **Fixtures.** Regenerate every `.bim` in `crates/persist/tests/fixtures/` with the `FABR`
    header, as ADR 0028 decides. Change only the header: `PRAGMA application_id = 1178681938` on
    each file (for example with `sqlite3`), so tables, rows and `user_version` stay as written. Check
    with `PRAGMA application_id; PRAGMA user_version;` before and after, and that `migrations` still
    opens every version. `format-0001.sql` holds no header and does not change.
  - **Regression test:** a database carrying the old `BIMC` id (1112100163) with a valid
    `user_version` is refused with `NotAProject`.
  - `docs/dev/schema-versions.md` says fixtures are "never rewritten": add one sentence pointing to
    ADR 0028 as the single exception.
- `host-desktop`
  - Worker thread name `fabrilo-worker-{n}`, and the test that checks the prefix.
  - `TEMP_SUFFIX` `.fabrilo-tmp`, and the storage tests that use it.
- `material`
  - `BUILTIN_SOURCE` `fabrilo`; doc comments in `library.rs`.
  - Keys in the `material` tests (`library/tests.rs`, `model/layer_sets/tests.rs`) and in the
    `api` tests that use material keys (`src/materials/tests.rs`, `src/buildups/tests.rs`).
    Only these key strings change in `api`; the rest of `api` is #384.
  - `data/README.md`: the product name and the `fabrilo/<name>` key rule. Check `data/*.json`
    for keys or names that embed the source; keys are built from the source at load time today.

### B. Geometry, georeference, crate docs, fuzz (node `front`)

- `geom/tests/golden.rs`, `georef/tests/golden.rs`: `BIMCRAFT_BLESS` → `FABRILO_BLESS` in the
  code, the module docs and the failure message. Same in `docs/dev/quality.md` (the two golden-test
  bullets). No CI workflow or script sets the variable today; confirm with `git grep`.
- `georef/epsg/epsg.tsv` header line, the line in `extract.py` that writes it, and
  `epsg/README.md`. Only the header comment changes; data rows stay byte-for-byte.
  Check that nothing hashes `epsg.tsv` (the README's SHA-256 is of PROJ's `proj.db`).
- Crate-level docs (line 1 of `lib.rs`): `core-ext`, `core-model`, `core-txn`, `geom`, `georef`,
  `platform`, `render`, `viewport`.
- `fuzz/`: package `bimcraft-fuzz` → `fabrilo-fuzz` in `fuzz/Cargo.toml`; `use bimcraft_fuzz::` →
  `use fabrilo_fuzz::` in every `fuzz_targets/*.rs`; refresh `fuzz/Cargo.lock` (`cargo update -w`
  in `fuzz/`, offline if possible, so only the package name changes). Check any script or workflow
  that names the fuzz package (`scripts/check-all.sh`, `.github/workflows/`).

The two tasks touch disjoint files and run in parallel.

## Out of scope

`interop-ifc` (#383), `api` outside the material-key tests and `schema-gen` (#384), `desktop-app`
(#385), `ui/` (#386), `docs/user/` (#387) except the troubleshooting entry above, other repository files, other dev docs, ADRs, scripts,
`.github/` (#388) except the dev-doc lines named above, the old-name guard (#389).

## Checks (each task, for what it changed)

- `cargo test -p <crate>` and `cargo clippy -p <crate> --all-targets --all-features -- -D warnings`
  for each changed crate; `cargo fmt --all --check`.
- Critical crates changed (`persist`, `material`, `geom`, `georef`, `api`, `platform`):
  `cargo llvm-cov -p <crate> --all-features --fail-under-lines 98`.
- Task B: `scripts/check-all.sh fuzz`; `FABRILO_BLESS=1 cargo test -p geom --test golden` leaves
  `golden.txt` unchanged (same for `georef`).
- `scripts/check-all.sh repo` before each commit.
- Commits: Conventional Commits, subject ≤ 100 characters after `type(scope): `, ending with
  `(Refs #382)`, and a `Signed-off-by: Guillaume <7831777+GuillaumeOj@users.noreply.github.com>`
  line. Never a `wip` subject; never the user's private e-mail.

## Done when

- `git grep -i bimcraft -- crates/ fuzz/` lists only files in `interop-ifc`, `api` (non-test
  or non-material code), `schema-gen` and `desktop-app`.
- The checks above pass, and CI is green on the pull request.
- The PR's user-docs line says: the one user-visible change, the "not a Fabrilo project" error,
  is updated in the troubleshooting entry of "Save and open a project"; everything else is internal
  identifiers. The rest of the user guide is #387.
