# Fix the picomatch vulnerability that fails the UI audit on main

Requirement IDs: SEC-05 (dependency auditing in CI). No issue. Milestone: Phase 1 – MVP.

## 1. Problem

The `UI` CI job (`scripts/check-all.sh ui`) ends with `bun audit`, which now fails on `main`:

```
picomatch@4.0.7
  vite > tinyglobby > fdir > picomatch
  vitest > tinyglobby > fdir > picomatch
  high: picomatch vulnerable to ReDoS through single-branch repeated extglobs (<=4.0.7)
  https://github.com/advisories/GHSA-fc5q-39xc-vjpf
```

## 2. Findings

- **No patched version exists.** The advisory covers `<=4.0.7`, and 4.0.7 (2026-08-24) is the newest picomatch on npm; there is no 4.0.8 and no 5.x. `bun audit fix --dry-run` says: "no published version fixes: picomatch@4.0.7 GHSA-fc5q-39xc-vjpf". An upgrade or an `overrides` entry cannot fix it today.
- **The vulnerable code is used by development tools only.** `bun pm why picomatch` shows it reaches us only through `vite`, `vitest`, `@vitejs/plugin-react`, `@vitest/coverage-v8` and `@testing-library/jest-dom`, through `tinyglobby` and `fdir`. All are `devDependencies`. The runtime dependencies (`react`, `react-dom`, `@tauri-apps/api`) do not use it, and none of it is in the built app.
- **The patterns are ours.** Exploiting a ReDoS takes a hostile glob pattern. Vite and Vitest only match globs written in our own config and test files, never patterns from users, models or the network. The worst case is a slow local build or test run caused by a pattern a developer wrote.

So the risk to users is nil and the risk to developers is negligible. The fix is to **accept the advisory on purpose, for now, with the reason written down**, the same way the Rust side accepts advisories in `deny.toml` and `.cargo/audit.toml`, and to drop the exception as soon as picomatch publishes a fixed version.

## 3. Deliverables

| File | Change |
| --- | --- |
| `scripts/check-all.sh` | In `check_ui`, `bun audit` becomes `bun audit --ignore=GHSA-fc5q-39xc-vjpf`, with a comment above it: the advisory, why it is accepted (dev-only tooling, our own patterns), and "drop when picomatch publishes a release newer than 4.0.7" |
| `docs/dev/quality.md` | Where it says "`bun audit` checks UI dependencies" (Security section): an advisory accepted on purpose for the UI goes in `--ignore` on the `bun audit` line of `scripts/check-all.sh`, with the reason, and is dropped when a fix is released |

Out of scope: upgrading vite, vitest or any other package (none of them fixes this); forking or vendoring picomatch; `bunfig.toml` changes; any Rust or UI code.

## 4. Acceptance

- `cd ui && bun audit --ignore=GHSA-fc5q-39xc-vjpf` passes: "No vulnerabilities found (… 1 ignored)" (checked on the integration branch).
- `bun audit` without the flag still reports the advisory: only this one advisory is ignored, nothing else is hidden.
- `scripts/check-all.sh repo` is clean (shellcheck, typos, editorconfig).
- CI's `UI` job is green on the pull request.

## 5. Commits and pull request

- One commit, `ci(ui): accept the picomatch ReDoS advisory until a fix is released (Refs SEC-05)`, signed off. Type `ci`, not `fix`: no `docs/user` change is needed (CI and developer docs only), so the PR states "User docs: N/A — CI and developer documentation only".
- No `wip` subjects.

## 6. Follow-up

When picomatch releases a fixed version, Renovate opens the bump (the 14-day age rule does not apply to security fixes). That pull request also removes the `--ignore` flag and its comment. `bun audit` does not warn about an unused `--ignore`, so the comment is the reminder.
