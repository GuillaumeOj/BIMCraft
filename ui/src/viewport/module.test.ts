import { beforeEach, describe, expect, it, vi } from "vitest";

// The generated bindings (scripts/build-viewport.sh), mocked: Vitest builds no Rust.
const pkg = vi.hoisted(() => {
  const viewport = {
    free: vi.fn(),
    applyScene: vi.fn(() => 4),
    revision: vi.fn((): number | undefined => undefined),
    resize: vi.fn(),
    setTheme: vi.fn(),
    command: vi.fn(),
    setTool: vi.fn(),
    onEvent: vi.fn(),
  };
  return {
    viewport,
    init: vi.fn(async () => undefined),
    webgpuAvailable: vi.fn(async () => true),
    create: vi.fn(async () => viewport),
  };
});
vi.mock("./pkg/viewport.js", () => ({
  default: pkg.init,
  webgpuAvailable: pkg.webgpuAvailable,
  Viewport: { create: pkg.create },
}));

beforeEach(() => {
  vi.resetModules();
  pkg.init.mockClear();
});

const load = async () => (await import("./module")).loadViewportModule();

describe("loadViewportModule", () => {
  it("initialises the module once, however many viewports mount", async () => {
    const { loadViewportModule } = await import("./module");
    const [first, second] = await Promise.all([loadViewportModule(), loadViewportModule()]);
    expect(first).toBe(second);
    expect(pkg.init).toHaveBeenCalledTimes(1);
  });

  it("loads again after a failed load", async () => {
    pkg.init.mockRejectedValueOnce(new Error("offline"));
    const { loadViewportModule } = await import("./module");
    await expect(loadViewportModule()).rejects.toThrow("offline");
    await expect(loadViewportModule()).resolves.toBeDefined();
    expect(pkg.init).toHaveBeenCalledTimes(2);
  });

  it("passes every call through to the bindings, with null for no revision", async () => {
    const module = await load();
    await expect(module.webgpuAvailable()).resolves.toBe(true);
    const canvas = document.createElement("canvas");
    const theme = {
      background: "#000000",
      selection: "#000000",
      hover: "#000000",
      snap: "#000000",
      preview: "#000000",
      cut: "#000000",
    };
    const handle = await module.create(canvas, { theme });
    expect(pkg.create).toHaveBeenCalledWith(canvas, { theme });
    const { viewport } = pkg;
    expect(handle.revision()).toBeNull();
    viewport.revision.mockReturnValueOnce(3);
    expect(handle.revision()).toBe(3);
    const bytes = new Uint8Array([1]);
    expect(handle.applyScene(bytes)).toBe(4);
    handle.resize(10, 20, 2);
    handle.setTheme(theme);
    handle.command({ kind: "zoom_fit" });
    handle.setTool("move");
    const listener = () => undefined;
    handle.onEvent(listener);
    handle.free();
    expect(viewport.applyScene).toHaveBeenCalledWith(bytes);
    expect(viewport.resize).toHaveBeenCalledWith(10, 20, 2);
    expect(viewport.setTheme).toHaveBeenCalledWith(theme);
    expect(viewport.command).toHaveBeenCalledWith({ kind: "zoom_fit" });
    expect(viewport.setTool).toHaveBeenCalledWith("move");
    expect(viewport.onEvent).toHaveBeenCalledWith(listener);
    expect(viewport.free).toHaveBeenCalled();
  });
});
