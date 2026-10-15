import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n/I18nProvider";
import { Shell } from "./Shell";

beforeEach(() => {
  vi.spyOn(navigator, "languages", "get").mockReturnValue(["en-US"]);
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

function renderShell() {
  render(
    <I18nProvider>
      <Shell />
    </I18nProvider>,
  );
  return userEvent.setup();
}

const region = (name: string) => screen.getByRole("region", { name });

describe("Shell", () => {
  it("lays out the title bar, viewport and labelled panel slots", () => {
    renderShell();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("heading", { level: 1, name: "BIMCraft" })).toBeInTheDocument();
    expect(within(banner).getByRole("navigation", { name: "Open views" })).toBeInTheDocument();
    expect(within(banner).getByRole("combobox", { name: "Language" })).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "Viewport" })).toBeInTheDocument();
    expect(region("Project")).toBeInTheDocument();
    expect(region("Properties")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Console" })).not.toBeInTheDocument();
  });

  it("offers a skip link to the viewport as the first stop", async () => {
    const user = renderShell();
    await user.tab();
    const skip = screen.getByRole("link", { name: "Skip to viewport" });
    expect(skip).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("main", { name: "Viewport" })).toHaveFocus();
  });

  it("moves focus between the panels with F6 and back with Shift+F6", async () => {
    const user = renderShell();
    const order = [
      screen.getByRole("banner"),
      region("Project"),
      screen.getByRole("main", { name: "Viewport" }),
      region("Properties"),
    ];
    for (const expected of [...order, order[0]]) {
      await user.keyboard("{F6}");
      expect(expected).toHaveFocus();
    }
    await user.keyboard("{Shift>}{F6}{/Shift}");
    expect(order[3]).toHaveFocus();
  });

  it("switches the right panel between Properties and History with the arrow keys", async () => {
    const user = renderShell();
    const properties = screen.getByRole("tab", { name: "Properties" });
    expect(properties).toHaveAttribute("aria-selected", "true");
    properties.focus();
    await user.keyboard("{ArrowRight}");
    const history = screen.getByRole("tab", { name: "History" });
    expect(history).toHaveFocus();
    expect(history).toHaveAttribute("aria-selected", "true");
    expect(region("History")).toBeInTheDocument();
    expect(screen.getByRole("tabpanel", { name: "History" })).toHaveTextContent("No changes yet");
    await user.keyboard("{ArrowLeft}");
    expect(properties).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}");
    expect(history).toHaveFocus();
    await user.keyboard("{Home}");
    expect(properties).toHaveFocus();
    await user.click(history);
    expect(history).toHaveAttribute("aria-selected", "true");
  });

  it("ignores other keys on the panel tabs", async () => {
    const user = renderShell();
    const properties = screen.getByRole("tab", { name: "Properties" });
    properties.focus();
    await user.keyboard("{ArrowDown}");
    expect(properties).toHaveFocus();
    expect(properties).toHaveAttribute("aria-selected", "true");
  });

  it("opens and closes the console from the title bar, the shortcut and its close button", async () => {
    const user = renderShell();
    const toggle = screen.getByRole("button", { name: "Console" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(region("Console")).toHaveFocus();
    await user.keyboard("{F6}");
    expect(screen.getByRole("banner")).toHaveFocus();

    await user.click(within(region("Console")).getByRole("button", { name: "Close console" }));
    expect(screen.queryByRole("region", { name: "Console" })).not.toBeInTheDocument();
    expect(screen.getByRole("main", { name: "Viewport" })).toHaveFocus();

    await user.keyboard("{Control>}`{/Control}");
    expect(region("Console")).toHaveFocus();
    await user.keyboard("{Control>}`{/Control}");
    expect(screen.queryByRole("region", { name: "Console" })).not.toBeInTheDocument();
  });

  it("follows the interface language", async () => {
    const user = renderShell();
    await user.selectOptions(screen.getByRole("combobox", { name: "Language" }), "fr");
    expect(region("Propriétés")).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "Vue" })).toBeInTheDocument();
  });
});
