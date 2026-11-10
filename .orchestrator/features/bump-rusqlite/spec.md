# Bump rusqlite — feature spec

Security fix. No issue, no requirement ID of its own; it keeps `persist` (PERS-*) building on a patched SQLite and unblocks every Rust CI job.

## Problem

`cargo deny check` fails on every pull request:

- **RUSTSEC-2026-0459** (CVE-2026-119375): SQLite before 3.54.0 can return uninitialised page-cache memory when a database is opened with a sibling `-wal` file whose page size differs. Local attack vector; the victim must open crafted files. A BIMCraft project file received from someone else is exactly that case.
- Our chain: `persist` → `rusqlite =0.40.2` (`bundled`) → `libsqlite3-sys 0.38.2` (SQLite 3.53.2).
- Fixed in `libsqlite3-sys 0.39.0` (SQLite 3.54.0) and `0.39.1` (SQLite 3.54.1), shipped with `rusqlite 0.41`. `rusqlite 0.41` declares Rust 1.89; our toolchain is 1.98.1, so no toolchain change.

## Goal

Every crate, lockfile and doc that pins `rusqlite` moves to the latest `0.41.x` release (`0.41.1` at the time of writing), so the bundled SQLite is ≥ 3.54.0 and `cargo deny check` is clean again.

## Scope

1. **Workspace pin** — `Cargo.toml` `[workspace.dependencies]`: `rusqlite = "=0.41.1"`, keeping `default-features = false` and the `bundled` and `ffi-sqlite-wasm-rs` features (check that both features still exist under the same names in 0.41; adapt if renamed). `crates/persist/Cargo.toml` inherits through `workspace = true` (its dev-dependency adds `hooks`: check that feature still exists).
2. **Lockfiles** — `Cargo.lock` and `fuzz/Cargo.lock` (the fuzz workspace depends on `persist`). Both must resolve `libsqlite3-sys ≥ 0.39.0`, and `rusqlite 0.40.2` / `libsqlite3-sys 0.38.2` must be gone from both. Use `cargo update -p rusqlite` (and `-p libsqlite3-sys` if needed) rather than a broad `cargo update`, so no unrelated crate moves.
3. **Browser target** — `rusqlite`'s `ffi-sqlite-wasm-rs` pulls `sqlite-wasm-rs` (0.5.5 today, SQLite 3.53.0). If `rusqlite 0.41` requires a newer `sqlite-wasm-rs`, take it; record which SQLite version the browser build now bundles. If the browser build still bundles SQLite < 3.54.0, say so in the report (it is not flagged by the advisory, which targets `libsqlite3-sys`, but the lead decides whether to raise it).
4. **API changes** — fix any breaking change of `rusqlite 0.41` in `crates/persist/src/` (`lib.rs`, `migrate.rs`, `model.rs`, `codec.rs`) and its tests. No behaviour change: the existing tests must pass unchanged in what they assert.
5. **ADR 0006** (`docs/dev/adr/0006-sqlite-binding.md`) — update the "two targets bundle slightly different SQLite releases (3.53.2 on desktop, 3.53.0 in the browser…)" sentence with the new versions. Keep the licence paragraph true (check the licences of any new transitive crate).
6. **New transitive dependencies** — if 0.41 adds a crate, justify it in the commit body (AGENTS.md §4) and make sure it passes `deny.toml` and builds for `wasm32-unknown-unknown`.

Out of scope: any other dependency bump, any change to how `persist` uses SQLite.

## Release-age rule

AGENTS.md §4 asks for versions published at least 14 days ago, "unless a security fix requires otherwise". This is a security fix: take the latest `0.41.x` even if it is younger than 14 days, and say so in the commit body.

## Checks (targeted, AGENTS.md §3a)

- `cargo deny check` — clean (the gate that fails today).
- `cargo test -p persist`, `cargo clippy -p persist --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`.
- `cargo llvm-cov -p persist --all-features --fail-under-lines 98` (`persist` is critical).
- The workspace manifest changed, so `scripts/check-all.sh wasm` (the browser build of SQLite) and `scripts/check-all.sh fuzz` (the fuzz lockfile) also run.
- `bun scripts/check-pinned-deps.ts`, `taplo fmt --check`, `scripts/check-all.sh repo`.

## Delivery

- One task, backend (`back`). No UI work: the frontend node is idle on this feature.
- One commit: `fix(deps): bump rusqlite to 0.41.1 for RUSTSEC-2026-0459` (signed off), body naming the advisory, the new `libsqlite3-sys` / SQLite versions on both targets, and the release-age exception.
- PR body: "User docs: N/A (dependency security update, no user-facing change)".
- `.orchestrator/` does not ship: the lead removes it in a final commit before the PR request.
