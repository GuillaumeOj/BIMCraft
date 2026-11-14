import { screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fakeCore } from "../test-support/fakeCore";
import { fakeViewport } from "../test-support/fakeViewport";
import { renderWithCore } from "../test-support/render";
import { group, viewState } from "../test-support/viewState";
import type { ViewportModule } from "./types";
import { ViewPanel } from "./ViewPanel";
import { ViewportBridgeProvider } from "./ViewportBridge";
import { ViewportView } from "./ViewportView";

const loader = vi.hoisted(() => ({
  load: (): Promise<ViewportModule> => Promise.reject(new Error("no module")),
}));
vi.mock("./module", () => ({ loadViewportModule: () => loader.load() }));

const BLOCK = "1".padStart(32, "0");
const BOX = { enabled: true, min: [-1, -2, 0] as const, max: [4, 5, 3] as const };
const box = (changes: Partial<{ enabled: boolean; min: number[]; max: number[] }> = {}) => ({
  enabled: changes.enabled ?? BOX.enabled,
  min: (changes.min ?? [...BOX.min]) as [number, number, number],
  max: (changes.max ?? [...BOX.max]) as [number, number, number],
});

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});
afterEach(() => vi.unstubAllGlobals());

/** The 3D view and the View panel side by side, as in the shell. */
async function setUp(options: { language?: "fr" } = {}) {
  if (options.language === "fr") localStorage.setItem("bimcraft.language", "fr");
  const core = fakeCore();
  core.blocks.add(BLOCK);
  core.setSceneRevision(3);
  const viewport = fakeViewport();
  loader.load = async () => viewport.module;
  const rendered = renderWithCore(
    <ViewportBridgeProvider>
      <ViewportView />
      <ViewPanel />
    </ViewportBridgeProvider>,
    core,
  );
  await waitFor(() => expect(viewport.applied).toEqual([3]));
  await waitFor(() =>
    expect(document.getElementById("vp-status")).not.toHaveTextContent("Loading"),
  );
  // The viewport counts what it draws, by category; an empty model has none.
  await viewport.emit({ kind: "view_state", state: viewState() });
  return { viewport, core, ...rendered };
}

const sent = (viewport: Awaited<ReturnType<typeof setUp>>["viewport"]) => viewport.commands;
const field = (name: string) => screen.getByRole("textbox", { name });

