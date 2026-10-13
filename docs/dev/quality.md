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

## Exact versions

Ranges let a build change without a commit, which breaks reproducibility and opens the door to supply-chain attacks (a compromised patch release is pulled automatically). Exact versions plus committed lockfiles mean every change of dependency is a reviewed diff. The 14-day minimum release age gives the ecosystem time to detect malicious releases. Actions are pinned by commit SHA because tags can be moved. Dependabot keeps pins current.

## Coverage thresholds

90 % lines globally catches untested features without forcing tests of trivial glue. Critical areas — the model, transactions, geometry, georeferencing, analysis, the API and file parsers — corrupt user data or produce wrong engineering results when they fail, so they need 98 % (aim for 100 %). Rust branch coverage needs a nightly compiler, so on the Rust side the gate is lines; UI critical code is gated on lines and branches. Coverage measures what was executed, not what was verified: reviews check that tests assert meaningful behaviour.

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

Layers keep the dependency graph pointing one way (`ARCH-04`): the kernel never learns about disciplines, disciplines never learn about the API, and nothing in the core learns about a host. Without it, a convenient import today becomes a cycle of knowledge that blocks reuse by plugins and the browser host later. `scripts/crate-layers.txt` declares the layers once, lowest first; a crate may depend on its own layer or below. `scripts/check-architecture.ts` reads every `crates/*/Cargo.toml` (normal, dev, build and target-specific dependencies) and fails on an upward dependency, or on a crate missing from the file or listed twice. Only path dependencies count, with `{ workspace = true }` resolved through the root manifest, so renames are followed and a registry crate that shares a member's name is ignored. It parses the manifests itself rather than calling `cargo metadata`, so it runs in the repository checks without a Rust toolchain.

Adding a crate: list it in `scripts/crate-layers.txt`, and in `scripts/wasm-crates.txt` (then add the `forbid` line) and `scripts/critical-crates.txt` when they apply.

## End-user documentation

BIMCraft users are architects and engineers. A feature they cannot find or understand does not exist for them. The user guide (`docs/user/`, built with mdBook) is organised by task and written in plain language, and `scripts/check-pr.sh` fails a pull request whose `feat`/`fix` commits change product code without touching `docs/user/`, unless it carries the `no-user-docs` label with a reason.
