# BIMCraft UI

The web frontend (Bun, React, TypeScript), shared by the desktop (Tauri) and browser hosts. See spec §13.

The interface is designed in Penpot; see [docs/dev/ui-design.md](../docs/dev/ui-design.md) before building a screen.

## Commands

Run them from `ui/` after `bun install`:

| Task | Command |
| --- | --- |
| Development server | `bun run dev` |
| Production build | `bun run build` |
| Unit tests with coverage | `bunx vitest run --coverage` |
| End-to-end tests (first run: `bunx playwright install chromium`) | `bunx playwright test` |
| Lint, format and imports | `bunx biome ci .` |
| Type check (all code, then the app without Node types) | `bun run typecheck` |

## Configuration

- `bunfig.toml` — exact versions, 14-day minimum release age.
- `biome.json` — formatter, linter and import sorting.
- `tsconfig.base.json` — strict TypeScript; `tsconfig.json` extends it.
- `vitest.config.ts` — unit tests and coverage thresholds; `playwright.config.ts` — end-to-end tests in `e2e/`; see [docs/dev/ui-setup.md](../docs/dev/ui-setup.md).

## Translations (UI-04)

Every string shown to the user comes from `src/i18n/messages/en.json` (the reference) and `fr.json`; components read them with `useI18n().t("key")`, with `{name}` placeholders filled from a second argument. Biome's `noJsxLiterals` rule rejects text written directly in JSX and the `lint/no-literal-text-attributes.grit` plugin rejects it in user-visible attributes (`aria-label`, `title`, `alt`, `placeholder`…), and a unit test fails, listing the keys, when `fr.json` misses or adds a key. At run time a missing French string falls back to English.
