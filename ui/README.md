# BIMCraft UI

The web frontend (Bun, React, TypeScript), shared by the desktop (Tauri) and browser hosts. See spec §13.

No application code yet. The quality configuration is already in place:

- `bunfig.toml` — exact versions, 14-day minimum release age.
- `biome.json` — formatter, linter and import sorting.
- `tsconfig.base.json` — strict TypeScript; the app's `tsconfig.json` extends it.

When creating `package.json`, follow [docs/dev/ui-setup.md](../docs/dev/ui-setup.md) for the test configuration (Vitest coverage thresholds and Playwright e2e).
