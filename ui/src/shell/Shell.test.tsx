import { screen, within } from "@testing-library/react";
import { useLayoutEffect, useRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { useGeoref } from "../georef/GeorefContext";
import { fakeCore } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { Shell } from "./Shell";

// The 3D view stays loading: its own tests cover it (viewport/ViewportView.test.tsx).
vi.mock("../viewport/module", () => ({ loadViewportModule: () => new Promise(() => undefined) }));

// jsdom reports an English system language (navigator.languages is ["en-US", "en"]).
function renderShell() {
  return renderWithCore(<Shell />, fakeCore()).user;
}

/**
 * Presses `key` (with Ctrl+Shift) in the very commit that closes the Project location dialog:
 * layout effects run before the passive ones, as a key does between a commit and its paint.
 */
function PressWhenDialogCloses({ chord }: { chord: string }) {
  const { dialogOpen } = useGeoref();
  const wasOpen = useRef(false);
  useLayoutEffect(() => {
    if (wasOpen.current && !dialogOpen) {
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: chord, ctrlKey: true, shiftKey: true }),
      );
    }
    wasOpen.current = dialogOpen;
  }, [dialogOpen, chord]);
  return null;
}

const region = (name: string) => screen.getByRole("region", { name });

describe("Shell", () => {
  it("lays out the title bar, viewport and labelled panel slots", () => {
    renderShell();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("heading", { level: 1, name: "Fabrilo" })).toBeInTheDocument();
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

  it("goes to the last panel on Shift+F6 when focus is outside every panel", async () => {
    const user = renderShell();
    await user.tab();
    expect(screen.getByRole("link", { name: "Skip to viewport" })).toHaveFocus();
    await user.keyboard("{Shift>}{F6}{/Shift}");
    expect(region("Properties")).toHaveFocus();
  });

  it("does not toggle the console again while the shortcut is held", async () => {
    const user = renderShell();
    await user.keyboard("{Control>}`{/Control}");
    expect(region("Console")).toBeInTheDocument();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "`", ctrlKey: true, repeat: true }));
    expect(region("Console")).toBeInTheDocument();
  });

  it("switches the right panel between its four tabs with the arrow keys", async () => {
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
    await user.keyboard("{ArrowRight}");
    const coordinates = screen.getByRole("tab", { name: "Coordinates" });
    expect(coordinates).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    const view = screen.getByRole("tab", { name: "View" });
    expect(view).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(properties).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(view).toHaveFocus();
    await user.keyboard("{Home}");
    expect(properties).toHaveFocus();
    await user.keyboard("{End}");
    expect(view).toHaveFocus();
    await user.click(history);
    expect(history).toHaveAttribute("aria-selected", "true");
  });

  it("shows the Coordinates panel with Ctrl+Shift+K", async () => {
    const user = renderShell();
    await user.keyboard("{Control>}{Shift>}K{/Shift}{/Control}");
    const tab = screen.getByRole("tab", { name: "Coordinates" });
    expect(tab).toHaveFocus();
    expect(tab).toHaveAttribute("aria-selected", "true");
    expect(
      await screen.findByRole("button", { name: "Set project location…" }),
    ).toBeInTheDocument();
  });

  it("opens the Project location dialog from the title bar and with Ctrl+Shift+L", async () => {
    const user = renderShell();
    await user.click(screen.getByRole("button", { name: "Project location…" }));
    expect(screen.getByRole("dialog", { name: "Project location" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.keyboard("{Control>}{Shift>}L{/Shift}{/Control}");
    expect(screen.getByRole("dialog", { name: "Project location" })).toBeInTheDocument();
  });

  it("opens the Materials dialog from the title bar and with Ctrl+Shift+M, and returns focus", async () => {
    const user = renderShell();
    const button = screen.getByRole("button", { name: "Materials…" });
    expect(button).toHaveAttribute("aria-keyshortcuts", "Control+Shift+M Meta+Shift+M");
    await user.click(button);
    expect(await screen.findByRole("dialog", { name: "Materials" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(button).toHaveFocus();
    await user.keyboard("{Meta>}{Shift>}M{/Shift}{/Meta}");
    expect(await screen.findByRole("dialog", { name: "Materials" })).toBeInTheDocument();
  });

  it("lets no shortcut open a second dialog over the first", async () => {
    const user = renderShell();
    await user.click(screen.getByRole("button", { name: "Materials…" }));
    await screen.findByRole("dialog", { name: "Materials" });
    await user.keyboard("{Control>}{Shift>}L{/Shift}{/Control}");
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Project location…" }));
    await user.keyboard("{Control>}{Shift>}M{/Shift}{/Control}");
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(screen.getByRole("dialog", { name: "Project location" })).toBeInTheDocument();
  });

  it("opens the Build-ups dialog from the title bar and with Ctrl+Shift+B", async () => {
    const user = renderShell();
    const button = screen.getByRole("button", { name: "Build-ups…" });
    expect(button).toHaveAttribute("aria-keyshortcuts", "Control+Shift+B Meta+Shift+B");
    await user.click(button);
    expect(await screen.findByRole("dialog", { name: "Build-ups" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(button).toHaveFocus();
    await user.keyboard("{Control>}{Shift>}B{/Shift}{/Control}");
    expect(await screen.findByRole("dialog", { name: "Build-ups" })).toBeInTheDocument();
  });

  it("goes from the in-use message of a material to the build-ups", async () => {
    const core = fakeCore();
    core.materials.seed("en1992-1-1/C25/30");
    core.materials.setUsedBy("C25/30", 2);
    const user = renderWithCore(<Shell />, core).user;
    await user.click(screen.getByRole("button", { name: "Materials…" }));
    await user.click(await screen.findByRole("button", { name: "Actions for C25/30" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete…" }));
    await user.click(await screen.findByRole("button", { name: "Show build-ups…" }));
    expect(await screen.findByRole("dialog", { name: "Build-ups" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Materials" })).not.toBeInTheDocument();
  });

  it("keeps the keyboard in the Project location dialog: F6 and Ctrl+Shift+K do nothing", async () => {
    const user = renderShell();
    await user.click(screen.getByRole("button", { name: "Project location…" }));
    const dialog = screen.getByRole("dialog", { name: "Project location" });
    await user.keyboard("{F6}");
    await user.keyboard("{Control>}{Shift>}K{/Shift}{/Control}");
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    expect(screen.getByRole("tab", { name: "Properties" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("reads the modal state when a key arrives in the commit that closes a dialog: Ctrl+Shift+L", async () => {
    const { user } = renderWithCore(
      <>
        <Shell />
        <PressWhenDialogCloses chord="L" />
      </>,
      fakeCore(),
    );
    await user.click(screen.getByRole("button", { name: "Project location…" }));
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Project location" })).toBeInTheDocument();
  });

  it("reads the modal state when a key arrives in the commit that closes a dialog: Ctrl+Shift+K", async () => {
    const { user } = renderWithCore(
      <>
        <Shell />
        <PressWhenDialogCloses chord="K" />
      </>,
      fakeCore(),
    );
    await user.click(screen.getByRole("button", { name: "Project location…" }));
    await user.keyboard("{Escape}");
    expect(screen.getByRole("tab", { name: "Coordinates" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
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
