# Add Renovate and fix dependencies — feature spec

Related requirement: SEC-05 (dependency auditing) and AGENTS.md §4 (exact versions, 14-day release age, Renovate). Developer tooling only: "User docs: N/A (developer tooling, no user-facing change)". [ux-brief.md](ux-brief.md) and [api-contract.md](api-contract.md) are N/A for the same reason.

## Starting point (audit of `main` at 6e597ef)

Most of the brief already holds on `main`. PR #289 replaced Dependabot with Renovate:

- **Exact pins: done.** `bun scripts/check-pinned-deps.ts` passes. Every crate in `Cargo.toml`, `fuzz/Cargo.toml` and `crates/*/Cargo.toml` uses `=x.y.z` or `workspace = true`. Every package in `ui/package.json` (dependencies, devDependencies, overrides) and `packageManager` is exact. `ui/bunfig.toml` sets `exact = true`. Every action is pinned to a commit SHA. The toolchains are exact (`1.98.1`, `nightly-2026-09-28`), and so are the CLI tools in `scripts/tool-versions.env`. All three lockfiles are committed.
- **Renovate config: present.** `.github/renovate.json5` sets the 14-day minimum release age and the Monday schedule, keeps actions pinned by SHA, signs commits off, and uses `rangeStrategy: bump`. It defines these groups: `rust-minor-patch`, `ui-minor-patch`, `actions`, `toolchains` (CLI tools, Rust toolchain, Bun) and `tauri` (crates and npm package together). The fuzz nightly is bumped monthly.

So no version needs to be re-pinned. This feature closes the gaps the audit found, so that Renovate actually keeps **every** pin current, in groups, without breaking CI.

## Gaps found

| # | Gap | Effect |
| --- | --- | --- |
| G1 | Renovate has no built-in manager for `rust-toolchain.toml`, as far as the lead can tell (to be confirmed in T1). The `toolchains` rule matches the file, but nothing reads the `channel`. The same goes for the fuzz nightly rule. | The Rust toolchain (`1.98.1`) and the fuzz nightly are never bumped, even though AGENTS.md §4 says Renovate covers them. |
| G2 | No lock-file maintenance. | Transitive dependencies in `Cargo.lock`, `fuzz/Cargo.lock` and `ui/bun.lock` only move when a direct dependency moves, or when a security alert fires. |
| G3 | Renovate only refreshes the lockfile of the workspace it edits. `fuzz/Cargo.lock` also locks the root crates' dependencies, so any Rust bump that leaves `fuzz/Cargo.toml` untouched breaks the Fuzzing check. Today only `docs/dev/quality.md` mentions it. | Every Rust Renovate PR fails CI until someone runs `cargo update -w` in `fuzz/` by hand, and nothing in the PR says so. |
| G4 | Nothing checks `.github/renovate.json5`. A typo or an invalid option shows up only as a Renovate error issue, after the merge. | Renovate can silently stop working. |
| G5 | `check-pinned-deps.ts` skips `scripts/tool-versions.env`. | A range or a tag there (`latest`, `1.7`) would pass. |
| G6 | Majors are not covered by any explicit grouping rule, which is fine (see D1), but neither the config nor the docs say so. | Unclear policy. |

## Decisions

### D1. Groups

The existing groups stay. One rule is added: lock-file maintenance becomes its own group. Final grouping:

| Group | Contents | Schedule |
| --- | --- | --- |
| `rust-minor-patch` | cargo minor + patch, both workspaces | Monday before 6am |
| `ui-minor-patch` | bun minor + patch | Monday before 6am |
| `actions` | every GitHub Action (any update type) | Monday before 6am |
| `toolchains` | `scripts/tool-versions.env`, the Rust stable toolchain, Bun (`packageManager` included) | Monday before 6am |
| `tauri` | tauri crates and `@tauri-apps/api` | Monday before 6am |
| `fuzz-nightly` | `fuzz/rust-toolchain.toml` | first day of the month |
| `lockfiles` (new) | `lockFileMaintenance` for the three lockfiles | first day of the month |
| majors | one PR per package. `config:recommended` keeps monorepos together (React, Vitest, Testing Library…). | Monday before 6am |

**Majors stay one PR per package.** A major update often needs code changes. If majors were grouped, one hard major would hold back every other one. They still go through the 14-day age and the Dependency Dashboard. *(If the user wants majors grouped per ecosystem too, that is a one-rule change. Say so at approval.)*

### D2. Rust toolchain via a regex custom manager

