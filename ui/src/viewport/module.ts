import type { ViewportHandle, ViewportModule } from "./types";

let loading: Promise<ViewportModule> | undefined;

/**
 * Loads the viewport's WebAssembly module (ADR 0025) the first time it is asked for, and the same
 * module after that. This is the only place that imports `./pkg/`: the dynamic import keeps the
 * module (about 250 kB gzipped) out of the start-up bundle until a viewport mounts.
 */
export function loadViewportModule(): Promise<ViewportModule> {
  loading ??= load().catch((error: unknown) => {
    // A failed download may succeed on the next mount.
    loading = undefined;
    throw error;
  });
  return loading;
}

async function load(): Promise<ViewportModule> {
  const pkg = await import("./pkg/viewport.js");
  await pkg.default();
  return {
    webgpuAvailable: () => pkg.webgpuAvailable(),
    async create(canvas, options) {
      const viewport = await pkg.Viewport.create(canvas, options);
      const handle: ViewportHandle = {
        free: () => viewport.free(),
        applyScene: (bytes) => viewport.applyScene(bytes),
        // The bindings give `undefined` for Rust's `None`.
        revision: () => viewport.revision() ?? null,
        resize: (width, height, ratio) => viewport.resize(width, height, ratio),
        setTheme: (theme) => viewport.setTheme(theme),
        command: (command) => viewport.command(command),
        setTool: (tool) => viewport.setTool(tool),
        onEvent: (listener) => viewport.onEvent(listener),
      };
      return handle;
    },
  };
}
