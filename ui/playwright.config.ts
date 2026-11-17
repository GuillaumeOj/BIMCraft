import { defineConfig, devices } from "@playwright/test";
import { browserLogFile, outputDir } from "./e2e/support/browser-log";

// WebGPU for the 3D viewport specs. Chromium's Dawn uses Metal on macOS, so it needs no adapter flag.
// Elsewhere it is told to use SwiftShader, a software Vulkan, as there is no graphics card on CI.
const software = process.platform !== "darwin";
const webgpuArgs = [
  "--enable-unsafe-webgpu",
  "--ignore-gpu-blocklist",
  ...(software
    ? [
        "--enable-features=Vulkan",
        "--use-webgpu-adapter=swiftshader",
        "--enable-unsafe-swiftshader",
        "--use-angle=swiftshader",
      ]
    : []),
  // Chromium's own log, GPU process included, goes to a file: a failed viewport test attaches its
  // GPU lines (e2e/support/viewport.ts), and the job's output stays quiet.
  // `--enable-logging` without a value logs to a file; `--log-file` says which.
  "--enable-logging",
  `--log-file=${browserLogFile}`,
];

const { CI } = process.env;

// Every page mounts the 3D view, and outside macOS each browser renders it on the CPU. The specs
// that draw the most then run alone, one at a time, after the others: two software renderers at
// once starve the GPU processes until a browser loses its WebGPU context. A failure in the first
// project skips the second for that run; that is the price of never running them side by side.
// On macOS, which draws with Metal, they keep no such order, so one GPU spec runs on its own.
// A new spec lands in the parallel project unless it is listed here. The WebGPU risk check
// (webgpu.spec.ts) draws one small triangle and stays in the first project, so that a browser
// without an adapter is named before anything else fails.
const gpuSpecs = ["**/viewport.spec.ts", "**/section-visibility.spec.ts"];

// `channel: "chromium"` selects the full browser in its new headless mode: the old headless shell has no WebGPU.
const chromium = {
  ...devices["Desktop Chrome"],
  channel: "chromium",
  launchOptions: { args: webgpuArgs },
};

export default defineConfig({
  testDir: "e2e",
  // Where Chromium's logs go as well (e2e/support/browser-log.ts).
  outputDir,
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
      dependencies: software ? ["chromium"] : [],
      workers: 1,
      use: chromium,
    },
  ],
});
