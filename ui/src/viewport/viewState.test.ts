import { describe, expect, it } from "vitest";
import {
  axisNormal,
  changedFaces,
  extentIsValid,
  hiddenNames,
  isolatedNames,
  withExtent,
} from "./viewState";

const box = {
  enabled: true,
  min: [0, 0, 0] as [number, number, number],
  max: [4, 5, 6] as [number, number, number],
};

describe("section box helpers", () => {
  it("lists the faces whose position changed", () => {
    expect(changedFaces(box, box)).toEqual([]);
    expect(changedFaces(box, withExtent(box, 0, "min", -1))).toEqual([0]);
    expect(changedFaces(box, withExtent(withExtent(box, 2, "max", 9), 1, "min", 1))).toEqual([
      1, 5,
    ]);
  });

  it("replaces one extent without touching the box it came from", () => {
    const next = withExtent(box, 1, "max", 8);
    expect(next.max).toEqual([4, 8, 6]);
    expect(box.max).toEqual([4, 5, 6]);
  });

  it("accepts a minimum below the maximum and refuses the rest", () => {
    expect(extentIsValid(box, 0, "min", 3.9)).toBe(true);
    expect(extentIsValid(box, 0, "min", 4)).toBe(false);
    expect(extentIsValid(box, 0, "max", 0)).toBe(false);
    expect(extentIsValid(box, 0, "max", 0.1)).toBe(true);
    expect(extentIsValid(box, 0, "max", Number.NaN)).toBe(false);
    expect(extentIsValid(box, 0, "max", Number.POSITIVE_INFINITY)).toBe(false);
  });
});

describe("axisNormal", () => {
  it("names the axis and the sign of an axis-aligned normal", () => {
    expect(axisNormal([1, 0, 0])).toEqual({ axis: 0, sign: "+" });
    expect(axisNormal([0, -1, 0])).toEqual({ axis: 1, sign: "−" });
    expect(axisNormal([0, 0, 1])).toEqual({ axis: 2, sign: "+" });
  });

  it("gives null for an oblique normal", () => {
    expect(axisNormal([Math.SQRT1_2, Math.SQRT1_2, 0])).toBeNull();
    expect(axisNormal([0, 0, 0.5])).toBeNull();
  });
});

describe("group names", () => {
  const groups = [
    { name: "A", count: 1, hidden: true, isolated: false },
    { name: "B", count: 1, hidden: false, isolated: true },
  ];
  it("picks the hidden and the isolated ones", () => {
    expect(hiddenNames(groups)).toEqual(["A"]);
    expect(isolatedNames(groups)).toEqual(["B"]);
  });
});
