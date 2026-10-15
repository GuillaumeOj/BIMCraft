import { defineConfig, devices } from "@playwright/test";

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
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
