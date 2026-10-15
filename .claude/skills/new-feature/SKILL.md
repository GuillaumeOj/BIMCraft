---
name: new-feature
description: Implement a new BIMCraft feature or change an existing one following the project's Definition of Done — spec requirement, tests first, coverage, e2e, end-user docs. Use whenever the user asks to add, build, implement or change a feature.
---

# New feature

Follow AGENTS.md. Steps:

1. **Locate the requirement.** Find the matching section and requirement IDs in `docs/specifications.md`. If nothing matches, tell the user and propose a requirement (next free ID in the family) before coding.
2. **Plan the slice.** Identify the crates and UI modules touched. For UI work, read the matching board in the Penpot design (`docs/dev/ui-design.md`). Check AGENTS.md §5 for critical areas (98 % coverage, property tests for parsers and geometry). Respect `ARCH-01` to `ARCH-04` (wasm32, no platform calls in core, layering).
3. **Tests first.** Write failing unit tests describing the behaviour, including edge cases and invalid input. For a main user flow, add or extend a Playwright e2e test in `ui/e2e/`.
4. **Implement** the smallest change that passes. No new dependency unless justified (AGENTS.md §4), pinned exactly.
5. **Check.** Run the targeted checks for the crates and UI files you touched (AGENTS.md §3a) until they are green, plus `scripts/check-all.sh repo`. Leave the full suite to CI.
6. **User documentation.** Copy `docs/user/_template.md` into the right task folder, write it for architects and engineers (no developer jargon), and link it in `docs/user/SUMMARY.md`. Update existing pages the change affects. Skip only for changes with no visible effect, and say so.
7. **Commit** locally with `git commit -s` and a Conventional Commit subject; name the requirement ID in the body.
8. **Stop.** Summarise what changed and what the user should review. Do not push unless they say so; suggest the `ship` skill when they are ready.