describe("View panel", () => {
  it("shows that the view is not running until it is", () => {
    loader.load = () => new Promise(() => undefined);
    renderWithCore(
      <ViewportBridgeProvider>
        <ViewPanel />
      </ViewportBridgeProvider>,
      fakeCore(),
    );
    expect(screen.getByText("The 3D view isn't running.")).toBeInTheDocument();
  });

  it("offers the camera, with the projection the view reports and a chooser for the views", async () => {
    const { viewport, user } = await setUp();
    const projection = screen.getAllByRole("group", { name: "Projection" });
    const parallel = within(projection[projection.length - 1] as HTMLElement).getByRole("button", {
      name: "Parallel",
    });
    expect(parallel).toHaveAttribute("aria-pressed", "false");
    await user.click(parallel);
    expect(sent(viewport)).toContainEqual({ kind: "projection", projection: "parallel" });

    await viewport.emit({ kind: "view_state", state: viewState({ projection: "parallel" }) });
    expect(parallel).toHaveAttribute("aria-pressed", "true");

    await user.selectOptions(screen.getByLabelText("Look from"), "Iso south-east");
    expect(sent(viewport)).toContainEqual({ kind: "view_preset", preset: "iso_se" });
  });

  describe("section box", () => {
    it("is off to begin with, and Active switches it on", async () => {
      const { viewport, user } = await setUp();
      const active = screen.getByRole("checkbox", { name: /^Active/ });
      expect(active).not.toBeChecked();
      expect(screen.queryByRole("textbox", { name: "Min X" })).not.toBeInTheDocument();
      await user.click(active);
      expect(sent(viewport)).toContainEqual({ kind: "section_box", enabled: true });
    });

    it("shows the extents in metres with three decimals and sends a typed one on Enter", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      expect(screen.getByRole("checkbox", { name: /^Active/ })).toBeChecked();
      expect(field("Min X")).toHaveValue("-1.000");
      expect(field("Max Z")).toHaveValue("3.000");

      await user.clear(field("Max X"));
      await user.type(field("Max X"), "16{Enter}");
      expect(sent(viewport)).toContainEqual({
        kind: "section_box_extents",
        min: [-1, -2, 0],
        max: [16, 5, 3],
      });
    });

    it("takes the decimal comma of the French locale", async () => {
      const { viewport, user } = await setUp({ language: "fr" });
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      expect(field("Min X")).toHaveValue("-1,000");
      await user.clear(field("Min Y"));
      await user.type(field("Min Y"), "-2,5{Enter}");
      expect(sent(viewport)).toContainEqual({
        kind: "section_box_extents",
        min: [-1, -2.5, 0],
        max: [4, 5, 3],
      });
      localStorage.removeItem("bimcraft.language");
    });

    it("refuses a minimum that is not below the maximum, says why, and sends nothing", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      const before = sent(viewport).length;
      await user.clear(field("Min Z"));
      await user.type(field("Min Z"), "12{Enter}");
      expect(field("Min Z")).toBeInvalid();
      expect(screen.getByText("Minimum must be below maximum")).toBeVisible();
      expect(field("Min Z")).toHaveAccessibleDescription(/Minimum must be below maximum/);
      expect(sent(viewport)).toHaveLength(before);
    });

    it("refuses text that is not a number", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      await user.clear(field("Max Y"));
      await user.type(field("Max Y"), "tall{Enter}");
      expect(screen.getByText("Enter a number in metres")).toBeVisible();
    });

    it("restores the value on Escape without the canvas hearing it", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      await user.clear(field("Max X"));
      await user.type(field("Max X"), "99");
      await user.keyboard("{Escape}");
      expect(field("Max X")).toHaveValue("4.000");
      expect(sent(viewport).some((command) => command.kind === "cancel")).toBe(false);
    });

    it("applies a typed value when the focus leaves the field, and sends nothing for an unchanged one", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      await user.click(field("Max X"));
      await user.tab();
      expect(sent(viewport).some((command) => command.kind === "section_box_extents")).toBe(false);
      await user.clear(field("Max X"));
      await user.type(field("Max X"), "7");
      await user.tab();
      expect(sent(viewport)).toContainEqual({
        kind: "section_box_extents",
        min: [-1, -2, 0],
        max: [7, 5, 3],
      });
    });

    it("clears a refusal on Reset, and Reset and Turn off send their commands", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      await user.clear(field("Min Z"));
      await user.type(field("Min Z"), "12{Enter}");
      await user.click(screen.getByRole("button", { name: "Reset" }));
      expect(screen.queryByText("Minimum must be below maximum")).not.toBeInTheDocument();
      expect(sent(viewport)).toContainEqual({ kind: "section_box_reset" });
      await user.click(screen.getByRole("button", { name: /^Turn off/ }));
      expect(sent(viewport)).toContainEqual({ kind: "section_box", enabled: false });
    });

    it("drops what was typed, and its refusal, when the viewport moves that extent", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      await user.clear(field("Min Z"));
      await user.type(field("Min Z"), "12{Enter}");
      expect(field("Min Z")).toBeInvalid();
      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ min: [-1, -2, 1] }) }),
      });
      expect(field("Min Z")).toHaveValue("1.000");
      expect(field("Min Z")).toBeValid();
      expect(screen.queryByText("Minimum must be below maximum")).not.toBeInTheDocument();
    });

    it("forgets a refusal when the box is switched off and on again", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      await user.clear(field("Min Z"));
      await user.type(field("Min Z"), "12{Enter}");
      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ enabled: false }) }),
      });
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      expect(field("Min Z")).toBeValid();
      expect(field("Min Z")).toHaveValue("0.000");
      expect(screen.queryByText("Minimum must be below maximum")).not.toBeInTheDocument();
    });

    it("fits the box to the selection only when there is one", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      const fit = screen.getByRole("button", { name: "Fit to selection" });
      expect(fit).toHaveAttribute("aria-disabled", "true");
      await user.click(fit);
      expect(sent(viewport).some((command) => command.kind === "section_box_fit_selection")).toBe(
        false,
      );

      await viewport.emit({ kind: "selection", ids: [BLOCK] });
      expect(fit).toHaveAttribute("aria-disabled", "false");
      await user.click(fit);
      expect(sent(viewport)).toContainEqual({ kind: "section_box_fit_selection" });
    });

    it("chooses how cut faces look", async () => {
      const { viewport, user } = await setUp();
      await user.selectOptions(screen.getByLabelText("Cut faces"), "None");
      expect(sent(viewport)).toContainEqual({ kind: "cut_faces", style: "none" });
      await viewport.emit({ kind: "view_state", state: viewState({ cutFaces: "none" }) });
      expect(screen.getByLabelText("Cut faces")).toHaveValue("none");
      await user.selectOptions(screen.getByLabelText("Cut faces"), "Tinted");
      expect(sent(viewport)).toContainEqual({ kind: "cut_faces", style: "tinted" });
    });

    it("tags the panel and the section, and the status counts the faces moved since the box began", async () => {
      const { viewport } = await setUp();
      const status = () => document.getElementById("vp-status");
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      expect(screen.getByText("Section box active")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /Section box.*On/ })).toBeInTheDocument();
      expect(status()).toHaveTextContent("No selection");

      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ max: [5, 5, 3] }) }),
      });
      expect(status()).toHaveTextContent("Section box · 1 face moved");
      // The same face again is still one face.
      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ max: [6, 5, 3] }) }),
      });
      expect(status()).toHaveTextContent("Section box · 1 face moved");
      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ min: [-1, -3, 0], max: [6, 5, 3] }) }),
      });
      expect(status()).toHaveTextContent("Section box · 2 faces moved");
    });

    it("starts a new count at Reset and Fit to selection, and when the box is switched off", async () => {
      const { viewport, user } = await setUp();
      const status = () => document.getElementById("vp-status");
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ max: [5, 5, 3] }) }),
      });
      expect(status()).toHaveTextContent("1 face moved");
      await user.click(screen.getByRole("button", { name: "Reset" }));
      expect(status()).toHaveTextContent("No selection · Snap on");

      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ max: [6, 5, 3] }) }),
      });
      expect(status()).toHaveTextContent("1 face moved");
      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ enabled: false }) }),
      });
      expect(status()).toHaveTextContent("No selection · Snap on");
    });

    it("counts a face dragged after a Reset that changed nothing", async () => {
      const { viewport, user } = await setUp();
      const status = () => document.getElementById("vp-status");
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      // The viewport already had this box, so it reports no new state for the Reset.
      await user.click(screen.getByRole("button", { name: "Reset" }));
      await viewport.emit({
        kind: "view_state",
        state: viewState({ sectionBox: box({ max: [5, 5, 3] }) }),
      });
      expect(status()).toHaveTextContent("Section box · 1 face moved");
      expect(field("Max X")).toHaveValue("5.000");
    });

    it("shows the hint as a toast when the box is switched on, not at first sight", async () => {
      const { viewport } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState() });
      expect(screen.queryByText(/drag a handle or type values/)).not.toBeInTheDocument();
      await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
      expect(
        screen.getByText(/Section box · drag a handle or type values in the View panel/),
      ).toBeVisible();
    });
  });

  describe("clipping planes", () => {
    const plane = (point: [number, number, number], normal: [number, number, number]) => ({
      point,
      normal,
    });

    it("starts with none and arms a face pick, whose hint and status follow the view", async () => {
      const { viewport, user } = await setUp();
      expect(screen.getByRole("button", { name: /Clipping planes.*None/ })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Add plane on face" }));
      expect(sent(viewport)).toContainEqual({ kind: "clip_plane_pick" });

      await viewport.emit({ kind: "tool_state", tool: "select", step: "pick_face" });
      expect(screen.getByText("Click a face to cut at it · Esc to cancel")).toBeVisible();
      expect(document.getElementById("vp-status")).toHaveTextContent(
        "Clipping plane · choose a face",
      );
      const add = screen.getByRole("button", { name: "Add plane on face" });
      expect(add).toHaveAttribute("aria-pressed", "true");
      await user.click(add);
      expect(sent(viewport)).toContainEqual({ kind: "cancel" });
    });

    it("lists each plane with its normal and its position, and flips and removes by row", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({
        kind: "view_state",
        state: viewState({
          clipPlanes: [
            plane([12, 0, 0], [1, 0, 0]),
            plane([0, 0, 6], [0, 0, -1]),
            plane([0, 0, 0], [0.6, 0.8, 0]),
          ],
        }),
      });
      expect(screen.getByText("Plane 1")).toBeInTheDocument();
      expect(screen.getByText("Normal +X · x = 12.000 m")).toBeInTheDocument();
      expect(screen.getByText("Normal −Z · z = 6.000 m")).toBeInTheDocument();
      expect(screen.getByText("Oblique")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /Clipping planes.*3 of 6/ })).toBeInTheDocument();
      expect(screen.getByText("Clipping planes", { selector: ".vp-tag" })).toBeInTheDocument();
      expect(document.getElementById("vp-status")).toHaveTextContent("3 clipping planes active");

      await user.click(screen.getByRole("button", { name: "Flip plane 2" }));
      expect(sent(viewport)).toContainEqual({ kind: "clip_plane_flip", index: 1 });
      await user.click(screen.getByRole("button", { name: "Remove plane 1" }));
      expect(sent(viewport)).toContainEqual({ kind: "clip_plane_remove", index: 0 });
    });

    it("removes the plane of the focused row with Delete", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({
        kind: "view_state",
        state: viewState({
          clipPlanes: [plane([1, 0, 0], [1, 0, 0]), plane([2, 0, 0], [1, 0, 0])],
        }),
      });
      screen.getByRole("button", { name: "Flip plane 2" }).focus();
      await user.keyboard("{Delete}");
      expect(sent(viewport)).toContainEqual({ kind: "clip_plane_remove", index: 1 });
      screen.getByRole("button", { name: "Add plane on face" }).focus();
      await user.keyboard("{Delete}");
      expect(sent(viewport).filter((command) => command.kind === "clip_plane_remove")).toHaveLength(
        1,
      );
    });

    it("says 1 plane in the singular and clears all with a toast and no Undo", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({
        kind: "view_state",
        state: viewState({ clipPlanes: [plane([1, 0, 0], [1, 0, 0])] }),
      });
      expect(document.getElementById("vp-status")).toHaveTextContent("1 clipping plane active");
      await user.click(screen.getByRole("button", { name: "Clear all" }));
      expect(sent(viewport)).toContainEqual({ kind: "clip_planes_clear" });
      expect(screen.getByText("Clipping planes cleared")).toBeVisible();
      expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
    });

    it("stops at six planes and says so", async () => {
      const { viewport, user } = await setUp();
      const six = Array.from({ length: 6 }, (_, index) => plane([index, 0, 0], [1, 0, 0]));
      await viewport.emit({ kind: "view_state", state: viewState({ clipPlanes: six }) });
      const add = screen.getByRole("button", { name: "Add plane on face" });
      expect(add).toHaveAttribute("aria-disabled", "true");
      await user.click(add);
      expect(screen.getByText("Maximum 6 planes")).toBeVisible();
      expect(sent(viewport).some((command) => command.kind === "clip_plane_pick")).toBe(false);
    });
  });

  describe("visibility", () => {
    it("says everything is visible until something is hidden, and what is hidden after", async () => {
      const { viewport } = await setUp();
      expect(screen.getByText("Everything is visible", { selector: "p" })).toBeInTheDocument();
      await viewport.emit({
        kind: "view_state",
        state: viewState({
          categories: [
            group("Furniture", { hidden: true }),
            group("MEP", { hidden: true }),
            group("Walls"),
          ],
          disciplines: [group("Plumbing", { hidden: true })],
          hiddenSelection: 2,
        }),
      });
      expect(screen.getByText("Furniture, MEP")).toBeInTheDocument();
      expect(screen.getByText("Plumbing")).toBeInTheDocument();
      expect(screen.getByText("Hidden elements")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "End isolate" })).not.toBeInTheDocument();
    });

    it("tags an isolate, names it, ends it, and resets visibility", async () => {
      const { viewport, user } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState() });
      await viewport.emit({
        kind: "view_state",
        state: viewState({
          isolating: true,
          categories: [group("Structure", { isolated: true }), group("Walls")],
        }),
      });
      expect(screen.getByText("Isolated", { selector: ".vp-tag" })).toBeInTheDocument();
      expect(screen.getByText("Structure", { selector: "dd" })).toBeInTheDocument();
      expect(
        screen.getByText("Isolate · only Structure is shown · Esc or “End isolate” to return"),
      ).toBeVisible();
      expect(document.getElementById("vp-status")).toHaveTextContent("Isolated: Structure");

      await user.click(screen.getByRole("button", { name: "End isolate" }));
      expect(sent(viewport)).toContainEqual({ kind: "end_isolate" });
      await user.click(screen.getByRole("button", { name: /^Reset visibility/ }));
      expect(sent(viewport)).toContainEqual({ kind: "reset_visibility" });
    });

    it("names the selection when the selection is isolated", async () => {
      const { viewport } = await setUp();
      await viewport.emit({ kind: "view_state", state: viewState() });
      await viewport.emit({ kind: "view_state", state: viewState({ isolating: true }) });
      expect(screen.getByText("The selection", { selector: "dd" })).toBeInTheDocument();
      expect(document.getElementById("vp-status")).toHaveTextContent("Isolated: the selection");
    });
  });

  it("collapses a section and keeps its state readable", async () => {
    const { viewport, user } = await setUp();
    await viewport.emit({ kind: "view_state", state: viewState({ sectionBox: box() }) });
    const toggle = screen.getByRole("button", { name: /^Section box.*On/ });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });

  it("goes quiet when the view stops", async () => {
    const { viewport, unmount } = await setUp();
    await viewport.emit({ kind: "error", key: "device_lost" });
    await waitFor(() => expect(screen.getByText("The 3D view isn't running.")).toBeInTheDocument());
    unmount();
  });
});
