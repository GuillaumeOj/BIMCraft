import { describe, expect, it } from "vitest";
import type { MaterialProperty } from "../api";
import {
  designCodeLabel,
  displayUnit,
  formatBounds,
  formatValue,
  formatWithUnit,
  parseValue,
  SYMBOLS,
  tabOf,
} from "./units";

const stress = { key: "fck", quantity: "stress" } as const;
const modulus = { key: "e_modulus", quantity: "modulus" } as const;
const strain = { key: "eps_c2", quantity: "strain" } as const;
const ratio = { key: "poisson_ratio", quantity: "ratio" } as const;
const expansion = { key: "thermal_expansion", quantity: "ratio" } as const;

describe("displayUnit", () => {
  it.each([
    [stress, "MPa"],
    [modulus, "GPa"],
    [{ key: "density", quantity: "density" }, "kg/m³"],
    [strain, "‰"],
    [ratio, ""],
    [{ key: "thermal_conductivity", quantity: "thermal_conductivity" }, "W/(m·K)"],
    [{ key: "specific_heat", quantity: "specific_heat" }, "J/(kg·K)"],
    [expansion, "10⁻⁶/K"],
  ] as const)("is fixed for %j", (property, unit) => {
    expect(displayUnit(property).unit).toBe(unit);
  });
});

describe("formatValue", () => {
  it("converts SI to the unit and trims trailing zeros", () => {
    expect(formatValue(stress, 25e6, "en")).toBe("25");
    expect(formatValue(stress, 2.6e6, "en")).toBe("2.6");
    expect(formatValue(modulus, 31e9, "en")).toBe("31");
    expect(formatValue(strain, 0.0035, "en")).toBe("3.5");
  });

  it("uses the decimal comma in French and no grouping", () => {
    expect(formatValue(stress, 2.6e6, "fr")).toBe("2,6");
    expect(formatValue({ key: "density", quantity: "density" }, 2500, "fr")).toBe("2500");
  });

  it("hides binary noise", () => {
    expect(formatValue(strain, 0.0025, "en")).toBe("2.5");
  });
});

describe("formatWithUnit", () => {
  it("joins value and unit with a no-break space", () => {
    expect(formatWithUnit(stress, 25e6, "en")).toBe("25 MPa");
  });

  it("is the bare number for a ratio", () => {
    expect(formatWithUnit(ratio, 0.2, "en")).toBe("0.2");
  });
});

describe("parseValue", () => {
  it("converts what the user typed back to SI, with either separator", () => {
    expect(parseValue(stress, "30")).toBe(30e6);
    expect(parseValue(stress, "2,6")).toBe(2.6e6);
    expect(parseValue(strain, "3.5")).toBe(0.0035);
    expect(parseValue(modulus, " 31 ")).toBe(31e9);
  });

  it("is null for text that is not a number", () => {
    expect(parseValue(stress, "abc")).toBeNull();
    expect(parseValue(stress, "")).toBeNull();
  });
});

describe("formatBounds", () => {
  const range = (min: number | null, max: number | null): MaterialProperty["range"] => ({
    min,
    max,
    min_inclusive: true,
    max_inclusive: false,
  });

  it("shows the bounds in the unit, null for a missing one", () => {
    expect(formatBounds({ ...ratio, range: range(0, 0.5) }, "en")).toEqual({
      min: "0",
      max: "0.5",
    });
    expect(formatBounds({ ...stress, range: range(null, 600e6) }, "en")).toEqual({
      min: null,
      max: "600",
    });
    expect(formatBounds({ ...stress, range: range(0, null) }, "en")).toEqual({
      min: "0",
      max: null,
    });
  });
});

describe("tabOf, SYMBOLS and designCodeLabel", () => {
  it("puts the thermal properties on the Thermal tab and the rest on Mechanical", () => {
    expect(tabOf({ key: "thermal_conductivity" })).toBe("thermal");
    expect(tabOf({ key: "specific_heat" })).toBe("thermal");
    expect(tabOf({ key: "thermal_expansion" })).toBe("thermal");
    expect(tabOf({ key: "fck" })).toBe("mechanical");
  });

  it("has a symbol for the properties the library gives", () => {
    expect(SYMBOLS).toMatchObject({ fck: "fck", thermal_conductivity: "λ", gamma_c: "γc" });
  });

  it("writes a design code as people do", () => {
    expect(designCodeLabel("en1992-1-1")).toBe("EN 1992-1-1");
    expect(designCodeLabel("other")).toBe("other");
  });
});
