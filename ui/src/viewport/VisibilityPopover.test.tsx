import { screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SINGLE_KEYS_STORAGE_KEY } from "../shell/ShellState";
import { fakeCore } from "../test-support/fakeCore";
import { fakeViewport } from "../test-support/fakeViewport";
import { renderWithCore } from "../test-support/render";
import { group, viewState } from "../test-support/viewState";
import type { ViewportModule } from "./types";
import { ViewportView } from "./ViewportView";

const loader = vi.hoisted(() => ({
  load: (): Promise<ViewportModule> => Promise.reject(new Error("no module")),
}));
vi.mock("./module", () => ({ loadViewportModule: () => loader.load() }));

const BLOCK = "1".padStart(32, "0");
const CATEGORIES = [group("Walls"), group("Slabs"), group("Spaces", { hidden: true })];
const DISCIPLINES = [group("Architecture"), group("Structure")];

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.removeItem(SINGLE_KEYS_STORAGE_KEY);
});

async function setUp(state = viewState({ categories: CATEGORIES, disciplines: DISCIPLINES })) {
  const core = fakeCore();
  core.blocks.add(BLOCK);
  core.setSceneRevision(3);
  const viewport = fakeViewport();
  loader.load = async () => viewport.module;
  const rendered = renderWithCore(<ViewportView />, core);
  await waitFor(() => expect(viewport.applied).toEqual([3]));
  await waitFor(() =>
    expect(document.getElementById("vp-status")).not.toHaveTextContent("Loading"),
  );
  await viewport.emit({ kind: "view_state", state });
  return { viewport, ...rendered };
}

const opener = () => screen.getByRole("button", { name: "Visibility" });
const popover = () => screen.getByRole("dialog", { name: "Visibility" });
const row = (name: string) =>
  within(popover()).getByRole("switch", { name: new RegExp(`^${name}`) });

