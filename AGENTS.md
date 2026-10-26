# AGENTS.md — rules for anyone working on BIMCraft

This file is the single source of truth for coding agents (Claude Code, Codex, Cursor, Copilot, Gemini…) and for humans. If another instruction file disagrees with it, this one wins. Rationale for each rule: [docs/dev/quality.md](docs/dev/quality.md).

BIMCraft is an open-source BIM authoring tool: a Rust core (native + WebAssembly), a React/TypeScript UI in Tauri, Python plugins later. Read the [specification](docs/specifications.md) before designing anything; requirements have stable IDs such as `CORE-01`.

## 1. Golden rules (never break)

1. **Push only with the user's OK.** Committing locally is fine. Push your own feature branch (plain `git push`, never `main`) and open a **draft** pull request (`gh pr create --draft`) only when the user explicitly says so **in the current conversation**, and only once they have stamped `HEAD` (rule 2); every later push needs a new OK. Never force-push (except after a rebase, where the user's stamp is the OK: section 3b), delete remote branches, push tags, merge pull requests or mark them ready for review, publish releases, or create or edit issues, comments, labels and project items unless the user asks. `scripts/push-pr.ts` is the user's tool; run it only with `--dry-run`.
2. **Review before push.** Before any push, the user must have run a code review and a simplification pass on the changes (Claude Code: `/code-review`, then `/simplify`; other agents: an equivalent review the user asks for). Only then does the user stamp `HEAD` with `scripts/mark-reviewed.sh`, which asks them to confirm at a terminal. The `pre-push` hook rejects any push whose `HEAD` is not stamped; it runs no tests (CI does). Every new commit, a CI fix included, needs a new review (of the new commits) and a new stamp. Never stamp, or try to answer the confirmation, yourself.
3. **Never bypass checks.** No `--no-verify`, no editing `core.hooksPath`, no disabling lints, lowering coverage thresholds or skipping tests to make a check pass. Fix the cause, or stop and ask the user.
4. **Exact versions only.** Every dependency, tool and CI action is pinned to an exact version (section 4).
5. **Done means documented.** A user-facing change is not done until its end-user documentation exists (section 6).

## 2. Definition of Done

Every feature or fix is done only when all of these hold. Check them before saying a task is complete.

- [ ] It implements or references a spec requirement ID when one applies (in the commit and PR).
- [ ] Unit tests cover it: **≥ 90 % lines** overall, **≥ 98 % lines** for critical areas, plus branches in the UI (section 5). Tests assert behaviour, not implementation details.
- [ ] Main user flows have an **end-to-end test** (Playwright).
- [ ] Bug fixes come with a regression test that failed before the fix.
- [ ] Format, lint, import order and type checks are clean (section 7) with **zero warnings**.
- [ ] `cargo deny check` is clean; no new dependency without justification (section 4).
- [ ] End-user documentation is added or updated in `docs/user/`, or the PR states “User docs: N/A” with a reason (section 6).
- [ ] Commits follow Conventional Commits and are signed off (`git commit -s`).
- [ ] The targeted local checks pass (section 3a), and **CI is green** on the pull request. CI is the gate.

## 3. Workflow

1. Understand the task; find its issue ([docs/dev/project-management.md](docs/dev/project-management.md)), the relevant spec section and requirement IDs and, for UI work, the matching board in the Penpot design ([docs/dev/ui-design.md](docs/dev/ui-design.md)).
2. Write or update tests first when practical, then implement.
3. Keep changes small and focused: one concern per commit, one feature per PR.
4. Run the targeted checks for what you changed (section 3a), not the whole suite.
5. Write the user documentation.
6. Commit locally (`git commit -s`, Conventional Commits, e.g. `feat(geom): add exact orientation predicate`).
7. Stop. Tell the user the work is ready for review. Wait for the review, the simplification pass, their stamp and an explicit instruction to push (or for them to push with `scripts/push-pr.ts`).
8. **Track CI** on the pull request: `gh pr checks <number> --watch`, then `gh run view <run-id> --log-failed` for a failed job. Fix each failure at its cause, rerun the targeted check that reproduces it, commit, and go back to step 7. Repeat until CI is green. A merge conflict or a branch behind its base (reported by CI, GitHub or Auto-fix) follows section 3b.

