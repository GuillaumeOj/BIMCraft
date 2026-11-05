import { describe, expect, it } from "vitest";
import {
  degToRad,
  formatDms,
  formatNumber,
  formatPlain,
  formatSigned,
  LATITUDE,
  LONGITUDE,
  parseAngle,
  parseNumber,
  radToDeg,
} from "./units";

describe("parseNumber", () => {
  it.each([
    ["652000.5", 652000.5],
    ["652000,5", 652000.5],
    ["-35", -35],
    ["−2,5", -2.5],
    ["+3", 3],
    [".5", 0.5],
    ["12.", 12],
    ["652 000,500", 652000.5],
    ["652 000,500", 652000.5],
    ["652 000.5", 652000.5],
    ["1,234.5", 1234.5],
    ["1.234,5", 1234.5],
    ["1,234,567", 1234567],
    ["1.234.567", 1234567],
  ])("reads %j as %d", (text, expected) => {
    expect(parseNumber(text)).toBe(expected);
  });

  it.each(["", " ", "abc", "1e3", "--1", "1,2,3x", ",", "12-3", "0x10"])("rejects %j", (text) => {
    expect(parseNumber(text)).toBeNull();
  });
});

describe("formatNumber", () => {
  it("uses the decimal separator of the language and no grouping", () => {
    expect(formatNumber(6862291.88, 3, "en")).toBe("6862291.880");
    expect(formatNumber(6862291.88, 3, "fr")).toBe("6862291,880");
  });

  it("does not show a minus sign for a value that rounds to zero", () => {
    expect(formatNumber(-0.0001, 3, "en")).toBe("0.000");
    expect(formatNumber(-0, 3, "en")).toBe("0.000");
  });

  it("keeps the minus sign of other values", () => {
    expect(formatNumber(-43.83, 3, "en")).toBe("-43.830");
  });
});

describe("formatPlain", () => {
  it("drops useless decimals", () => {
    expect(formatPlain(1, 6, "en")).toBe("1");
    expect(formatPlain(0.9999, 6, "fr")).toBe("0,9999");
    expect(formatPlain(1.00000049, 6, "en")).toBe("1");
  });
});

describe("formatSigned", () => {
  it("uses a true minus sign", () => {
    expect(formatSigned(-30.4717, 4, "fr")).toBe("−30,4717");
    expect(formatSigned(2, 1, "en")).toBe("2.0");
  });
});

describe("radians and degrees", () => {
  it("converts both ways", () => {
    expect(radToDeg(Math.PI)).toBeCloseTo(180, 12);
    expect(degToRad(90)).toBeCloseTo(Math.PI / 2, 12);
  });
});

describe("formatDms", () => {
  it("writes degrees, minutes and seconds with a hemisphere letter", () => {
    expect(formatDms(48.8530064, LATITUDE, "en")).toBe("48°51′10.8230″N");
    expect(formatDms(-2.5, LONGITUDE, "fr")).toBe("2°30′0,0000″W");
  });

  it("never rounds to 60 seconds", () => {
    expect(formatDms(0.9999999999, LATITUDE, "en")).toBe("1°0′0.0000″N");
  });

  it("writes a minus sign when there are no hemisphere letters", () => {
    expect(formatDms(-30.5, null, "en")).toBe("-30°30′0.0000″");
    expect(formatDms(30.5, null, "en")).toBe("30°30′0.0000″");
  });

  it("writes zero with the positive letter", () => {
    expect(formatDms(-0, LATITUDE, "en")).toBe("0°0′0.0000″N");
  });
});

describe("parseAngle", () => {
  it.each([
    ["48.5", 48.5],
    ["48,5", 48.5],
    ["-30,4717", -30.4717],
    ["48°30′0″N", 48.5],
    ["48°30'0\"N", 48.5],
    ["2°30′0,0000″W", -2.5],
    ["S 10°15′", -10.25],
    ["E 2°21′07,9″", 2 + 21 / 60 + 7.9 / 3600],
    ["-2°30′", -2.5],
    ["12°", 12],
    ["N 45", 45],
    ["45 O", -45],
    ["S 10 15 30", -(10 + 15 / 60 + 30 / 3600)],
    ["48 51 10.8", 48 + 51 / 60 + 10.8 / 3600],
  ])("reads %j as %d", (text, expected) => {
    expect(parseAngle(text)).toBeCloseTo(expected, 10);
  });

  it.each([
    "",
    "abc",
    "48°60′",
    "48°30′60″",
    "48.5°30′",
    "48°30.5′",
    "1°2′3″4",
    "°",
    "48°x′",
    "10 -5",
    "10 15 -30",
  ])("rejects %j", (text) => {
    expect(parseAngle(text)).toBeNull();
  });
});
