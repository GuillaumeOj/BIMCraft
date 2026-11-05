import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Georeference } from "../api";
import { fakeCore, refusal } from "../test-support/fakeCore";
import { renderWithCore } from "../test-support/render";
import { CoordinatesPanel, type PickedPoint } from "./CoordinatesPanel";
import { useGeoref } from "./GeorefContext";

const PLACED: Georeference = {
  crs_epsg: 2154,
  base_point: [652000, 6862000, 35],
  grid_rotation: 0,
  scale: 1,
  survey_point: null,
};

// What the fake core answers for the origin: latitude and longitude from the map coordinates.
const LAT = ((0.06862 * 180) / Math.PI).toFixed(8);
const LON = ((0.00652 * 180) / Math.PI).toFixed(8);

function Dialogs() {
  const { dialogOpen } = useGeoref();
  return <p>{dialogOpen ? "dialog open" : "dialog closed"}</p>;
}

function renderPanel(core = fakeCore({ georeference: PLACED }), picked: PickedPoint | null = null) {
  const view = (point: PickedPoint | null) => (
    <>
      <CoordinatesPanel pickedPoint={point} />
      <Dialogs />
    </>
  );
  const rendered = renderWithCore(view(picked), core);
  return {
    core,
    ...rendered,
    pick: (point: PickedPoint) => rendered.rerender(view(point)),
  };
}

/** The three fields of a block, found by the block's title. */
function block(title: string) {
  const section = screen.getByRole("heading", { name: title }).closest("section") as HTMLElement;
  const fields = within(section).getAllByRole("textbox") as HTMLInputElement[];
  return { section, fields, values: () => fields.map((input) => input.value) };
}

const toast = () => document.querySelector(".toast-region") as HTMLElement;

const convertCalls = (core: ReturnType<typeof fakeCore>) =>
  core.calls.filter((call) => call.method === "georef.convert");

afterEach(() => {
  vi.useRealTimers();
});

