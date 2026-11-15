import { screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fakeCore } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { EmptyCard, UnavailableCard } from "./StateCards";

const opener = vi.hoisted(() => ({ inDesktopWindow: vi.fn(), openInSystemBrowser: vi.fn() }));
vi.mock("../api/opener", () => opener);

beforeEach(() => {
  opener.inDesktopWindow.mockReset();
  opener.openInSystemBrowser.mockReset();
});

describe("guide links", () => {
  it("keep a real link that a browser follows by itself", async () => {
    opener.inDesktopWindow.mockReturnValue(false);
    const { user } = renderWithCore(<UnavailableCard />, fakeCore());
    const link = screen.getByRole("link", { name: "Open the troubleshooting guide" });
    expect(link).toHaveAttribute("href", expect.stringContaining("#troubleshooting"));
    expect(link).toHaveAttribute("target", "_blank");
    await user.click(link);
    expect(opener.openInSystemBrowser).not.toHaveBeenCalled();
  });

  it("open in the system browser in the desktop window", async () => {
    opener.inDesktopWindow.mockReturnValue(true);
    opener.openInSystemBrowser.mockResolvedValue(undefined);
    const { user } = renderWithCore(<EmptyCard onPlaceBlock={() => undefined} />, fakeCore());
    await user.click(screen.getByRole("link", { name: /Guide: place a block/ }));
    expect(opener.openInSystemBrowser).toHaveBeenCalledWith(
      expect.stringContaining("docs/user/place-a-block.md"),
    );
    expect(screen.queryByText(/could not be opened/)).not.toBeInTheDocument();
  });

  it("tell the address in a message when they cannot be opened", async () => {
    opener.inDesktopWindow.mockReturnValue(true);
    opener.openInSystemBrowser.mockRejectedValue("refused");
    const { user } = renderWithCore(<UnavailableCard />, fakeCore());
    await user.click(screen.getByRole("link", { name: "Open the troubleshooting guide" }));
    await waitFor(() =>
      expect(
        screen.getByText(/The guide could not be opened\. Its address is https:\/\/github\.com\//),
      ).toBeVisible(),
    );
  });
});
