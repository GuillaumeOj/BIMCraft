# UI test setup

Apply this when creating `ui/package.json`. Install every package with an exact version (`ui/bunfig.toml` enforces `exact = true`), and set `"packageManager": "bun@<BUN_VERSION from scripts/tool-versions.env>"`.

## Unit tests: Vitest

Dev dependencies: `vitest`, `@vitest/coverage-v8`, `@testing-library/react`, `@testing-library/user-event`, `@testing-library/jest-dom`, `jsdom`. `src/test-setup.ts` registers the jest-dom matchers and unmounts rendered components after each test.

`ui/vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test-setup.ts"],
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/**/*.test.{ts,tsx}", "src/main.tsx", "src/test-setup.ts"],
      reporter: ["text", "lcov"],
      // AGENTS.md §5: 90 % overall, 98 % for critical areas.
      thresholds: {
        lines: 90,
        branches: 90,
        functions: 90,
        statements: 90,
        "src/api/**": { lines: 98, branches: 98, functions: 98, statements: 98 },
      },
    },
  },
});
```

## End-to-end tests: Playwright

Dev dependencies: `@playwright/test`, and `@types/node` for `process.env` in the config. Tests live in `ui/e2e/` and cover the main user flows. The `@playwright/test` version pins the browser build: install it with `bunx playwright install chromium` (CI adds `--with-deps`).

`ui/playwright.config.ts`:

```ts
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "e2e",
  forbidOnly: true,
  retries: process.env["CI"] ? 1 : 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: { baseURL: "http://localhost:4173", trace: "retain-on-failure" },
  webServer: { command: "bun run build && bun run preview --port 4173", port: 4173 },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
```

## TypeScript

`ui/tsconfig.json` extends `./tsconfig.base.json` and adds `include` paths (`src`, `e2e` and the config files) and types (`node`, `vite/client`, `@testing-library/jest-dom/vitest`).
