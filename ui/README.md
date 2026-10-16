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
