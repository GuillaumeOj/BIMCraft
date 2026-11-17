import { defineConfig, devices } from "@playwright/test";
import { browserLogFile } from "./e2e/support/browser-log";

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
  // Chromium's own log, GPU process included, goes to a file: a failed viewport test attaches its
  // GPU lines (e2e/support/viewport.ts), and the job's output stays quiet.
  "--enable-logging",
  `--log-file=${browserLogFile}`,
];

const { CI } = process.env;

// Every page mounts the 3D view, and on CI each browser renders it on the CPU. The specs that
// draw the most run alone, one at a time, after the others: two software renderers at once
// starve the GPU process until the browser loses its WebGPU context. A failure in the first
// project skips the second for that run; that is the price of never running them side by side.
// A new spec lands in the parallel project unless it is listed here.
const gpuSpecs = ["**/viewport.spec.ts", "**/section-visibility.spec.ts", "**/webgpu.spec.ts"];

// `channel: "chromium"` selects the full browser in its new headless mode: the old headless shell has no WebGPU.
const chromium = {
  ...devices["Desktop Chrome"],
  channel: "chromium",
  launchOptions: { args: webgpuArgs },
};

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
    { name: "chromium", testIgnore: gpuSpecs, use: chromium },
    {
      name: "gpu",
      testMatch: gpuSpecs,
      dependencies: ["chromium"],
      workers: 1,
      use: chromium,
    },
  ],
});
