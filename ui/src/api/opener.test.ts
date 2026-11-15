import { beforeEach, describe, expect, it, vi } from "vitest";
import { inDesktopWindow, openInSystemBrowser } from "./opener";

const tauri = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke, isTauri: tauri.isTauri }));

beforeEach(() => {
  tauri.invoke.mockReset();
  tauri.isTauri.mockReset();
});

describe("inDesktopWindow", () => {
  it("tells the desktop window from a browser", () => {
    tauri.isTauri.mockReturnValue(true);
    expect(inDesktopWindow()).toBe(true);
    tauri.isTauri.mockReturnValue(false);
    expect(inDesktopWindow()).toBe(false);
  });
});

describe("openInSystemBrowser", () => {
  it("asks the opener plugin to open the address", async () => {
    tauri.invoke.mockResolvedValue(undefined);
    await openInSystemBrowser("https://example.test/guide.md");
    expect(tauri.invoke).toHaveBeenCalledWith("plugin:opener|open_url", {
      url: "https://example.test/guide.md",
    });
  });

  it("rejects when the plugin refuses", async () => {
    tauri.invoke.mockRejectedValue("not allowed");
    await expect(openInSystemBrowser("https://elsewhere.test")).rejects.toBe("not allowed");
  });
});
