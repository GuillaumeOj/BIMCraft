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

const { CI } = process.env;

export default defineConfig({
  testDir: "e2e",
  forbidOnly: true,
  retries: CI ? 1 : 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: { baseURL: "http://localhost:4173", trace: "retain-on-failure" },
  // Type-checking is the UI check's job; e2e only needs the viewport module and the bundle. Locally, reuse a running preview.
  webServer: {
    command: "bun run build:viewport && bunx vite build && bunx vite preview --port 4173",
    port: 4173,
    reuseExistingServer: !CI,
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
