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

// WebGPU for the 3D viewport specs. Chromium's Dawn uses Metal on macOS, so it needs no adapter flag.
// Elsewhere there is no graphics card (CI), so it falls back to SwiftShader, a software Vulkan.
const webgpuArgs = [
  "--enable-unsafe-webgpu",
  "--ignore-gpu-blocklist",
  ...(process.platform === "darwin"
    ? []
    : [
        "--enable-features=Vulkan",
        "--use-webgpu-adapter=swiftshader",
        "--enable-unsafe-swiftshader",
        "--use-angle=swiftshader",
      ]),
];

export default defineConfig({
  testDir: "e2e",
  forbidOnly: true,
  retries: process.env["CI"] ? 1 : 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: { baseURL: "http://localhost:4173", trace: "retain-on-failure" },
  // Type-checking is the UI check's job; e2e only needs the bundle. Locally, reuse a running preview.
  webServer: {
    command: "bunx vite build && bunx vite preview --port 4173",
    port: 4173,
    reuseExistingServer: !process.env["CI"],
  },
  projects: [
    {
      name: "chromium",
      // `channel: "chromium"` selects the full browser in its new headless mode: the old headless shell has no WebGPU.
      use: {
        ...devices["Desktop Chrome"],
        channel: "chromium",
        launchOptions: { args: webgpuArgs },
      },
    },
  ],
});
```

`e2e/webgpu.spec.ts` checks that this browser gives WebGPU an adapter and can render and read back a pixel, so the viewport specs can rely on it.

## TypeScript

`ui/tsconfig.json` extends `./tsconfig.base.json` and adds `include` paths (`src`, `e2e` and the config files) and types (`node`, `vite/client`, `@testing-library/jest-dom/vitest`). `ui/tsconfig.app.json` checks the application code alone (`src` without tests) with browser types only, so Node globals such as `process` cannot reach code that runs in the window; `bun run typecheck` runs both, and `bun run build` and `scripts/check-all.sh ui` call it.
