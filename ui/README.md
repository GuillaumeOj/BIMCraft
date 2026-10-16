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
| Lint, format and imports | `bunx biome ci .` |
| Type check | `bunx tsc --noEmit` |

## Configuration

- `bunfig.toml` — exact versions, 14-day minimum release age.
- `biome.json` — formatter, linter and import sorting.
- `tsconfig.base.json` — strict TypeScript; `tsconfig.json` extends it.
- `vitest.config.ts` — unit tests and coverage thresholds; see [docs/dev/ui-setup.md](../docs/dev/ui-setup.md).
