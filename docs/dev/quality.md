# Quality and security rules: rationale

[AGENTS.md](../../AGENTS.md) states the rules; this page explains why they exist and how they are enforced. Change a rule here and in AGENTS.md in the same pull request.

## How rules are enforced

| Layer | What | Binds |
| --- | --- | --- |
| Instructions | `AGENTS.md` (and `CLAUDE.md`, which imports it) | Every coding agent and human |
| Git hooks | `.githooks/`, enabled by `scripts/setup.sh`: fast checks on staged files, commit messages, the review stamp | Everyone committing or pushing from a clone |
| Agent guardrails | `.claude/settings.json` deny/ask rules and hooks | Claude Code |
| CI | `.github/workflows/` running `scripts/check-all.sh` and `scripts/check-pr.sh` | Every pull request; required by branch protection |

Instructions can be forgotten, hooks can be skipped on a misconfigured clone; CI with branch protection is the gate and the final authority.

## Push only with the user, never without review

Coding agents produce plausible code fast; the cost of a bad change on `main` is paid by everyone. So:

- Agents commit locally and push only when the user says so in the conversation: `.claude/settings.json` asks the user before each `git push` and `gh pr create`, and denies merges and releases. `.claude/hooks/guard-bash.sh` blocks, anywhere in a command line, what AGENTS.md §1 says an agent never does; its tests (`scripts/guard-bash.test.ts`) list each form.
- The `pre-push` hook refuses any `HEAD` not stamped by `scripts/mark-reviewed.sh`. The stamp is the commit SHA, stored in `.git/bimcraft-review-stamp` (never committed), so any new commit invalidates it. An agent can therefore push only commits the user has reviewed.
- Branches are kept up to date by rebasing, never by merging `main` into them, because branch protection requires a linear history ([AGENTS.md §3b](../../AGENTS.md#3b-keeping-a-branch-up-to-date-auto-fix-included)). A rebase rewrites the branch, so the agent may force-push it, but only with `--force-with-lease --force-if-includes`, as `scripts/push-pr.ts --force` does: together they refuse to overwrite remote commits of the branch that were never in the local branch. A lease alone is refreshed by any `git fetch` and then protects nothing, so `.claude/hooks/guard-bash.sh` rejects it without `--force-if-includes`. The rebased `HEAD` is a new commit and needs a new stamp, which lets the user review the conflict resolutions.
- The stamp is written only after the user has run a code review (correctness) and a simplification pass (reuse, clarity). In Claude Code these are `/code-review` and `/simplify`; the `ship` skill walks through the steps. `scripts/mark-reviewed.sh` asks the user to confirm both at a terminal and refuses when its input is not one, as in an agent's shell. This is the gate for every way of stamping, so tools that stamp, such as `scripts/push-pr.ts`, need no name-based block.
- `scripts/push-pr.ts` chains the commit checks (`scripts/check-pr.sh`), the stamp, the push and the pull request; every step that changes anything runs after the stamp, so without the user at a terminal it pushes nothing. Agents may run it with `--dry-run`. For a stack of branches it stamps the top one and pushes them all in one atomic push: the hook checks `HEAD`, and the review of the top covers every commit below it. Each pull request is based on the branch below and lists the stack in its description.

## CI is the gate; local checks are targeted

The full suite lints and tests every crate, measures coverage of each critical crate, builds for wasm, fuzzes the parsers and runs the UI and end-to-end tests. It grows with the workspace and keeps a desktop's CPU busy for minutes, while CI runs it in parallel on Linux, macOS and Windows for every pull request. So the full suite runs in CI alone, and the `pre-push` hook only checks the review stamp.

While working, agents and contributors run the narrowest check that covers their change ([AGENTS.md §3a](../../AGENTS.md#3a-local-checks-while-working)): the tests and lints of the crates or UI files they touched, plus the repository checks, which take seconds. A whole section runs locally only when nothing narrower can check the change, such as a workspace manifest or toolchain change. A check skipped locally costs a red CI run, never a bad merge. After the push, the agent tracks CI and fixes failures ([AGENTS.md §3](../../AGENTS.md#3-workflow), step 8).

## Exact versions

Ranges let a build change without a commit, which breaks reproducibility and opens the door to supply-chain attacks (a compromised patch release is pulled automatically). Exact versions plus committed lockfiles mean every change of dependency is a reviewed diff. The 14-day minimum release age gives the ecosystem time to detect malicious releases. Actions are pinned by commit SHA because tags can be moved. Dependabot keeps pins current.

## Coverage thresholds

90 % lines globally catches untested features without forcing tests of trivial glue. Critical areas — the schema and its generators, the model, transactions, project files, geometry, georeferencing, analysis, the API and file parsers — corrupt user data or produce wrong engineering results when they fail, so they need 98 % (aim for 100 %). Rust branch coverage needs a nightly compiler, so on the Rust side the gate is lines; UI critical code is gated on lines and branches. Coverage measures what was executed, not what was verified: reviews check that tests assert meaningful behaviour.

## Fuzzing

Parsers read hostile input (AGENTS.md §8), so each one has a property test in its crate and a [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) target in `fuzz/` (ENG-02). So does the geometry kernel, whose degenerate cases are where tolerance bugs hide: `profile`, `extrusion`, `boolean` (boxes) and `extruded_boolean` (prisms over any profile) check that nothing panics and that every accepted result holds its invariants. Their inputs come from the generators of `fuzz/src/lib.rs`, which put vertices on a quarter-metre grid so that edges and faces coincide often, and scale whole profiles down to the tolerance or out past the model extent. `fuzz/` is a workspace of its own because libFuzzer needs a nightly compiler, pinned by date in `fuzz/rust-toolchain.toml`; the main workspace stays on stable. Starting inputs live in `fuzz/seeds/<target>/`, a link to the test fixtures when a crate already has suitable ones, so seeds never go stale.

`scripts/check-all.sh fuzz` lints the targets and runs each one for a fixed number of inputs with a fixed seed, so CI stays reproducible; booleans of prisms (`extruded_boolean`), a few hundred inputs a second, run a quarter as many. Set `FUZZ_RUNS` to change the count of every target. To hunt for bugs, run a target for longer from `fuzz/`, for example `cargo fuzz run --fuzz-dir . schema_definition corpus/schema_definition seeds/schema_definition -- -max_total_time=600`. A crash is saved in `fuzz/artifacts/`. Before fixing it, minimise it (`cargo fuzz tmin`), commit it under a descriptive name in `fuzz/seeds/<target>/`, so every later run replays it, and decode it (`cargo fuzz fmt`) into a regression test in the crate, for the geometry kernel in `crates/geom/tests/fuzz_regressions.rs`. The local corpus (`fuzz/corpus/`) is not committed; seeds are the findings and a few starting inputs.

## IFC sample corpus

IFC 4.3 files must round-trip without loss (INT-03), and the round-trip runs in CI on public sample files (INT-04); passing it is part of the Phase 0 gate. The corpus is the *Simple-Scene* set of buildingSMART's [Sample-Test-Files](https://github.com/buildingSMART/Sample-Test-Files), licensed CC BY 4.0. It is never committed and never contains real customer models (AGENTS.md §8).

`crates/interop-ifc/tests/corpus/manifest.txt` pins the repository revision and the SHA-256 of every file. `scripts/fetch-ifc-corpus.sh` downloads the files into `target/ifc-corpus/` and rejects any file whose checksum differs; files already there and intact are not downloaded again. The test, `crates/interop-ifc/tests/corpus.rs`, reads them from `IFC_CORPUS_DIR` and never touches the network. Each file must read, write and read back to the same values, write the same bytes twice, decode instance by instance into the IFC 4.3 classes, and keep what an import does not map. Without `IFC_CORPUS_DIR` the test does nothing, so a plain `cargo test` stays offline. `scripts/check-all.sh corpus` runs both steps, on Linux in CI.

To add files, list them in the manifest with their checksum (`shasum -a 256`), after checking their licence. To move to a newer revision, update the revision and every checksum together, and wait 14 days after the revision is published, as for dependencies.

## Benchmarks

Hot paths keep [criterion](https://docs.rs/criterion) benchmarks in their crate's `benches/`: the exact predicates of the geometry kernel (`cargo bench -p geom`) and the attribute tables of the model (`cargo bench -p core-model`). CI compiles them through `clippy --all-targets` but does not run them: timings on shared runners are too noisy to gate on. Run them before and after a change to a hot path and quote the numbers in the pull request. Reference numbers, and the memory measured per object (NFR-08), live in [performance-baselines.md](performance-baselines.md).

## Linters, formatters and import sorting

Formatting and import order are never discussed in review: tools decide. Rust uses `rustfmt` and `clippy` (pedantic, warnings as errors); the UI uses Biome, which formats, lints and sorts imports in one tool; TOML uses `taplo`; spelling uses `typos`. Agents' edits are formatted automatically by `.claude/hooks/format-file.sh`.

## Security

- `unsafe` is denied workspace-wide. Modules that genuinely need it (e.g. GPU buffer mapping) are listed below with the reason, and opt in with a local `#[allow(unsafe_code)]`.
- `cargo deny` checks advisories, licences, duplicate versions and sources. `cargo audit --deny warnings` checks the same lockfiles against the RustSec advisory database with the reference tool, so a gap or bug in one auditor does not hide a vulnerability; unmaintained and yanked crates fail too. Both run in the `rust` and `fuzz` sections of `scripts/check-all.sh`. An advisory accepted on purpose goes in both `deny.toml` and `.cargo/audit.toml` (`[advisories] ignore`), with the reason. `bun audit` checks UI dependencies.
- Every published GitHub release gets a Software Bill of Materials (SBOM): `.github/workflows/release.yml` attaches one CycloneDX file per workspace crate, written by `scripts/sbom.sh <dir>`. The `rust` section of `scripts/check-all.sh` generates it too, so a change that breaks it fails before a release.
- `gitleaks` scans for secrets at commit time and in CI. CodeQL analyses workflows, Rust and TypeScript.
- Workflows run with read-only tokens by default, never persist checkout credentials and never use `pull_request_target`.

### Audited `unsafe` modules

None yet.

## Architecture rules

Core crates run unchanged on the desktop and in the browser (`ARCH-01`). In `wasm32-unknown-unknown`, `std::fs`, `std::thread`, `std::time::Instant`, sockets and blocking I/O compile but fail or panic at run time, so a successful wasm32 build proves nothing. Core crates reach the outside world only through the `platform` traits (`Storage`, `TaskExecutor`, `Clock`, `ComputeBackend`), which each host implements (`ARCH-02`).

- `clippy.toml` lists the forbidden functions and types in `disallowed-methods` and `disallowed-types`; `-D warnings` turns any use into a lint error. Clippy checks the paths themselves, so a misspelt entry fails too.
- The list applies to every crate. Crates outside `scripts/wasm-crates.txt` (desktop host, solvers, plugin host) need these APIs and opt out with `#![allow(clippy::disallowed_methods, clippy::disallowed_types)]` at their crate root.
- Core crates start their crate root with `#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]`. Unlike `deny`, a `forbid` cannot be undone by an `allow` or `expect` anywhere in the crate, by a lint group such as `clippy::style`, or by Cargo `[lints]`. `scripts/check-architecture.ts` checks that every core crate root carries it and that no core crate has its own `clippy.toml`. Integration tests in `tests/` are separate crates and may opt out, for instance to read parser fixtures.
- The same lists ban the std transcendental float methods (`f64::sin`, `f64::exp`, `f64::powi` and their relatives, for `f64` and `f32`), whose precision depends on the platform. Core crates use the pure-Rust `libm` crate instead, so geometry gives the same bits on every target (`ENG-04`). Exactly rounded methods such as `sqrt`, `mul_add` and `floor` stay allowed. The opt-out above lifts this ban too. Rationale: [ADR 0003](adr/0003-deterministic-math-library.md).
- Core crate tests use the in-memory doubles in `platform::testing`: add `platform = { workspace = true, features = ["testing"] }` to `[dev-dependencies]`. They keep tests deterministic and runnable on wasm32: time moves only when the test advances it, and tasks run only when the test drives them.
- `crates/geom/tests/golden.rs` checks that `ENG-04` holds. It computes a fixed, seeded set of `libm` functions, compositions of them and exact predicates on nearly degenerate input. It then compares every result bit for bit with the committed `tests/golden.txt`. It runs natively on Linux, macOS and Windows (`rust` section) and under `wasm32-wasip1` with wasmtime (`wasm` section; `.cargo/config.toml` sets wasmtime as the runner). geom's `criterion` and `proptest` dev-dependencies do not build for wasm, so they are declared for native targets only, and the wasm run builds just this test. On a difference it fails, naming the first diverging case with the expected and actual values. It runs on `wasm32-wasip1` rather than the browser target `wasm32-unknown-unknown` because wasmtime can host libtest there; both targets use the same wasm code generation and the same pure-Rust `libm`. After a deliberate change (a `libm` upgrade, a new kernel function worth covering), regenerate the file natively with `BIMCRAFT_BLESS=1 cargo test -p geom --test golden` and review the diff.

Layers keep the dependency graph pointing one way (`ARCH-04`): the kernel never learns about disciplines, disciplines never learn about the API, and nothing in the core learns about a host. Without it, a convenient import today becomes a cycle of knowledge that blocks reuse by plugins and the browser host later. `scripts/crate-layers.txt` declares the layers once, lowest first; a crate may depend on its own layer or below. `scripts/check-architecture.ts` reads every `crates/*/Cargo.toml` (normal, dev, build and target-specific dependencies) and fails on an upward dependency, or on a crate missing from the file or listed twice. Only path dependencies count, with `{ workspace = true }` resolved through the root manifest, so renames are followed and a registry crate that shares a member's name is ignored. It parses the manifests itself rather than calling `cargo metadata`, so it runs in the repository checks without a Rust toolchain.

Adding a crate: list it in `scripts/crate-layers.txt`, and in `scripts/wasm-crates.txt` (then add the `forbid` line) and `scripts/critical-crates.txt` when they apply.

## End-user documentation

BIMCraft users are architects and engineers. A feature they cannot find or understand does not exist for them. The user guide (`docs/user/`, built with mdBook) is organised by task and written in plain language, and `scripts/check-pr.sh` fails a pull request whose `feat`/`fix` commits change product code without touching `docs/user/`, unless it carries the `no-user-docs` label with a reason.
