# Contributing to BIMCraft

Thank you for your interest in BIMCraft. The project needs two kinds of contributors equally: software developers, and practitioners (architects, engineers, draftspeople, BIM managers) who know what the tool must do.

## Where the project stands

BIMCraft is in its specification phase. Until the first crates land, the most valuable contributions are:

- **Reviewing the [specification](docs/specifications.md).** Open an issue that names the requirement ID you are discussing (e.g. `STR-07`), or the section if you propose something new.
- **Bringing domain knowledge.** National practices, design codes, drawing conventions and real project workflows all shape the requirements.
- **Discussing open questions** listed in section 16.2 of the specification.

## Ground rules

- Be respectful and constructive. The [code of conduct](CODE_OF_CONDUCT.md) (Contributor Covenant 2.1) applies.
- Write in English in code, commits, issues and documentation, so that everyone can follow.
- Never paste text from design codes or standards (Eurocodes, national annexes, ISO, etc.): it is copyrighted. Cite clauses by number and enter values as data with their source (see `REP-05`).

## Developer Certificate of Origin

Every commit must be signed off, certifying that you wrote the change or have the right to submit it under the project's MIT licence, as described in the [Developer Certificate of Origin 1.1](https://developercertificate.org/). Use `git commit -s`, which adds a line such as:

```
Signed-off-by: Jane Doe <jane@example.com>
```

Pull requests with unsigned commits cannot be merged. To sign off commits you already made: `git rebase --signoff main`.

## Workflow

1. Run `scripts/setup.sh` once after cloning: it enables the shared git hooks (`.githooks/`) and lists the tools to install, with their exact versions.
2. Open or pick an issue first for anything beyond a small fix, so the approach can be agreed before you write code.
3. Fork the repository and create a branch from `main` (e.g. `feat/ifc-step-parser` or `docs/spec-georef`).
4. Keep each pull request focused on one change, and reference the issue and any requirement IDs it implements.
5. Meet the [Definition of Done](AGENTS.md#2-definition-of-done): tests and coverage, e2e for main flows, end-user documentation in `docs/user/`.
6. Run the checks for what you changed ([AGENTS.md §3a](AGENTS.md#3a-local-checks-while-working); CI runs everything), get the changes reviewed and simplified, then stamp, push and open the pull request in one go with `bun scripts/push-pr.ts` (or stamp with `scripts/mark-reviewed.sh` and push by hand). Stamping asks you to confirm the reviews at a terminal, and the `pre-push` hook refuses unreviewed commits.

   ```sh
   # One branch
   bun scripts/push-pr.ts --title "feat(geom): add predicates" --body-file pr.md --label geometry --assignee @me
   # A stack of branches (each PR is based on the branch below it), from the top branch
   bun scripts/push-pr.ts --stack --assignee @me \
     --branch geom-predicates --title "feat(geom): add predicates" --body-file predicates.md \
     --branch geom-offsets --title "feat(geom): add offsets" --body "Builds on the predicates."
   ```

   Options before any `--branch` apply to every pull request. After a rebase, add `--force`; existing pull requests are updated, not duplicated. `--dry-run` shows what would happen, `--help` lists every option.
7. CI is the gate: fix failures until every check on the pull request is green (`gh pr checks <number> --watch`).

### Using a coding agent

Agents are welcome. [AGENTS.md](AGENTS.md) is the rulebook every agent must follow (Claude Code reads it through `CLAUDE.md`). In short: you run a code review and a simplification pass (in Claude Code: `/code-review`, then `/simplify`; the `ship` skill guides you) and stamp the result; then you push, or tell the agent to push its branch and open a draft pull request. The agent then tracks CI and proposes fixes, each reviewed and stamped by you before it is pushed. You stay responsible for every line you submit.

## Commit messages

Use the [Conventional Commits](https://www.conventionalcommits.org/) style, for example:

```
feat(geom): add exact orientation predicate for planar booleans

Implements part of GEO-03.

Signed-off-by: Jane Doe <jane@example.com>
```

Common types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`.

## Code guidelines

The full rules are in [AGENTS.md](AGENTS.md), with their rationale in [docs/dev/quality.md](docs/dev/quality.md). Highlights:

- Exact dependency versions only (Rust `=x.y.z`, no `^`/`~` in `package.json`, actions pinned by commit SHA).
- Rust: `cargo fmt`, `cargo clippy ... -D warnings`, `cargo deny check`. UI: Biome (format, lint, import order) and strict `tsc`.
- Test coverage of at least 90 % lines, 98 % for critical areas (`NFR-07`); e2e tests for main user flows.
- Core crates must build for `wasm32-unknown-unknown` (`ARCH-01`) and must not use `std::fs`, `std::thread`, `std::time::Instant`, sockets or blocking I/O directly (`ARCH-02`); go through the `platform` traits instead.
- No `unsafe` outside the audited list of modules (`SEC-01`).
- Calculation features also need validation cases against hand calculations or published references (`ENG-03`).
- User-facing changes come with a page in the [user guide](docs/user/), written for non-developers.

## Changing the specification

The specification lives in [`docs/specifications.md`](docs/specifications.md) and changes through pull requests like code.

- Requirement IDs are stable: never renumber or reuse one. Retire a requirement by marking it as withdrawn.
- New requirements take the next free number in their family and a priority (MVP, V1 or Later).
- Explain the reason for a change in the pull request, especially for scope or priority changes.

## Reporting security issues

Do not open a public issue for a security vulnerability. Follow the [security policy](SECURITY.md).

## Licence

By contributing, you agree that your contributions are licensed under the [MIT licence](LICENSE).