A `customManagers` regex entry reads `channel = "x.y.z"` in the root `rust-toolchain.toml`. Datasource: the Rust releases (`github-releases` or `github-tags` on `rust-lang/rust`, whichever gives clean `x.y.z` versions). It is in the `toolchains` group. The root `Cargo.toml` `rust-version` (MSRV `1.98`) is a policy decision, not a pin, and is **not** bumped automatically.

The fuzz nightly (`nightly-YYYY-MM-DD`) has no datasource listing dated nightlies. T1 checks whether one exists that Renovate can use without a self-hosted setup. If none does, the dead `fuzz/rust-toolchain.toml` schedule rule is removed, and `docs/dev/quality.md` says the nightly is bumped by hand, for instance when the Fuzzing check needs it. No Renovate rule should pretend to manage a file it cannot read.

T1 first confirms G1 from the Renovate docs (the manager list). If a built-in manager does exist, D2 is dropped and T1 just checks that the existing rules match it.

### D3. The fuzz lockfile: a PR body note, not automation

Automating `cargo update -w` in `fuzz/` on Renovate branches needs either `postUpgradeTasks` (self-hosted Renovate only) or a workflow that pushes to the PR branch. Pushes made with `GITHUB_TOKEN` do not trigger CI, so that workflow would need a PAT or an app secret. It also makes Renovate treat the branch as edited and stop rebasing it. Neither is worth it now. Instead:

- Every Renovate PR from the cargo manager (the `rust-minor-patch` group, cargo majors, `tauri`, `lockfiles`) carries a `prBodyNotes` line: "Run `cargo update -w` in `fuzz/` and commit `fuzz/Cargo.lock`, or the Fuzzing check fails (docs/dev/quality.md#fuzzing)."
- The Fuzzing check's failure message says so too, when the cause is a stale `fuzz/Cargo.lock` (cargo's `--locked` error), as long as that can be done cheaply in `scripts/check-all.sh fuzz`.

Automation is out of scope. It could be a follow-up issue if the manual step turns out to be a burden.

### D4. Validate the Renovate config in CI

- A new `renovate` section in `scripts/check-all.sh` runs `renovate-config-validator --strict` from the `renovate` npm package, at an exact version, fetched with `bunx`. The version lives in `scripts/tool-versions.env` with a `# renovate:` comment, so Renovate bumps it like any other tool (`toolchains` group).
- CI runs it in an existing job: the cheapest place is the repo job, or a job filtered on `.github/renovate.json5` changes if the download is slow. T1 measures and picks. It is not part of the local `repo` section, which must stay fast and offline. AGENTS.md §3a gets one line: run `scripts/check-all.sh renovate` when `.github/renovate.json5` changes. The §9 table lists the section.

### D5. Enforce exact versions in `scripts/tool-versions.env`

`check-pinned-deps.ts` also checks `scripts/tool-versions.env`. Every `NAME_VERSION=` value must be an exact `x.y.z`, and every one must have a `# renovate:` comment on the line above it, so no tool is left unmanaged. Tests are added in `scripts/check-pinned-deps.test.ts`.

## Out of scope

- Re-pinning or bumping any current version. Renovate does that after the merge.
- Automating the fuzz lockfile refresh (D3).
- Changing the MSRV, the 14-day age, or the schedule.

## Tasks (after approval)

One task, `back`. This is repository tooling (`.github/`, `scripts/`, `docs/dev/`, `AGENTS.md`), with no UI or design work, so `front`, `ui` and `ux` are not used.

- **T1 (back)**: D1–D5. Files: `.github/renovate.json5`, `scripts/check-pinned-deps.ts` and its test, `scripts/tool-versions.env`, `scripts/check-all.sh`, `.github/workflows/ci.yml`, `docs/dev/quality.md`, `docs/dev/github-setup.md` if needed, `AGENTS.md` §3a/§4/§9.

## Acceptance criteria

1. `bun scripts/check-pinned-deps.ts` passes, and its tests cover the `tool-versions.env` rule (a range, a tag, and a missing `# renovate:` comment are rejected).
2. `scripts/check-all.sh renovate` validates `.github/renovate.json5` with no warning, and CI runs it.
3. Every pinned version in the repo is read by a Renovate manager (built-in or custom), or is documented as bumped by hand (fuzz nightly, if D2 finds no datasource). The commit body lists each file and the manager that reads it.
4. The config has the groups of D1 and the `prBodyNotes` of D3, and lock-file maintenance is on.
5. `docs/dev/quality.md` (dependencies and fuzzing sections) and AGENTS.md describe the result. "User docs: N/A (developer tooling)."
6. `scripts/check-all.sh repo` and the targeted checks pass. Commits are Conventional (`ci(deps): …`, `build(deps): …`) and signed off.
