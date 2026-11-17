# Show Fabrilo in the web interface (EN and FR)

Issue [#386](https://github.com/GuillaumeOj/BIMCraft/issues/386), sub-issue of the epic [#380](https://github.com/GuillaumeOj/BIMCraft/issues/380) "Rename BIMCraft to Fabrilo". Requirement: UI-04 (externalised strings, EN and FR). Decision record: [ADR 0028](../../../docs/dev/adr/0028-rename-to-fabrilo.md) (the issue says "ADR 0025": the record was numbered 0028 when it merged).

The web UI (`ui/`) shows and stores the product name. This feature renames it to **Fabrilo**, following the ADR's identifier table: `Fabrilo` in prose, `fabrilo` in lowercase identifiers, `fabrilo.*` for browser storage keys.

Frontend work only: no Rust, no command API change, so there is no `api-contract.md` and no design task (`ux-brief.md`): the favicon artwork does not change, only its title.

## 1. Scope

The issue lists the page title, the header, the favicon, the language key, the package name and two docs. The ADR's table covers **every** browser storage key (`bimcraft.*` → `fabrilo.*`) and the epic's acceptance criterion is that `git grep -i bimcraft` matches only the ADR and the guard's allowlist. #386 is the web-UI area of the rename, so this feature renames **every** mention under `ui/`, with the one exception below. The extra items are marked †.

| File | Change |
| --- | --- |
| `ui/src/i18n/messages/en.json`, `fr.json` | `app.name`: `Fabrilo`. † `viewport.unavailable.body` and `viewport.error.repeated`: "BIMCraft" → "Fabrilo" (EN and FR) |
| `ui/index.html` | `<title>Fabrilo</title>` |
| `ui/public/favicon.svg` | `<title>Fabrilo</title>` and "Fabrilo app icon, …" in the comment. Artwork unchanged |
| `ui/src/i18n/I18nProvider.tsx` | `OVERRIDE_STORAGE_KEY = "fabrilo.language"` |
| † `ui/src/shell/ShellState.tsx` | `SINGLE_KEYS_STORAGE_KEY = "fabrilo.singleKeyShortcuts"` |
| † `ui/src/viewport/HintCard.tsx` | `HINT_STORAGE_KEY = "fabrilo.viewport.navigationHint"` |
| † `ui/src/viewport/StateCards.tsx` | Copied error report: `Fabrilo 3D view stopped: …` |
| † `ui/src/viewport/useViewportSession.ts` | Doc comment: "restart Fabrilo" |
| `ui/package.json` | `"name": "fabrilo-ui"`; `ui/bun.lock` refreshed with `bun install` (only the workspace name changes) |
| Tests | `I18nProvider.test.tsx`, `Shell.test.tsx`, `LocationDialog.test.tsx`, † `ViewPanel.test.tsx`, `ViewportView.test.tsx`, `storage.test.ts`, and the e2e `smoke.spec.ts`, † `e2e/support/viewport.ts` |
| `ui/README.md` | Title "Fabrilo UI", design file `docs/Fabrilo.figcraft` |
| `docs/dev/ui-design.md` | Lines 3 and 15: "The Fabrilo interface", design file `docs/Fabrilo.figcraft` |

Tests should reference the exported constants (`OVERRIDE_STORAGE_KEY`, `HINT_STORAGE_KEY`) instead of repeating the key string where that is simple, so the key lives in one place.

**Not in scope (left as is):**

- `ui/src/viewport/guide.ts` and `guide.test.ts`: the user-guide URL `github.com/GuillaumeOj/BIMCraft/…`. Per the ADR, GitHub links stay until the owner renames the repository; #388 and #389 handle them.
- `docs/dev/ui-design.md` lines 35–36 (`BIMCraft.icon`) and `crates/desktop-app/icons/BIMCraft.icon/`: the desktop icon bundle, renamed by #385 (desktop), whose branch also touches `scripts/build-icons.ts`.
- The FigCraft design file itself and the names on its boards: renamed by the maintainer (#380). This feature only updates the path the docs point to.
- The user guide (`docs/user/`): #387.

## 2. Behaviour

- The header heading (level 1), the browser tab title and the favicon title read "Fabrilo" in English and in French (`app.name` is the same in both).
- The language choice is stored under `fabrilo.language`. **No migration** of `bimcraft.language` (ADR 0028, "No compatibility"): nothing was released, so a developer's saved language, single-key-shortcut setting and dismissed navigation hint reset to their defaults once. The pull request says so.

## 3. Definition of Done

- `git grep -i bimcraft -- ui docs/dev/ui-design.md` matches only `guide.ts`, `guide.test.ts` and `ui-design.md` lines 35–36 (not `bun.lock`).
- From `ui/`: `bunx vitest run --coverage` meets the thresholds (90 % lines; 98 % lines and branches in `ui/src/api/`), `bunx biome ci .` and `bunx tsc --noEmit` are clean, and the Playwright smoke test (`bunx playwright test e2e/smoke.spec.ts`) passes. The sandbox cannot run Chromium: if Playwright cannot start, say so and leave it to CI.
- `scripts/check-all.sh repo` passes.
- Conventional Commits, `Refs #386`, signed off. No `wip` subjects.
- User docs: N/A here, covered by #387 (linked in the PR).

## 4. Tasks

1. **front** — the whole rename above, one or two commits (`chore(ui): show Fabrilo in the web interface (Refs #386)`, `docs(dev): …`). Review by **back**.

**back** has no task: there is no backend work in this feature.
