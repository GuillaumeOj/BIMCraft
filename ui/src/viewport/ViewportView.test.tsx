import { act, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../api";
import { OVERRIDE_STORAGE_KEY } from "../i18n/I18nProvider";
import { SINGLE_KEYS_STORAGE_KEY } from "../shell/ShellState";
import { fakeCore } from "../test-support/fakeCore";
import { fakeViewport } from "../test-support/fakeViewport";
import { renderWithCore } from "../test-support/render";
import { group, viewState } from "../test-support/viewState";
import { Dialog } from "../ui/Dialog";
import { HINT_STORAGE_KEY } from "./HintCard";
import type { ViewportEvent, ViewportHandle, ViewportModule } from "./types";
import { ViewportView } from "./ViewportView";

const loader = vi.hoisted(() => ({
  load: (): Promise<ViewportModule> => Promise.reject(new Error("no module")),
}));
vi.mock("./module", () => ({ loadViewportModule: () => loader.load() }));

const BLOCK = "1".padStart(32, "0");
const OTHER = "2".padStart(32, "0");

let resizeCallbacks: (() => void)[] = [];

beforeEach(() => {
  resizeCallbacks = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(callback: () => void) {
        resizeCallbacks.push(callback);
      }
      observe() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

/** Renders the view on a fake core holding `blocks`, with a fake viewport module. */
async function setUp(options: { blocks?: string[]; webgpu?: boolean; beside?: ReactNode } = {}) {
  const core = fakeCore();
  for (const id of options.blocks ?? [BLOCK]) core.blocks.add(id);
  core.setSceneRevision(3);
  const viewport = fakeViewport({ webgpu: options.webgpu ?? true });
  loader.load = async () => viewport.module;
  const { user, unmount } = renderWithCore(
    <>
      <ViewportView />
      {options.beside}
    </>,
    core,
  );
  return { core, viewport, user, unmount };
}

const status = () => document.getElementById("vp-status");
const liveRegion = () =>
  screen.getAllByRole("status").find((element) => element.getAttribute("aria-live") === "polite");
const tool = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}`) });

async function ready(options: Parameters<typeof setUp>[0] = {}) {
  const setup = await setUp(options);
  await waitFor(() => expect(setup.viewport.applied).toEqual([3]));
  await waitFor(() => expect(status()).not.toHaveTextContent("Loading"));
  // The viewport counts what it draws, by category; an empty model has none.
  if ((options.blocks ?? [BLOCK]).length > 0)
    setup.viewport.emit({ kind: "view_state", state: viewState() });
  return setup;
}

describe("ViewportView", () => {
  it("mounts a labelled, focusable canvas, creates the viewport with the theme and feeds it the scene", async () => {
    const { core, viewport } = await ready();
    const canvas = screen.getByRole("application", { name: "3D view" });
    expect(canvas.tagName).toBe("CANVAS");
    expect(canvas).toHaveAttribute("tabindex", "0");
    expect(viewport.create).toHaveBeenCalledWith(canvas, {
      theme: {
        background: "#f4f5f6",
        selection: "#0f6b5e",
        hover: "#b25e00",
        snap: "#b4137b",
        preview: "#5c6470",
        cut: "#b4410a",
      },
    });
    expect(core.sceneCalls).toEqual([null]);
    expect(status()).toHaveTextContent("No selection · Snap on");
    expect(canvas).toHaveAccessibleDescription("No selection · Snap on");
  });

  it("follows the canvas size and the device pixel ratio", async () => {
    const { viewport } = await ready();
    vi.stubGlobal("devicePixelRatio", 2);
    act(() => {
      for (const callback of resizeCallbacks) callback();
    });
    // jsdom lays nothing out: the size is clamped to one pixel.
    expect(viewport.sizes).toEqual([[1, 1, 2]]);
  });

  it("sends the theme again when the system switches between light and dark", async () => {
    const listeners: (() => void)[] = [];
    vi.stubGlobal("matchMedia", () => ({
      addEventListener: (_: string, listener: () => void) => listeners.push(listener),
      removeEventListener: vi.fn(),
    }));
    const { viewport } = await ready();
    act(() => {
      for (const listener of listeners) listener();
    });
    expect(viewport.themes).toHaveLength(2);
  });

  it("fetches a delta since the held revision after each change to the model", async () => {
    const { core, viewport } = await ready();
    core.setSceneRevision(4);
    act(() => core.notifyChange());
    await waitFor(() => expect(viewport.applied).toEqual([3, 4]));
    expect(core.sceneCalls).toEqual([null, 3]);
  });

  it("shows the empty state on an empty model, and Place a block arms the block tool", async () => {
    const { viewport, user } = await ready({ blocks: [] });
    expect(await screen.findByRole("heading", { name: "Nothing to see yet" })).toBeVisible();
    expect(status()).toHaveTextContent("No elements");
    expect(screen.getByRole("button", { name: "Zoom fit" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(screen.getByRole("link", { name: /Guide: place a block/ })).toHaveAttribute(
      "href",
      expect.stringContaining("place-a-block"),
    );
    await user.click(screen.getByRole("button", { name: "Place a block" }));
    expect(viewport.tools).toEqual(["place_block"]);
  });

  it("explains that the 3D view is unavailable without WebGPU, and disables the tools", async () => {
    const { viewport } = await setUp({ webgpu: false });
    expect(
      await screen.findByRole("heading", { name: "The 3D view isn't available on this computer" }),
    ).toBeVisible();
    expect(screen.getByRole("link", { name: "Open the troubleshooting guide" })).toHaveAttribute(
      "href",
      expect.stringContaining("#troubleshooting"),
    );
    expect(status()).toHaveTextContent("3D view unavailable");
    expect(tool("Select")).toBeDisabled();
    expect(viewport.create).not.toHaveBeenCalled();
  });

  it("stops on a device loss, focuses Reload, and starts a new viewport from it", async () => {
    const { core, viewport, user } = await ready();
    screen.getByRole("application", { name: "3D view" }).focus();
    viewport.emit({ kind: "error", key: "device_lost" });
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The 3D view stopped");
    expect(alert).toHaveTextContent("The graphics card stopped responding. Your model is safe.");
    expect(alert).not.toHaveTextContent("restart Fabrilo");
    expect(screen.getByRole("button", { name: "Reload view" })).toHaveFocus();
    expect(viewport.free).toHaveBeenCalled();
    expect(status()).toHaveTextContent("3D view stopped");
    expect(tool("Select")).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Reload view" }));
    await waitFor(() => expect(viewport.create).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(core.sceneCalls).toEqual([null, 3]));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("ignores what a stopped viewport reports after Reload: only the live one can stop the view", async () => {
    const core = fakeCore();
    core.blocks.add(BLOCK);
    core.setSceneRevision(3);
    const first = fakeViewport();
    const second = fakeViewport();
    const modules = [first.module, second.module];
    loader.load = async () => modules.shift() ?? second.module;
    const { user } = renderWithCore(<ViewportView />, core);
    await waitFor(() => expect(first.applied).toEqual([3]));

    first.emit({ kind: "error", key: "device_lost" });
    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: "Reload view" }));
    await waitFor(() => expect(second.applied).toEqual([3]));

    // The first viewport reports once more, late: the healthy second one must go on.
    first.emit({ kind: "error", key: "device_lost" });
    first.emit({ kind: "selection", ids: [BLOCK] });
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(second.free).not.toHaveBeenCalled();
    expect(status()).not.toHaveTextContent("1 element selected");
  });

  it("adds the restart advice when the view stops twice within 30 s", async () => {
    const { viewport, user } = await ready();
    viewport.emit({ kind: "error", key: "out_of_memory" });
    await screen.findByRole("alert");
    // The focus was not in the view: it stays where it was.
    expect(screen.getByRole("button", { name: "Reload view" })).not.toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Reload view" }));
    await waitFor(() => expect(viewport.create).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    viewport.emit({ kind: "error", key: "out_of_memory" });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "If it keeps happening, restart Fabrilo.",
    );
  });

  it("stops when the viewport rejects the scene", async () => {
    const core = fakeCore();
    const viewport = fakeViewport();
    viewport.rejectScenes();
    loader.load = async () => viewport.module;
    renderWithCore(<ViewportView />, core);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The view couldn't read the model data.",
    );
  });

  it("stops, without blaming the model data, when the scene cannot be fetched", async () => {
    const core = fakeCore();
    core.answerScene(() => Promise.reject(new Error("document closed")));
    const viewport = fakeViewport();
    loader.load = async () => viewport.module;
    renderWithCore(<ViewportView />, core);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The view lost its link to the model.");
    expect(alert).not.toHaveTextContent("couldn't read the model data");
  });

  it("offers Cancel when the first scene takes longer than 10 s", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const core = fakeCore();
    core.answerScene(() => new Promise(() => undefined));
    const viewport = fakeViewport();
    loader.load = async () => viewport.module;
    const { user } = renderWithCore(<ViewportView />, core);
    await waitFor(() => expect(viewport.create).toHaveBeenCalled());
    await waitFor(() => expect(core.sceneCalls).toEqual([null]));
    expect(screen.getByText("Loading the 3D view…")).toBeVisible();
    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.getByText("Still working…")).toBeVisible();
    expect(screen.getByText("Loading the 3D view…")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Loading was cancelled.");
    expect(viewport.free).toHaveBeenCalled();
  });

  it("stops when the module cannot be loaded, or the viewport cannot be created", async () => {
    loader.load = () => Promise.reject(new Error("offline"));
    const { unmount } = renderWithCore(<ViewportView />, fakeCore());
    expect(await screen.findByRole("alert")).toHaveTextContent("The 3D view could not start.");
    unmount();

    const viewport = fakeViewport();
    viewport.create.mockRejectedValueOnce(new Error("no adapter"));
    loader.load = async () => viewport.module;
    renderWithCore(<ViewportView />, fakeCore());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The graphics card stopped responding.",
    );
  });

  it("copies the details of a stop without model content", async () => {
    const writeText = vi.fn(async () => undefined);
    const { viewport, user } = await ready();
    // After user-event's setup, which installs a clipboard of its own.
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    viewport.emit({ kind: "error", key: "device_lost" });
    await user.click(await screen.findByRole("button", { name: "Copy details" }));
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining("device_lost"));
    expect(await screen.findByText("Details copied")).toBeVisible();
    Reflect.deleteProperty(navigator, "clipboard");
  });

  it("says it is still working after 10 s, and Cancel stops loading", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    loader.load = () => new Promise(() => undefined);
    const { user } = renderWithCore(<ViewportView />, fakeCore());
    expect(screen.getByText("Loading the 3D view…")).toBeVisible();
    expect(status()).toHaveTextContent("Loading…");
    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.getByText("Still working…")).toBeVisible();
    expect(screen.getByText("Loading the 3D view…")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Loading was cancelled.");
  });

  it("frees the viewport when it unmounts", async () => {
    const { viewport, unmount } = await ready();
    unmount();
    expect(viewport.free).toHaveBeenCalled();
  });

  it("frees a viewport created after the view unmounted", async () => {
    const viewport = fakeViewport();
    let created: (handle: ViewportHandle) => void = () => undefined;
    viewport.create.mockImplementationOnce(
      () =>
        new Promise<ViewportHandle>((resolve) => {
          created = resolve;
        }),
    );
    loader.load = async () => viewport.module;
    const { unmount } = renderWithCore(<ViewportView />, fakeCore());
    await waitFor(() => expect(viewport.create).toHaveBeenCalled());
    unmount();
    await act(async () => created(viewport.handle));
    expect(viewport.free).toHaveBeenCalled();
  });
});

describe("Esc in the step hints", () => {
  const hint = () => document.querySelector(".vp-hint");
  const label = (
    key: "move.distance" | "place.size" | "place.depth" | "place.height",
    typed: string | null,
  ): ViewportEvent => ({
    kind: "preview_label",
    label: { at: [10, 10], key, values: { distance: 1, width: 1, depth: 1, height: 1 }, typed },
  });

  it.each([
    ["first_corner", "Esc: cancel"],
    ["base_point", "Esc: cancel"],
    ["second_corner", "Esc: previous step"],
    ["height", "Esc: previous step"],
    ["target_point", "Esc: previous step"],
  ] as const)("says what Esc does at %s", async (step, text) => {
    const { viewport } = await ready();
    viewport.emit({ kind: "tool_state", tool: "move", step });
    expect(hint()).toHaveTextContent(new RegExp(`${text}$`));
  });

  it("says what Esc does at the depth step", async () => {
    const { viewport } = await ready();
    viewport.emit({ kind: "tool_state", tool: "place_block", step: "second_corner" });
    viewport.emit(label("place.depth", null));
    expect(hint()).toHaveTextContent(/type the depth, Enter · Esc: previous step$/);
  });

  it.each([
    ["second_corner", "place.size"],
    ["second_corner", "place.depth"],
    ["height", "place.height"],
    ["target_point", "move.distance"],
  ] as const)("says that Esc clears the value while one is typed at %s", async (step, key) => {
    const { viewport } = await ready();
    viewport.emit({ kind: "tool_state", tool: "move", step });
    viewport.emit(label(key, "2"));
    expect(hint()).toHaveTextContent(/Esc: clear the value$/);
    expect(hint()).not.toHaveTextContent(/previous step|Esc: cancel/);
    // The first Esc cleared it: the step's own wording is back.
    viewport.emit(label(key, null));
    expect(hint()).toHaveTextContent(/Esc: previous step$/);
  });

  it("is worded in French too", async () => {
    localStorage.setItem(OVERRIDE_STORAGE_KEY, "fr");
    const { viewport } = await ready();
    viewport.emit({ kind: "tool_state", tool: "move", step: "target_point" });
    expect(hint()).toHaveTextContent(/Échap : étape précédente$/);
    viewport.emit(label("move.distance", "2"));
    expect(hint()).toHaveTextContent(/Échap : effacer la valeur$/);
    viewport.emit({ kind: "tool_state", tool: "move", step: "base_point" });
    viewport.emit(label("move.distance", null));
    expect(hint()).toHaveTextContent(/Échap : annuler$/);
    localStorage.removeItem(OVERRIDE_STORAGE_KEY);
  });
});

describe("ViewportView tools", () => {
  it("sets the tool from the tool bar and shows the step's hint", async () => {
    const { viewport, user } = await ready();
    expect(tool("Select")).toHaveAttribute("aria-pressed", "true");
    expect(tool("Block")).toHaveAttribute("aria-keyshortcuts", "B");
    expect(tool("Block")).toHaveAttribute("title", "Block (B)");
    await user.click(tool("Block"));
    expect(viewport.tools).toEqual(["place_block"]);
    expect(tool("Block")).toHaveAttribute("aria-pressed", "true");
    viewport.emit({ kind: "tool_state", tool: "place_block", step: "second_corner" });
    expect(screen.getByText(/click the second corner or type the width/)).toBeVisible();
    // The width is confirmed: the hint and the typed field name the depth.
    viewport.emit({
      kind: "preview_label",
      label: { at: [10, 10], key: "place.depth", values: { width: 6, depth: 0 }, typed: "4" },
    });
    expect(screen.getByText(/type the depth/)).toBeVisible();
    expect(document.querySelector(".vp-chip")).toHaveTextContent("Depth 4 m ↵");
    viewport.emit({ kind: "tool_state", tool: "select", step: "idle" });
    expect(screen.queryByText(/click the second corner/)).not.toBeInTheDocument();
    expect(tool("Select")).toHaveAttribute("aria-pressed", "true");
  });

  it("asks for a selection before the Move tool", async () => {
    const { viewport, user } = await ready();
    await user.click(tool("Move"));
    expect(await screen.findByText("Select something to move")).toBeVisible();
    expect(viewport.tools).toEqual([]);
    viewport.emit({ kind: "selection", ids: [BLOCK] });
    await user.click(tool("Move"));
    expect(viewport.tools).toEqual(["move"]);
  });

  it("answers V, M, B, P and S outside text fields, with no modifier", async () => {
    const { viewport, user } = await ready();
    viewport.emit({ kind: "selection", ids: [BLOCK] });
    await user.keyboard("bmv");
    expect(viewport.tools).toEqual(["place_block", "move", "select"]);
    await user.keyboard("ps");
    expect(viewport.commands).toEqual([
      { kind: "projection", projection: "parallel" },
      { kind: "snapping", enabled: false },
    ]);
    viewport.emit({
      kind: "view_state",
      state: viewState({ projection: "parallel", snapping: false }),
    });
    expect(status()).toHaveTextContent("1 element selected · Esc to clear · Snap off");
    await user.keyboard("ps");
    expect(viewport.commands.slice(2)).toEqual([
      { kind: "projection", projection: "perspective" },
      { kind: "snapping", enabled: true },
    ]);

    await user.keyboard("{Control>}b{/Control}{Alt>}m{/Alt}");
    const field = document.createElement("input");
    document.body.append(field);
    field.focus();
    await user.keyboard("v");
    const area = document.createElement("textarea");
    document.body.append(area);
    area.focus();
    await user.keyboard("m");
    const editable = document.createElement("div");
    editable.contentEditable = "true";
    document.body.append(editable);
    editable.focus();
    await user.keyboard("b");
    const box = document.createElement("input");
    box.type = "checkbox";
    document.body.append(box);
    box.focus();
    await user.keyboard("b");
    expect(viewport.tools).toEqual(["place_block", "move", "select", "place_block"]);
  });

  it("keeps single-key shortcuts to the focused view once they are turned off", async () => {
    localStorage.setItem(SINGLE_KEYS_STORAGE_KEY, "off");
    const { viewport, user } = await ready();
    expect(tool("Block")).not.toHaveAttribute("aria-keyshortcuts");
    expect(tool("Block")).toHaveAttribute("title", "Block");
    const parallel = within(screen.getByRole("group", { name: "Projection" })).getByRole("button", {
      name: "Parallel",
    });
    expect(parallel).not.toHaveAttribute("title");
    await user.keyboard("bp");
    expect(viewport.tools).toEqual([]);
    expect(viewport.commands).toEqual([]);
    screen.getByRole("application", { name: "3D view" }).focus();
    await user.keyboard("bp");
    expect(viewport.tools).toEqual(["place_block"]);
    expect(viewport.commands).toEqual([{ kind: "projection", projection: "parallel" }]);
  });

  it("names P on the projection toggle while single-key shortcuts are on", async () => {
    await ready();
    const parallel = within(screen.getByRole("group", { name: "Projection" })).getByRole("button", {
      name: "Parallel",
    });
    expect(parallel).toHaveAttribute("aria-keyshortcuts", "P");
    expect(parallel).toHaveAttribute("title", "Parallel (P)");
  });

  it("controls the view: zoom, presets and projection", async () => {
    const { viewport, user } = await ready();
    const zoomSelection = screen.getByRole("button", { name: "Zoom to selection" });
    expect(zoomSelection).toHaveAttribute("aria-disabled", "true");
    await user.click(zoomSelection);
    await user.click(screen.getByRole("button", { name: "Zoom fit" }));
    viewport.emit({ kind: "selection", ids: [BLOCK, OTHER] });
    expect(status()).toHaveTextContent("2 elements selected · Esc to clear");
    expect(zoomSelection).toHaveAttribute("aria-disabled", "false");
    await user.click(zoomSelection);
    await user.click(screen.getByRole("button", { name: "View presets" }));
    await user.click(screen.getByRole("menuitem", { name: "Iso south-west" }));
    const projection = screen.getByRole("group", { name: "Projection" });
    await user.click(within(projection).getByRole("button", { name: "Parallel" }));
    expect(viewport.commands).toEqual([
      { kind: "zoom_fit" },
      { kind: "zoom_selection" },
      { kind: "view_preset", preset: "iso_sw" },
      { kind: "projection", projection: "parallel" },
    ]);
  });
});

describe("ViewportView feedback", () => {
  it("announces the viewport's keys in words, with the locale's numbers", async () => {
    const { viewport } = await ready();
    const said = (event: Parameters<typeof viewport.emit>[0]) => {
      viewport.emit(event);
      return liveRegion()?.textContent;
    };
    const announce = (key: string, values: Record<string, string | number> = {}) =>
      said({ kind: "announce", key: key as never, values });
    expect(announce("snap.midpoint")).toBe("Snap: midpoint");
    expect(announce("selection.count", { count: 0 })).toBe("Selection cleared");
    expect(announce("selection.count", { count: 1 })).toBe("1 element selected");
    expect(announce("selection.count", { count: 3 })).toBe("3 elements selected");
    expect(announce("move.distance", { distance: 2.5 })).toBe("Moved by 2.500 metres");
    expect(announce("place.size", { width: 6, depth: 4 })).toBe("Footprint 6.000 by 4.000 metres");
    expect(announce("place.height", { height: 3 })).toBe("Block placed, height 3.000 metres");
    expect(announce("section_box.face_moved", { value: 1.25 })).toBe(
      "Section box face moved to 1.250 metres",
    );
    expect(announce("view.preset", { name: "top" })).toBe("Top view");
    expect(announce("view.projection", { projection: "parallel" })).toBe("Parallel projection");
    expect(announce("visibility.hidden", { count: 2 })).toBe("2 elements hidden");
    // The same words again are announced again.
    expect(announce("snap.midpoint")).toBe("Snap: midpoint");
    // Hover is never announced.
    expect(said({ kind: "hover", id: BLOCK })).toBe("Snap: midpoint");
  });

  it("formats the announcements in French", async () => {
    localStorage.setItem(OVERRIDE_STORAGE_KEY, "fr");
    const { viewport } = await ready();
    viewport.emit({ kind: "announce", key: "move.distance", values: { distance: 2.5 } });
    expect(liveRegion()).toHaveTextContent("Déplacé de 2,500 mètres");
  });

  it("draws the chip next to the pointer without re-rendering", async () => {
    const { viewport } = await ready();
    const chip = document.querySelector<HTMLElement>(".vp-chip");
    expect(chip).not.toBeVisible();
    const label = (key: "move.distance" | "place.size" | "place.height", values: object) =>
      viewport.emit({
        kind: "preview_label",
        label: { at: [40, 60], key, values: values as Record<string, number>, typed: null },
      });
    label("move.distance", { distance: 2.5 });
    expect(chip).toBeVisible();
    expect(chip).toHaveTextContent("2.500 m");
    expect(chip?.style.transform).toBe("translate(40px, 60px)");
    label("place.size", { width: 6, depth: 4 });
    expect(chip).toHaveTextContent("6.000 × 4.000 m");
    label("place.height", {});
    expect(chip).toHaveTextContent("Height 0.000 m");
    expect(chip).not.toHaveClass("vp-chip-typed");
    viewport.emit({
      kind: "preview_label",
      label: { at: [0, 0], key: "move.distance", values: {}, typed: "2.5" },
    });
    expect(chip).toHaveTextContent("Distance 2.5 m ↵");
    // A typed value reads as a field.
    expect(chip).toHaveClass("vp-chip-typed");
    // Near the bottom edge the chip goes above the pointer, clear of the tool bar.
    const view = chip?.parentElement;
    if (view) Object.defineProperty(view, "clientHeight", { value: 600, configurable: true });
    viewport.emit({
      kind: "preview_label",
      label: { at: [30, 550], key: "place.size", values: { width: 6, depth: 4 }, typed: "6" },
    });
    expect(chip?.style.transform).toBe("translate(30px, 502px)");
    expect(chip).toHaveTextContent("Width 6 m ↵");
    viewport.emit({ kind: "preview_label", label: null });
    expect(chip).not.toBeVisible();
  });

  it("moves the elements of a move intent and offers Undo", async () => {
    const { core, viewport, user } = await ready({ blocks: [BLOCK] });
    viewport.emit({
      kind: "intent",
      intent: { kind: "move", ids: [BLOCK], translation: [0, 2.5, 0] },
    });
    expect(await screen.findByText("Moved 1 element by 2.500 m")).toBeVisible();
    expect(core.calls).toContainEqual({
      method: "elements.move",
      params: { ids: [BLOCK], translation: [0, 2.5, 0] },
    });
    await user.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() => expect(core.calls.map((call) => call.method)).toContain("history.undo"));
  });

  it("names how many elements moved", async () => {
    const { viewport } = await ready({ blocks: [BLOCK, OTHER] });
    viewport.emit({
      kind: "intent",
      intent: { kind: "move", ids: [BLOCK, OTHER], translation: [3, 4, 0] },
    });
    expect(await screen.findByText("Moved 2 elements by 5.000 m")).toBeVisible();
  });

  it("says why a move was refused", async () => {
    const { core, viewport } = await ready();
    const move = () =>
      viewport.emit({
        kind: "intent",
        intent: { kind: "move", ids: [BLOCK], translation: [1, 0, 0] },
      });
    core.failNext("elements.move", new ApiError(-32000, "refused", { reason: "not_movable" }));
    move();
    expect(await screen.findByText("Can't move this element yet")).toBeVisible();
    core.failNext("elements.move", new ApiError(-32000, "refused", { reason: "too_many" }));
    move();
    expect(await screen.findByText("Too many elements to move at once")).toBeVisible();
    core.failNext("elements.move", new ApiError(-32602, "invalid params"));
    move();
    expect(await screen.findByText("The move failed. Try again.")).toBeVisible();
    core.failNext("elements.move", new ApiError(-32000, "refused", { reason: "out_of_range" }));
    move();
    expect(
      await screen.findByText("This move would take the element too far from the project origin."),
    ).toBeVisible();
    core.failNext("elements.move", new ApiError(-32000, "refused", { reason: "unknown_element" }));
    move();
    expect(
      await screen.findByText("This element no longer exists. It may have been deleted."),
    ).toBeVisible();
  });

  it("says why a move was refused, in French", async () => {
    localStorage.setItem(OVERRIDE_STORAGE_KEY, "fr");
    const { core, viewport } = await ready();
    const move = () =>
      viewport.emit({
        kind: "intent",
        intent: { kind: "move", ids: [BLOCK], translation: [1, 0, 0] },
      });
    core.failNext("elements.move", new ApiError(-32000, "refused", { reason: "out_of_range" }));
    move();
    expect(
      await screen.findByText(
        "Ce déplacement amènerait l’élément trop loin de l’origine du projet.",
      ),
    ).toBeVisible();
    core.failNext("elements.move", new ApiError(-32000, "refused", { reason: "unknown_element" }));
    move();
    expect(
      await screen.findByText("Cet élément n’existe plus. Il a peut-être été supprimé."),
    ).toBeVisible();
    localStorage.removeItem(OVERRIDE_STORAGE_KEY);
  });

  it("derives the empty state from the categories the viewport reports, not from the model", async () => {
    const { core, viewport } = await ready();
    expect(screen.queryByRole("heading", { name: "Nothing to see yet" })).toBeNull();
    viewport.emit({
      kind: "view_state",
      state: viewState({ categories: [group("Block")].map((g) => ({ ...g, count: 0 })) }),
    });
    expect(await screen.findByRole("heading", { name: "Nothing to see yet" })).toBeVisible();
    viewport.emit({ kind: "view_state", state: viewState() });
    await waitFor(() =>
      expect(screen.queryByRole("heading", { name: "Nothing to see yet" })).toBeNull(),
    );
    expect(core.calls.some((call) => call.method === "model.list_objects")).toBe(false);
  });

  it("places the block of an intent, offers Undo and selects it once the view has it", async () => {
    const { core, viewport } = await ready({ blocks: [] });
    core.setSceneRevision(4);
    viewport.emit({
      kind: "intent",
      intent: { kind: "place_block", location: [1, 2, 0], rotation: 0, size: [6, 4, 3] },
    });
    expect(await screen.findByText("Block placed · 6.000 × 4.000 × 3.000 m")).toBeVisible();
    const created = core.calls.find((call) => call.method === "model.create");
    const { id } = (created?.params ?? {}) as { id?: string };
    await waitFor(() =>
      expect(viewport.commands).toContainEqual({ kind: "select", ids: [id ?? ""] }),
    );
    expect(viewport.applied.at(-1)).toBe(4);
    // The viewport now draws the block.
    viewport.emit({ kind: "view_state", state: viewState() });
    await waitFor(() => expect(screen.queryByText("Nothing to see yet")).not.toBeInTheDocument());
  });

  it("says when a block could not be placed", async () => {
    const { core, viewport } = await ready();
    core.failNext("model.create", new ApiError(-32602, "invalid params"));
    viewport.emit({
      kind: "intent",
      intent: { kind: "place_block", location: [0, 0, 0], rotation: 0, size: [1, 1, 1] },
    });
    expect(await screen.findByText("The block could not be placed. Try again.")).toBeVisible();
  });
});

describe("Navigation hint", () => {
  it("lists the bindings once; Got it hides it for the session", async () => {
    const { user, unmount } = await ready();
    const card = await screen.findByRole("region", { name: "Moving around the model" });
    expect(card).toHaveTextContent("Right-click + drag · arrows");
    await user.click(within(card).getByRole("button", { name: "Got it" }));
    expect(screen.queryByRole("region", { name: "Moving around the model" })).toBeNull();
    unmount();
    await ready();
    expect(screen.queryByRole("region", { name: "Moving around the model" })).toBeNull();
  });

  it("is not shown again after Don't show again", async () => {
    const { user } = await ready();
    await user.click(await screen.findByRole("button", { name: "Don’t show again" }));
    expect(localStorage.getItem(HINT_STORAGE_KEY)).toBe("hidden");
  });

  it("stays hidden for the session when the storage cannot be used", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const { user, unmount } = await ready();
    await user.click(await screen.findByRole("button", { name: "Don’t show again" }));
    expect(screen.queryByRole("region", { name: "Moving around the model" })).toBeNull();
    unmount();
    await ready();
    expect(screen.queryByRole("region", { name: "Moving around the model" })).toBeNull();
  });
});

describe("section box and visibility shortcuts", () => {
  it("hides and isolates the selection with H and I, and asks for a selection first", async () => {
    const { viewport, user } = await ready();
    await user.keyboard("h");
    expect(await screen.findByText("Select something to hide or isolate")).toBeVisible();
    expect(viewport.commands).toEqual([]);
    viewport.emit({ kind: "selection", ids: [BLOCK] });
    await user.keyboard("hi");
    expect(viewport.commands).toEqual([
      { kind: "hide", target: { by: "selection" } },
      { kind: "isolate", target: { by: "selection" } },
    ]);
  });

  it("resets visibility with Alt+H and toggles the section box with Alt+B, even with single keys off", async () => {
    localStorage.setItem(SINGLE_KEYS_STORAGE_KEY, "off");
    const { viewport, user } = await ready();
    await user.keyboard("{Alt>}h{/Alt}");
    await user.keyboard("{Alt>}b{/Alt}");
    expect(viewport.commands).toEqual([
      { kind: "reset_visibility" },
      { kind: "section_box", enabled: true },
    ]);
    viewport.emit({
      kind: "view_state",
      state: viewState({ sectionBox: { enabled: true, min: [0, 0, 0], max: [1, 1, 1] } }),
    });
    await user.keyboard("{Alt>}b{/Alt}");
    expect(viewport.commands.at(-1)).toEqual({ kind: "section_box", enabled: false });
    // Shift makes it another chord, and a text field keeps its keys.
    await user.keyboard("{Alt>}{Shift>}b{/Shift}{/Alt}");
    const field = document.createElement("input");
    document.body.append(field);
    field.focus();
    await user.keyboard("{Alt>}h{/Alt}");
    expect(viewport.commands).toHaveLength(3);
    field.remove();
  });

  it("ends an isolate with Escape from outside the canvas, once, and leaves the canvas to the viewport", async () => {
    const { viewport, user } = await ready();
    viewport.emit({ kind: "view_state", state: viewState() });
    await user.keyboard("{Escape}");
    expect(viewport.commands).toEqual([]);

    viewport.emit({
      kind: "view_state",
      state: viewState({ isolating: true, categories: [group("Structure", { isolated: true })] }),
    });
    screen.getByRole("application", { name: "3D view" }).focus();
    await user.keyboard("{Escape}");
    expect(viewport.commands).toEqual([]);
    (document.activeElement as HTMLElement).blur();
    await user.keyboard("{Escape}");
    expect(viewport.commands).toEqual([{ kind: "cancel" }]);
    await user.keyboard("{Shift>}{Escape}{/Shift}");
    expect(viewport.commands).toHaveLength(1);

    const field = document.createElement("input");
    document.body.append(field);
    field.focus();
    await user.keyboard("{Escape}");
    expect(viewport.commands).toHaveLength(1);
    field.remove();
  });

  it("cancels a pending face pick with Escape too", async () => {
    const { viewport, user } = await ready();
    viewport.emit({ kind: "tool_state", tool: "select", step: "pick_face" });
    await user.keyboard("{Escape}");
    expect(viewport.commands).toEqual([{ kind: "cancel" }]);
  });

  it("leaves B and Escape to an open dialog", async () => {
    const { viewport, user } = await ready({
      beside: (
        <Dialog titleId="t" onCancel={() => undefined}>
          <h2 id="t">Title</h2>
        </Dialog>
      ),
    });
    viewport.emit({ kind: "tool_state", tool: "select", step: "pick_face" });
    (document.activeElement as HTMLElement).blur();
    await user.keyboard("b{Escape}");
    expect(viewport.tools).toEqual([]);
    expect(viewport.commands).toEqual([]);
  });
});
