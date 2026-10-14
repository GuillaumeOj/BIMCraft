# Quality and security rules: rationale

[AGENTS.md](../../AGENTS.md) states the rules; this page explains why they exist and how they are enforced. Change a rule here and in AGENTS.md in the same pull request.

## How rules are enforced

| Layer | What | Binds |
| --- | --- | --- |
| Instructions | `AGENTS.md` (and `CLAUDE.md`, which imports it) | Every coding agent and human |
| Git hooks | `.githooks/` enabled by `scripts/setup.sh` | Everyone committing or pushing from a clone |
| Agent guardrails | `.claude/settings.json` deny/ask rules and hooks | Claude Code |
| CI | `.github/workflows/` running `scripts/check-all.sh` and `scripts/check-pr.sh` | Every pull request; required by branch protection |

Instructions can be forgotten, hooks can be skipped on a misconfigured clone; CI with branch protection is the final authority. Local checks exist so that CI failures are rare.

## Never push without the user, never push without review

Coding agents produce plausible code fast; the cost of a bad change on `main` is paid by everyone. So:

- Agents may commit locally but never push: `.claude/settings.json` denies `git push`, PR creation and hook bypasses; `.claude/hooks/guard-bash.sh` catches them anywhere in a command line.
- The `pre-push` hook refuses any `HEAD` not stamped by `scripts/mark-reviewed.sh`. The stamp is the commit SHA, stored in `.git/bimcraft-review-stamp` (never committed), so any new commit invalidates it.
- The stamp is written only after the user has run a code review (correctness) and a simplification pass (reuse, clarity). In Claude Code these are `/code-review` and `/simplify`; the `ship` skill walks through the steps.

## Pre-push checks follow the changes

Running every check before each push grows with the workspace: every crate is linted and tested, each critical crate is measured for coverage again, the parsers are fuzzed. So after the review stamp, the `pre-push` hook asks `scripts/affected.ts` which checks the branch can break, comparing it with the remote's `main` since they diverged:

| Changed | Checks run |
| --- | --- |
| a crate in `crates/` | Rust lint, tests, critical-crate coverage and wasm build for that crate and every crate depending on it; fuzzing if the fuzz targets link one of them |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `clippy.toml`, `deny.toml`, `rustfmt.toml`, `.cargo/`, the critical or wasm crate lists | every Rust check on every crate |
| `fuzz/` | fuzzing |
| `ui/` | UI and end-to-end tests |
| `docs/user/` | user guide build |
| `scripts/check-all.sh`, or a path matching no rule | everything |

The repository checks (spelling, TOML, secrets, pinned versions, layering, tooling tests) always run: they take seconds and cover every file. A scoped run skips the workspace-wide 90 % coverage gate, which only a full run can measure. If the scope cannot be computed (no remote `main`, unreadable manifest), the hook runs everything; `BIMCRAFT_FULL_CHECKS=1 git push` forces it. CI always runs every check, so a missed dependency costs a red CI run, never a bad merge.

## Exact versions

Ranges let a build change without a commit, which breaks reproducibility and opens the door to supply-chain attacks (a compromised patch release is pulled automatically). Exact versions plus committed lockfiles mean every change of dependency is a reviewed diff. The 14-day minimum release age gives the ecosystem time to detect malicious releases. Actions are pinned by commit SHA because tags can be moved. Dependabot keeps pins current.

## Coverage thresholds

90 % lines globally catches untested features without forcing tests of trivial glue. Critical areas — the schema and its generators, the model, transactions, project files, geometry, georeferencing, analysis, the API and file parsers — corrupt user data or produce wrong engineering results when they fail, so they need 98 % (aim for 100 %). Rust branch coverage needs a nightly compiler, so on the Rust side the gate is lines; UI critical code is gated on lines and branches. Coverage measures what was executed, not what was verified: reviews check that tests assert meaningful behaviour.

## Fuzzing

Parsers read hostile input (AGENTS.md §8), so each one has a property test in its crate and a [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) target in `fuzz/` (ENG-02). `fuzz/` is a workspace of its own because libFuzzer needs a nightly compiler, pinned by date in `fuzz/rust-toolchain.toml`; the main workspace stays on stable. Starting inputs live in `fuzz/seeds/<target>/`, a link to the test fixtures when a crate already has suitable ones, so seeds never go stale.

`scripts/check-all.sh fuzz` lints the targets and runs each one for a fixed number of inputs with a fixed seed, so CI stays reproducible; set `FUZZ_RUNS` to change the count. To hunt for bugs, run a target for longer from `fuzz/`, for example `cargo fuzz run --fuzz-dir . schema_definition corpus/schema_definition seeds/schema_definition -- -max_total_time=600`. A crash is saved in `fuzz/artifacts/`: turn it into a regression test in the crate before fixing it.

## Benchmarks

Hot paths of the geometry kernel, starting with the exact predicates, keep [criterion](https://docs.rs/criterion) benchmarks in their crate's `benches/` (run them with `cargo bench -p geom`). CI compiles them through `clippy --all-targets` but does not run them: timings on shared runners are too noisy to gate on. Run them before and after a change to a hot path and quote the numbers in the pull request.

## Linters, formatters and import sorting

Formatting and import order are never discussed in review: tools decide. Rust uses `rustfmt` and `clippy` (pedantic, warnings as errors); the UI uses Biome, which formats, lints and sorts imports in one tool; TOML uses `taplo`; spelling uses `typos`. Agents' edits are formatted automatically by `.claude/hooks/format-file.sh`.

## Security

- `unsafe` is denied workspace-wide. Modules that genuinely need it (e.g. GPU buffer mapping) are listed below with the reason, and opt in with a local `#[allow(unsafe_code)]`.
- `cargo deny` checks advisories, licences, duplicate versions and sources. `bun audit` checks UI dependencies.
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