describe("CoordinatesPanel, project not georeferenced", () => {
  it("explains and opens the location dialog", async () => {
    const { user } = renderPanel(fakeCore());
    expect(await screen.findByText(/no location yet/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Set project location…" }));
    expect(screen.getByText("dialog open")).toBeInTheDocument();
  });

  it("says so when the georeference cannot be read", async () => {
    const core = fakeCore();
    core.failNext("georef.get", new Error("down"));
    renderPanel(core);
    expect(await screen.findByRole("alert")).toHaveTextContent("could not be read");
  });

  it("shows a loading note until the georeference arrives", () => {
    renderPanel(fakeCore());
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });
});

describe("CoordinatesPanel, project georeferenced", () => {
  it("shows the origin in the three systems with the CRS and datum names", async () => {
    renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    expect(block("Model (local)").values()).toEqual(["0.000", "0.000", "0.000"]);
    expect(block("Map").values()).toEqual(["652000.000", "6862000.000", "35.000"]);
    expect(block("Geographic").values()).toEqual([LAT, LON, "35.000"]);
    expect(screen.getByText("RGF93 v1 / Lambert-93 (EPSG 2154)")).toBeInTheDocument();
    expect(screen.getByText("Project axes, in metres")).toBeInTheDocument();
    expect(screen.getAllByText("RGF93 v1").length).toBeGreaterThan(0);
    expect(screen.getByText(/Height is approximate/)).toBeInTheDocument();
  });

  it("converts a typed point when Enter is pressed and marks the block as the source", async () => {
    const { core, user } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    const [east] = block("Map").fields;
    await user.clear(east as HTMLElement);
    await user.type(east as HTMLElement, "652100,5{Enter}");
    await waitFor(() => expect(block("Model (local)").values()[0]).toBe("100.500"));
    expect(convertCalls(core).at(-1)?.params).toEqual({
      from: "projected",
      points: [[652100.5, 6862000, 35]],
    });
    expect(within(block("Map").section).getByText("Edited")).toBeInTheDocument();
    expect(
      within(block("Model (local)").section).getByText("Computed from Map"),
    ).toBeInTheDocument();
    expect(block("Map").values()[0]).toBe("652100,5");
  });

  it("converts after a short pause in typing, and only once", async () => {
    const { core } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    vi.useFakeTimers();
    const [x] = block("Model (local)").fields;
    const before = convertCalls(core).length;
    fireEvent.change(x as HTMLElement, { target: { value: "1" } });
    fireEvent.change(x as HTMLElement, { target: { value: "12" } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(199);
    });
    expect(convertCalls(core)).toHaveLength(before);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2);
    });
    expect(convertCalls(core)).toHaveLength(before + 1);
    expect(convertCalls(core).at(-1)?.params).toEqual({ from: "local", points: [[12, 0, 0]] });
  });

  it("flags a value that is not a number and does not convert", async () => {
    const { core, user } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    const before = convertCalls(core).length;
    const [x] = block("Model (local)").fields;
    await user.clear(x as HTMLElement);
    await user.type(x as HTMLElement, "abc{Enter}");
    expect(x).toBeInvalid();
    expect(x).toHaveAccessibleDescription("Enter a number, e.g. 652000.000.");
    expect(convertCalls(core)).toHaveLength(before);
    await user.clear(x as HTMLElement);
    await user.type(x as HTMLElement, "2{Enter}");
    await waitFor(() => expect(x).toBeValid());
  });

  it("clears the flag of a block that is computed again from another block", async () => {
    const { user } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    const [easting] = block("Map").fields;
    await user.clear(easting as HTMLElement);
    await user.type(easting as HTMLElement, "abc{Enter}");
    expect(easting).toBeInvalid();
    const [x] = block("Model (local)").fields;
    await user.clear(x as HTMLElement);
    await user.type(x as HTMLElement, "2{Enter}");
    await waitFor(() => expect(block("Map").values()[0]).toBe("652002.000"));
    expect(easting).toBeValid();
  });

  it("switches the geographic block between decimal degrees and DMS, and accepts DMS", async () => {
    const { core, user } = renderPanel();
    await waitFor(() => expect(block("Geographic").values()[0]).toBe(LAT));
    await user.click(screen.getByRole("button", { name: "DMS" }));
    expect(block("Geographic").values()[0]).toMatch(/^3°\d+′[\d.]+″N$/);
    const [lat] = block("Geographic").fields;
    await user.clear(lat as HTMLElement);
    await user.type(lat as HTMLElement, "10°30′0″N{Enter}");
    await waitFor(() =>
      expect(convertCalls(core).at(-1)?.params).toMatchObject({ from: "geographic" }),
    );
    const sent = convertCalls(core).at(-1)?.params as { points: number[][] };
    expect(sent.points[0]?.[0]).toBeCloseTo((10.5 * Math.PI) / 180, 12);
    await user.click(screen.getByRole("button", { name: "Decimal degrees" }));
    expect(block("Geographic").values()[0]).toBe("10.50000000");
  });

  it("says the point is too far from the projection and clears the other blocks", async () => {
    const { core, user } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    core.failNext("georef.convert", refusal("outside_domain"));
    const [x] = block("Model (local)").fields;
    await user.type(x as HTMLElement, "9{Enter}");
    expect(
      await screen.findAllByText("This point is too far from the projection to convert."),
    ).not.toHaveLength(0);
    expect(block("Map").values()).toEqual(["", "", ""]);
    expect(within(block("Model (local)").section).queryByText(/too far/)).not.toBeInTheDocument();
  });

  it("shows a generic message when the conversion fails, in every block", async () => {
    const { core, user } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    core.failNext("georef.convert", new Error("down"));
    await user.type(block("Model (local)").fields[0] as HTMLElement, "9{Enter}");
    expect(
      await within(block("Model (local)").section).findByText("Conversion failed. Try again."),
    ).toBeInTheDocument();
  });

  it("copies a block as tab-separated text with the decimal separator of the language", async () => {
    const { user } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    await user.click(screen.getByRole("button", { name: "Copy Map" }));
    expect(await navigator.clipboard.readText()).toBe("652000.000\t6862000.000\t35.000");
    expect(toast()).toHaveTextContent("Copied");
    await user.click(screen.getByRole("button", { name: "Copy Geographic" }));
    expect(await navigator.clipboard.readText()).toBe(`${LAT}\t${LON}\t35.000`);
  });

  it("does not announce a copy that the system refused", async () => {
    const { user } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new Error("denied"));
    await user.click(screen.getByRole("button", { name: "Copy Map" }));
    expect(toast()).not.toHaveTextContent("Copied");
  });

  it("goes back to the origin with Reset to origin", async () => {
    const { user } = renderPanel();
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    const [x] = block("Model (local)").fields;
    await user.clear(x as HTMLElement);
    await user.type(x as HTMLElement, "5{Enter}");
    await waitFor(() => expect(block("Map").values()[0]).toBe("652005.000"));
    await user.click(screen.getByRole("button", { name: "Reset to origin" }));
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    expect(block("Model (local)").values()).toEqual(["0.000", "0.000", "0.000"]);
    expect(screen.queryByText("Edited")).not.toBeInTheDocument();
  });

  it("opens the location dialog from Change…", async () => {
    const { user } = renderPanel();
    await user.click(await screen.findByRole("button", { name: "Change…" }));
    expect(screen.getByText("dialog open")).toBeInTheDocument();
  });

  it("has a Pick from view button that is focusable, described and does nothing yet", async () => {
    const { core, user } = renderPanel();
    const pick = await screen.findByRole("button", { name: "Pick from view" });
    expect(pick).toHaveAttribute("aria-disabled", "true");
    expect(pick).not.toBeDisabled();
    expect(pick).toHaveAccessibleDescription("Available when the 3D view is ready.");
    const before = convertCalls(core).length;
    await user.click(pick);
    expect(convertCalls(core)).toHaveLength(before);
  });

  it("converts a point pushed in from outside, as if it had been typed", async () => {
    const core = fakeCore({ georeference: PLACED });
    const { pick } = renderPanel(core);
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    pick({ system: "projected", point: [652010, 6862020, 35] });
    await waitFor(() =>
      expect(block("Model (local)").values()).toEqual(["10.000", "20.000", "0.000"]),
    );
    expect(within(block("Map").section).getByText("Edited")).toBeInTheDocument();
    expect(block("Map").values()).toEqual(["652010.000", "6862020.000", "35.000"]);
  });

  it("keeps the current local point when the georeference changes", async () => {
    const core = fakeCore({ georeference: PLACED });
    renderPanel(core);
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    core.replace({ ...PLACED, base_point: [700000, 6900000, 10] });
    await act(async () => core.notifyChange());
    await waitFor(() =>
      expect(block("Map").values()).toEqual(["700000.000", "6900000.000", "10.000"]),
    );
    expect(block("Model (local)").values()).toEqual(["0.000", "0.000", "0.000"]);
  });

  it("falls back to the explanation when the georeference is removed", async () => {
    const core = fakeCore({ georeference: PLACED });
    renderPanel(core);
    await waitFor(() => expect(block("Map").values()[0]).toBe("652000.000"));
    core.replace(null);
    await act(async () => core.notifyChange());
    expect(await screen.findByText(/no location yet/)).toBeInTheDocument();
  });
});
