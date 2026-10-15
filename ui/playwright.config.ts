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
