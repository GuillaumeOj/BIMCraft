import { beforeEach, describe, expect, it, vi } from "vitest";
import { openGuide } from "./guide";

const api = vi.hoisted(() => ({ inDesktopWindow: vi.fn(), openInSystemBrowser: vi.fn() }));
vi.mock("../api", () => api);

const URL = "https://github.com/GuillaumeOj/BIMCraft/blob/main/docs/user/place-a-block.md";

beforeEach(() => {
  api.inDesktopWindow.mockReset();
  api.openInSystemBrowser.mockReset();
});

describe("openGuide", () => {
  it("leaves the link alone in a browser", () => {
    api.inDesktopWindow.mockReturnValue(false);
    const event = { preventDefault: vi.fn() };
    const failed = vi.fn();
    openGuide(event, URL, failed);
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(api.openInSystemBrowser).not.toHaveBeenCalled();
    expect(failed).not.toHaveBeenCalled();
  });

  it("hands the click to the system browser in the desktop window", async () => {
    api.inDesktopWindow.mockReturnValue(true);
    api.openInSystemBrowser.mockResolvedValue(undefined);
    const event = { preventDefault: vi.fn() };
    const failed = vi.fn();
    openGuide(event, URL, failed);
    expect(event.preventDefault).toHaveBeenCalled();
    expect(api.openInSystemBrowser).toHaveBeenCalledWith(URL);
    await Promise.resolve();
    expect(failed).not.toHaveBeenCalled();
  });

  it("reports the address when the system browser cannot be opened", async () => {
    api.inDesktopWindow.mockReturnValue(true);
    api.openInSystemBrowser.mockRejectedValue("refused");
    const failed = vi.fn();
    openGuide({ preventDefault: vi.fn() }, URL, failed);
    await vi.waitFor(() => expect(failed).toHaveBeenCalledWith(URL));
  });
});