## 3a. Local checks while working

CI runs every check on every pull request, on Linux, macOS and Windows. Locally, run only what the change can break, with the narrowest command:

- **Rust**: `cargo test -p <crate>`, `cargo clippy -p <crate> --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`; for a critical crate you changed, `cargo llvm-cov -p <crate> --all-features --fail-under-lines 98`. `CHECK_CRATES="<crate> …" scripts/check-all.sh rust` runs the whole Rust section on those crates only.
- **UI**, from `ui/`: `bunx vitest run <files>` (or `bunx vitest related --run <files>`), `bunx biome check <files>`, `bunx tsc --noEmit`.
- **Before each commit**: `scripts/check-all.sh repo` (seconds).
- **A full section only when nothing narrower checks the change**: workspace manifests, the toolchain, `clippy.toml`, `deny.toml`, the critical or wasm crate lists or `scripts/check-all.sh` changed (run the sections they affect); a core crate's platform use changed (`wasm`); a fuzz target changed (`fuzz`); the STEP reader or writer, or the IFC corpus manifest changed (`corpus`); or a CI failure cannot be reproduced otherwise. Never run the whole `scripts/check-all.sh` by default.

## 3b. Keeping a branch up to date (Auto-fix included)

1. **Rebase, never merge**, whenever the branch is behind `main`, even when nothing conflicts; never use merge-based sync tools (such as the Claude desktop app's `sync_with_base_branch`). Run `git fetch origin main`, then `git rebase origin/main`; for a stack, from its top branch, `git rebase --update-refs origin/main`.
2. **Conflicts.** Resolve each conflict as the rebase reaches it (`git add`, `git rebase --continue`). Keep the intent of both sides; never resolve by dropping the other side's changes wholesale. If the right resolution is unclear, or the rebase stops for any other reason (a file it cannot write, a failing hook), `git rebase --abort` and ask the user.
3. **Check.** Run the targeted checks (section 3a) for the files whose conflicts you resolved, then `scripts/check-all.sh repo`; CI covers the rest.
4. **Restamp.** The rebased `HEAD` is a new commit: stop and ask the user to review it (the conflict resolutions in particular) and run `scripts/mark-reviewed.sh`.
5. **Force-push with lease.** Once the user has stamped the rebased `HEAD`, push with `git push --force-with-lease --force-if-includes`; that stamp is the OK for this push. For a stack, hand the user `bun scripts/push-pr.ts --stack --force …` instead. Never use `--force`, `-f` or a `+` refspec.

## 4. Dependencies

- **Rust**: exact requirements only: `serde = "=1.0.228"`. Declare versions once in `[workspace.dependencies]`; member crates use `dep = { workspace = true }`. `Cargo.lock` is committed.
- **JavaScript (Bun)**: exact versions only, no `^`, `~`, ranges, tags or `latest`. `ui/bunfig.toml` sets `exact = true`; `bun.lock` is committed and CI installs with `--frozen-lockfile`. Dependency install scripts are blocked unless listed in `trustedDependencies` with a reason.
- **GitHub Actions**: `uses:` must reference a full 40-character commit SHA, with the version as a comment: `uses: actions/checkout@3d3c… # v7.0.1`.
- **Toolchains**: Rust in `rust-toolchain.toml`, Bun in `ui/package.json` (`packageManager`) and CI, every CLI tool version in CI and `scripts/setup.sh`.
- **Release age**: only adopt versions published at least 14 days ago (supply-chain protection), unless a security fix requires otherwise.
- **New dependency?** Prefer the standard library or existing dependencies. Justify a new one in the commit body: purpose, maintenance status, licence (must pass `deny.toml`), size, and whether it builds for `wasm32-unknown-unknown` when used by a core crate.
- `scripts/check-pinned-deps.ts` enforces this locally and in CI. Dependabot proposes version bumps.

## 5. Tests and coverage

- Rust: unit tests next to the code (`#[cfg(test)]`), integration tests in `tests/`. Coverage with `cargo llvm-cov`.
- UI: Vitest + Testing Library for units, Playwright for e2e (`ui/e2e/`). Coverage with V8 provider.
- Thresholds: **90 % lines** globally (CI fails below). **98 % lines** for critical areas (and 98 % branches in the UI; Rust branch coverage needs nightly); aim for 100 % and justify each uncovered line.
- **Critical areas**: `schema`, `schema-gen`, `core-model`, `core-txn`, `geom`, `georef`, `persist`, `analysis`, `api`, all `interop-*` parsers and writers, `platform` implementations that touch files; in the UI, the command/API client layer (`ui/src/api/`). The list lives in `scripts/critical-crates.txt` (Rust); keep it in sync when crates are added.
- Parsers (including the `schema` definition parser) and the geometry kernel also need property-based tests (`proptest`) and fuzz targets (`cargo fuzz`, in `fuzz/`) — `ENG-02`.
- Structural calculations need validation cases against hand calculations or published references — `ENG-03`.
- Tests must be deterministic: no network, no wall clock, no real time sleeps, seeded randomness.

## 6. End-user documentation

Users are architects and engineers, not developers. For every user-facing feature or change:

- Add or update a page in `docs/user/` from [`docs/user/_template.md`](docs/user/_template.md), and link it in `docs/user/SUMMARY.md`.
- Organise by **task** (“Place a column”, “Export to IFC”), not by code module.
- Plain language, no internal jargon (crate names, “entity”, “transaction”); explain domain terms on first use.
- Step-by-step instructions with expected results; mention keyboard shortcuts and command-palette names; add a “Troubleshooting” section for known pitfalls.
- English first; French follows (`UI-04`).
- Developer documentation goes in `docs/dev/`, not in the user guide.

## 7. Code quality

**Rust**

- `cargo fmt` (config `rustfmt.toml`; imports are sorted by rustfmt).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` with the workspace lints in the root `Cargo.toml` (pedantic, no `unwrap`/`expect`/`panic` in non-test code, `unsafe_code` denied).
- `taplo fmt` for TOML files.
- Errors: typed errors (`thiserror`) in libraries; no panics on bad input.
- Core crates build for `wasm32-unknown-unknown` (`ARCH-01`) and never use `std::fs`, `std::thread`, `std::time::Instant`, sockets or blocking I/O (`ARCH-02`); go through `platform` traits. `clippy.toml` rejects these APIs; core crates start with `#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]`, and only crates outside `scripts/wasm-crates.txt` may opt out, at their crate root.
- Core crates compute transcendental functions with `libm` (`libm::sin`, `libm::pow`…), never the std float methods (`f64::sin`, `f64::powi`…), whose precision depends on the platform (`ENG-04`); `clippy.toml` rejects them.
- No crate depends on a crate in a higher layer (`ARCH-04`). Layers are declared in `scripts/crate-layers.txt`; add every new crate there.
- Public items are documented (`missing_docs`).

**TypeScript / UI**

- Biome for formatting, linting **and import sorting** (`ui/biome.json`). No ESLint or Prettier.
- `tsc --noEmit` in strict mode (`ui/tsconfig.base.json`); no `any`, no non-null assertions, no `@ts-ignore`.
- The UI talks to the core only through the command API (`UI-01`); all strings are externalised (`UI-04`); UI components are accessible (labels, keyboard, contrast — `UI-06`).

**Everywhere**

- `typos` for spelling, `.editorconfig` for whitespace.
- Small functions, clear names, no dead code, no commented-out code, no `TODO` without an issue link.
- Comments explain *why*, not *what*.

## 8. Security

- No `unsafe` Rust outside the audited list in `docs/dev/quality.md` (`SEC-01`). Every `unsafe` block carries a `// SAFETY:` comment.
- Treat every external input as hostile: IFC/DXF/BCF files, IPC messages, plugin and MCP calls, user text. Validate, bound sizes and recursion, never trust offsets or counts.
- Never commit secrets, tokens, credentials or real customer models. `gitleaks` runs in pre-commit and CI.
- Never log model content or personal data (`SEC-03`).
- CI workflows use least-privilege `permissions:` and never run untrusted code with secrets.
- Report suspected vulnerabilities privately (see [SECURITY.md](SECURITY.md)), never in public issues.
- Never paste text from design codes or standards (copyright, `REP-05`).

## 9. Commands

| Check | Command |
| --- | --- |
| One-time setup (hooks + tool check) | `scripts/setup.sh` |
| Everything CI runs that applies (rarely needed locally, section 3a) | `scripts/check-all.sh` (or one section: `repo rust coverage wasm fuzz fuzz-explore corpus ui e2e docs`; `CHECK_CRATES="a b"` limits the Rust sections) |
| Commit messages + user-docs rule for a branch | `scripts/check-pr.sh main` |
| Rust format | `cargo fmt --all --check` |
| Rust lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Rust tests + coverage | `cargo llvm-cov --workspace --fail-under-lines 90` |
| Dependency audit | `cargo deny check` |
| Fuzz targets: lint and replay every seed (blocking in CI) | `scripts/check-all.sh fuzz` (details: [quality.md](docs/dev/quality.md#fuzzing)) |
| Fuzz exploration (bounded, seeded run; nightly in CI, not blocking) | `scripts/check-all.sh fuzz-explore` |
| IFC round-trip on the public sample corpus | `scripts/check-all.sh corpus` (details: [quality.md](docs/dev/quality.md#ifc-sample-corpus)) |
| TOML format | `taplo fmt --check` |
| Spelling | `typos` |
| Pinned versions | `bun scripts/check-pinned-deps.ts` |
| Crate layering and core-crate lint scope | `bun scripts/check-architecture.ts` |
| UI lint/format/imports | `cd ui && bunx biome ci .` |
| UI types | `cd ui && bunx tsc --noEmit` |
| UI tests + coverage | `cd ui && bunx vitest run --coverage` |
| UI e2e | `cd ui && bunx playwright test` |
| User guide build | `mdbook build docs/user` |
| Reference book build (schema pages regenerated by `UPDATE_SNAPSHOTS=1 cargo test -p schema-gen`) | `mdbook build docs/reference` |
| Stamp HEAD as reviewed (user only, at a terminal) | `scripts/mark-reviewed.sh` |
| Stamp, push and open the PR, or a whole stack of PRs (user only) | `bun scripts/push-pr.ts --help` |
| Watch CI on a pull request | `gh pr checks <number> --watch` |
| Logs of the failed CI jobs | `gh run view <run-id> --log-failed` |

## 10. Where things live

| Path | Content |
| --- | --- |
| `docs/specifications.md` | Specification and requirement IDs |
| `docs/user/` | End-user guide (mdBook) |
| `docs/reference/` | API reference (mdBook); `schema/` pages are generated |
| `docs/dev/` | Developer documentation, quality rationale, GitHub setup |
| `docs/dev/adr/` | Architecture decision records |
| `crates/` | Rust crates |
| `ui/` | Web frontend (Bun, React, TypeScript) |
| `scripts/` | Repository tooling |
| `.githooks/` | Git hooks shared by everyone (enable with `scripts/setup.sh`) |
| `.github/` | CI workflows, templates, Dependabot |
| `.claude/` | Claude Code settings, hooks and skills |
