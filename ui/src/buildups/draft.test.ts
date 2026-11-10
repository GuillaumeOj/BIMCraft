import { describe, expect, it } from "vitest";
import type { BuildUpDetail } from "../api";
import {
  canAddLayer,
  type Draft,
  type DraftLayer,
  defaultCore,
  draftFromDetail,
  effectiveCore,
  insertLayer,
  isDirty,
  isThickness,
  layerInputs,
  moveLayer,
  newDraft,
  thicknessInMetres,
} from "./draft";

const layer = (patch: Partial<DraftLayer> = {}): DraftLayer => ({
  key: "k",
  function: "finish",
  material: "m",
  thickness: "13",
  priority: null,
  core: null,
  ...patch,
});

const named = (...keys: string[]) => keys.map((key) => layer({ key }));
const keys = (layers: DraftLayer[]) => layers.map((entry) => entry.key).join("");

describe("defaults", () => {
  it("gives each function its priority and only structure its core", () => {
    expect(defaultCore({ function: "structure" })).toBe(true);
    expect(effectiveCore(layer({ function: "finish" }))).toBe(false);
    expect(effectiveCore(layer({ function: "finish", core: true }))).toBe(true);
    expect(effectiveCore(layer({ function: "structure", core: false }))).toBe(false);
  });
});

describe("newDraft", () => {
  it("starts with one empty structure layer", () => {
    const draft = newDraft("slab");
    expect(draft).toMatchObject({ name: "", usage: "slab", layers: [{ function: "structure" }] });
    expect(draft.layers).toHaveLength(1);
    expect(draft.layers[0]).toMatchObject({ material: null, thickness: "", priority: null });
  });
});

describe("draftFromDetail", () => {
  const detail = {
    id: "b",
    name: "Wall",
    description: null,
    usage: "wall",
    total_thickness: 0.2125,
    thermal_resistance: null,
    layers_without_thermal_data: 0,
    layers: [
      {
        id: "l1",
        material: "m1",
        material_name: "Plaster",
        thickness: 0.0125,
        function: "finish",
        priority: 5,
        core: false,
        name: null,
        thermal_resistance: null,
      },
      {
        id: "l2",
        material: "m2",
        material_name: "Concrete",
        thickness: 0.2,
        function: "structure",
        priority: 2,
        core: false,
        name: null,
        thermal_resistance: null,
      },
    ],
  } as BuildUpDetail;

  it("keeps ids, shows millimetres and leaves defaults as defaults", () => {
    const draft = draftFromDetail(detail, "en");
    expect(draft.description).toBe("");
    expect(draft.layers[0]).toMatchObject({
      key: "l1",
      id: "l1",
      thickness: "12.5",
      priority: null,
      core: null,
    });
    expect(draft.layers[1]).toMatchObject({ thickness: "200", priority: 2, core: false });
  });

  it("keeps a thickness finer than a tenth of a millimetre", () => {
    const [first] = detail.layers;
    const fine = { ...detail, layers: first ? [{ ...first, thickness: 0.00025 }] : [] };
    const draft = draftFromDetail(fine, "en");
    expect(draft.layers[0]?.thickness).toBe("0.25");
    expect(thicknessInMetres("0.25")).toBe(0.00025);
  });

  it("writes the decimal comma in French and keeps a description", () => {
    const draft = draftFromDetail({ ...detail, description: "South" }, "fr");
    expect(draft.layers[0]?.thickness).toBe("12,5");
    expect(draft.description).toBe("South");
  });
});

describe("thickness", () => {
  it("converts millimetres to metres with either separator", () => {
    expect(thicknessInMetres("12,5")).toBe(0.0125);
    expect(thicknessInMetres("200")).toBe(0.2);
    expect(thicknessInMetres("")).toBe(0);
    expect(thicknessInMetres("abc")).toBe(0);
  });

  it("tells a number from text", () => {
    expect(isThickness("12.5")).toBe(true);
    expect(isThickness("")).toBe(false);
  });
});

describe("layerInputs", () => {
  const draft = (layers: DraftLayer[]): Draft => ({
    name: "n",
    description: "",
    usage: "wall",
    layers,
  });

  it("sends ids, metres and only the overrides", () => {
    expect(
      layerInputs(
        draft([
          layer({ id: "l1" }),
          layer({ function: "structure", priority: 2, core: false, thickness: "200" }),
        ]),
      ),
    ).toEqual([
      { id: "l1", material: "m", thickness: 0.013, function: "finish" },
      { material: "m", thickness: 0.2, function: "structure", priority: 2, core: false },
    ]);
  });

  it("is null while a layer has no material", () => {
    expect(layerInputs(draft([layer({ material: null })]))).toBeNull();
  });
});

describe("isDirty and canAddLayer", () => {
  it("compares drafts by value", () => {
    const saved = newDraft("wall");
    expect(isDirty({ ...saved }, saved)).toBe(false);
    expect(isDirty({ ...saved, name: "x" }, saved)).toBe(true);
  });

  it("stops at 32 layers", () => {
    const draft = (count: number): Draft => ({
      ...newDraft("wall"),
      layers: Array.from({ length: count }, (_, index) => layer({ key: String(index) })),
    });
    expect(canAddLayer(draft(31))).toBe(true);
    expect(canAddLayer(draft(32))).toBe(false);
  });
});

describe("moveLayer and insertLayer", () => {
  it("moves a layer up and down", () => {
    expect(keys(moveLayer(named("a", "b", "c"), 1, 0))).toBe("bac");
    expect(keys(moveLayer(named("a", "b", "c"), 1, 2))).toBe("acb");
  });

  it("does nothing outside the list", () => {
    expect(keys(moveLayer(named("a", "b"), 0, -1))).toBe("ab");
    expect(keys(moveLayer(named("a", "b"), 1, 2))).toBe("ab");
    expect(keys(moveLayer(named("a", "b"), 5, 0))).toBe("ab");
  });

  it("inserts after a layer or at the end", () => {
    expect(keys(insertLayer(named("a", "b"), 0, layer({ key: "x" })))).toBe("axb");
    expect(keys(insertLayer(named("a", "b"), null, layer({ key: "x" })))).toBe("abx");
  });
});