describe("Visibility popover", () => {
  it("opens from the view controls with the first row focused, and Escape closes it and returns the focus", async () => {
    const { user } = await setUp();
    expect(screen.queryByRole("dialog", { name: "Visibility" })).not.toBeInTheDocument();
    expect(opener()).toHaveAttribute("aria-expanded", "false");
    await user.click(opener());
    expect(opener()).toHaveAttribute("aria-expanded", "true");
    expect(row("Walls")).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Visibility" })).not.toBeInTheDocument();
    expect(opener()).toHaveFocus();
  });

  it("closes on a press outside, but not on the opener's own press, which toggles", async () => {
    const { user } = await setUp();
    await user.click(opener());
    await user.click(opener());
    expect(screen.queryByRole("dialog", { name: "Visibility" })).not.toBeInTheDocument();
    await user.click(opener());
    await user.click(popover());
    expect(popover()).toBeInTheDocument();
    await user.click(screen.getByRole("application", { name: "3D view" }));
    expect(screen.queryByRole("dialog", { name: "Visibility" })).not.toBeInTheDocument();
  });

  it("lists the categories with an eye each and tags the hidden and the isolated ones", async () => {
    const { user } = await setUp(
      viewState({
        isolating: true,
        categories: [group("Columns", { isolated: true }), group("Spaces", { hidden: true })],
      }),
    );
    await user.click(opener());
    expect(row("Columns")).toBeChecked();
    expect(within(row("Columns")).getByText("Isolated")).toBeInTheDocument();
    expect(row("Spaces")).not.toBeChecked();
    expect(within(row("Spaces")).getByText("Hidden")).toBeInTheDocument();
  });

  it("moves between rows with the arrows, Home and End on a roving tab stop", async () => {
    const { user } = await setUp();
    await user.click(opener());
    expect(row("Walls")).toHaveAttribute("tabindex", "0");
    expect(row("Slabs")).toHaveAttribute("tabindex", "-1");
    await user.keyboard("{ArrowDown}");
    expect(row("Slabs")).toHaveFocus();
    expect(row("Slabs")).toHaveAttribute("tabindex", "0");
    expect(row("Walls")).toHaveAttribute("tabindex", "-1");
    await user.keyboard("{End}");
    expect(row("Spaces")).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(row("Spaces")).toHaveFocus();
    await user.keyboard("{Home}");
    expect(row("Walls")).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(row("Walls")).toHaveFocus();
  });

  it("hides with Space, shows a hidden one, and sends one command per press", async () => {
    const { viewport, user } = await setUp();
    await user.click(opener());
    await user.keyboard(" ");
    expect(viewport.commands).toEqual([
      { kind: "hide", target: { by: "category", names: ["Walls"] } },
    ]);
    await user.keyboard("{End} ");
    expect(viewport.commands.at(-1)).toEqual({
      kind: "show",
      target: { by: "category", names: ["Spaces"] },
    });
    await user.keyboard("{ArrowUp}{Enter}");
    expect(viewport.commands.at(-1)).toEqual({
      kind: "hide",
      target: { by: "category", names: ["Slabs"] },
    });
    expect(viewport.commands).toHaveLength(3);
  });

  it("isolates with Shift+Space, adds a second one, and ends the isolate when the last one is removed", async () => {
    const { viewport, user } = await setUp(
      viewState({
        categories: [group("Walls"), group("Slabs", { isolated: true })],
        isolating: true,
      }),
    );
    await user.click(opener());
    await user.keyboard("{Shift>} {/Shift}");
    expect(viewport.commands.at(-1)).toEqual({
      kind: "isolate",
      target: { by: "category", names: ["Slabs", "Walls"] },
    });
    await user.keyboard("{ArrowDown}{Shift>} {/Shift}");
    expect(viewport.commands.at(-1)).toEqual({ kind: "end_isolate" });
    await user.click(row("Walls"));
    expect(viewport.commands.at(-1)).toEqual({
      kind: "hide",
      target: { by: "category", names: ["Walls"] },
    });
  });

  it("isolates with Shift+click", async () => {
    const { viewport, user } = await setUp();
    await user.click(opener());
    await user.keyboard("{Shift>}");
    await user.click(row("Slabs"));
    await user.keyboard("{/Shift}");
    expect(viewport.commands.at(-1)).toEqual({
      kind: "isolate",
      target: { by: "category", names: ["Slabs"] },
    });
  });

  it("switches to the disciplines with the tabs and the arrow keys", async () => {
    const { viewport, user } = await setUp();
    await user.click(opener());
    const categories = screen.getByRole("tab", { name: "Category" });
    expect(categories).toHaveAttribute("aria-selected", "true");
    categories.focus();
    await user.keyboard("{ArrowRight}");
    const disciplines = screen.getByRole("tab", { name: "Discipline" });
    expect(disciplines).toHaveFocus();
    expect(disciplines).toHaveAttribute("aria-selected", "true");
    expect(within(popover()).queryByRole("switch", { name: /^Walls/ })).not.toBeInTheDocument();
    await user.keyboard("{Tab}");
    expect(row("Architecture")).toHaveFocus();
    await user.keyboard(" ");
    expect(viewport.commands.at(-1)).toEqual({
      kind: "hide",
      target: { by: "discipline", names: ["Architecture"] },
    });
    await user.click(disciplines);
    await user.keyboard("{ArrowLeft}");
    expect(categories).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(disciplines).toHaveFocus();
    await user.keyboard("{Home}");
    expect(disciplines).toHaveFocus();
  });

  it("says when the model has nothing to list", async () => {
    const { user } = await setUp(viewState());
    await user.click(opener());
    expect(screen.getByText("The model has no categories yet.")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Discipline" }));
    expect(screen.getByText("The model has no disciplines yet.")).toBeInTheDocument();
  });

  it("isolates and hides the selection from its buttons, and asks for a selection first", async () => {
    const { viewport, user } = await setUp();
    await user.click(opener());
    const isolate = within(popover()).getByRole("button", { name: /^Isolate selection/ });
    expect(isolate).toHaveAttribute("aria-disabled", "true");
    await user.click(isolate);
    expect(screen.getByText("Select something to hide or isolate")).toBeVisible();
    expect(viewport.commands).toEqual([]);

    await viewport.emit({ kind: "selection", ids: [BLOCK] });
    expect(isolate).toHaveAttribute("aria-disabled", "false");
    await user.click(isolate);
    expect(viewport.commands.at(-1)).toEqual({ kind: "isolate", target: { by: "selection" } });
    await user.click(within(popover()).getByRole("button", { name: /^Hide/ }));
    expect(viewport.commands.at(-1)).toEqual({ kind: "hide", target: { by: "selection" } });
  });

  it("names the shortcuts on the buttons while single keys are on, and not when they are off", async () => {
    const first = await setUp();
    await first.user.click(opener());
    expect(within(popover()).getByRole("button", { name: /^Isolate selection/ })).toHaveAttribute(
      "aria-keyshortcuts",
      "I",
    );
    first.unmount();

    localStorage.setItem(SINGLE_KEYS_STORAGE_KEY, "off");
    const second = await setUp();
    await second.user.click(opener());
    expect(within(popover()).getByRole("button", { name: "Hide" })).not.toHaveAttribute(
      "aria-keyshortcuts",
    );
  });

  it("is not offered when the view is not running", async () => {
    const { viewport } = await setUp();
    await viewport.emit({ kind: "error", key: "device_lost" });
    await waitFor(() => expect(opener()).toBeDisabled());
  });
});
