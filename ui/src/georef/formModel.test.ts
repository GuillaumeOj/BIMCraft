import { describe, expect, it } from "vitest";
import type { CrsSummary } from "../api";
import { fakeCore, LAMBERT93 } from "../test-support/fakeCore";
import {
  COORDINATE_LIMIT,
  type FormValues,
  firstInvalid,
  formatRotation,
  initialValues,
  reasonMessage,
  toParams,
} from "./formModel";

const FILLED: FormValues = {
  crs: LAMBERT93 as CrsSummary,
  easting: "652 000,5",
  northing: "6862000",
  height: "35",
  rotation: "30",
  scale: "1",
  surveyEasting: "",
  surveyNorthing: "",
  surveyHeight: "",
};

describe("initialValues", () => {
  it("is empty, with rotation 0 and scale 1, for a project that is not georeferenced", () => {
    expect(initialValues(null, "decimal", "fr")).toEqual({
      crs: null,
      easting: "",
      northing: "",
      height: "",
      rotation: "0,0000",
      scale: "1",
      surveyEasting: "",
      surveyNorthing: "",
      surveyHeight: "",
    });
  });

  it("writes lengths with millimetres, the rotation in degrees and the survey point if any", async () => {
    const core = fakeCore({
      georeference: {
        crs_epsg: 2154,
        base_point: [1, 2, 3],
        grid_rotation: Math.PI / 2,
        scale: 0.9999,
        survey_point: [4, 5, 6],
      },
    });
    const { georeference } = await core.client.query("georef.get", {});
    expect(initialValues(georeference, "dms", "en")).toMatchObject({
      easting: "1.000",
      rotation: "90°0′0.0000″",
      scale: "0.9999",
      surveyNorthing: "5.000",
    });
  });
});

describe("formatRotation", () => {
  it("formats radians as degrees", () => {
    expect(formatRotation(Math.PI, "decimal", "en")).toBe("180.0000");
  });
});

describe("toParams", () => {
  it("reads lengths with either separator, the angle in degrees and the scale", () => {
    const result = toParams(FILLED);
    expect(result).toMatchObject({
      params: { crs_epsg: 2154, base_point: [652000.5, 6862000, 35], scale: 1, survey_point: null },
    });
    expect("params" in result && result.params.grid_rotation).toBeCloseTo(Math.PI / 6, 12);
  });

  it("includes a complete survey point", () => {
    const result = toParams({
      ...FILLED,
      surveyEasting: "1",
      surveyNorthing: "2",
      surveyHeight: "3",
    });
    expect(result).toMatchObject({ params: { survey_point: [1, 2, 3] } });
  });

  it("reports every invalid field with its message", () => {
    const result = toParams({
      ...FILLED,
      crs: null,
      easting: "x",
      height: String(COORDINATE_LIMIT * 2),
      rotation: "??",
      scale: "0",
      surveyEasting: "1",
    });
    expect(result).toEqual({
      errors: {
        crs: "georef.error.required",
        easting: "georef.error.number",
        height: "georef.error.number",
        rotation: "georef.error.angle",
        scale: "georef.error.invalid_scale",
        surveyNorthing: "georef.error.survey",
        surveyHeight: "georef.error.survey",
      },
    });
  });

  it("refuses a scale that is not a number, and the ends of the allowed range are valid", () => {
    expect(toParams({ ...FILLED, scale: "x" })).toMatchObject({
      errors: { scale: "georef.error.number" },
    });
    expect(toParams({ ...FILLED, scale: "0,000001" })).toHaveProperty("params");
    expect(toParams({ ...FILLED, scale: "1000000" })).toHaveProperty("params");
    expect(toParams({ ...FILLED, scale: "1000001" })).toHaveProperty("errors");
  });
});

describe("firstInvalid", () => {
  it("follows the order of the form, not of the errors", () => {
    expect(firstInvalid({ scale: "georef.error.number", northing: "georef.error.number" })).toBe(
      "northing",
    );
    expect(firstInvalid({})).toBeUndefined();
  });
});

describe("reasonMessage", () => {
  it("maps the reasons the UI knows and falls back to the generic message", () => {
    expect(reasonMessage("outside_domain")).toBe("georef.error.outside_domain");
    expect(reasonMessage("missing_site_id")).toBe("georef.error.generic");
    expect(reasonMessage(null)).toBe("georef.error.generic");
  });
});
