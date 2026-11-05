import type { CrsSummary, GeoreferenceValue } from "../api";
import type { Language, MessageKey } from "../i18n/i18n";
import type { GeorefInfo } from "./GeorefContext";
import {
  ANGLE_DECIMALS,
  degToRad,
  formatDms,
  formatNumber,
  formatPlain,
  LENGTH_DECIMALS,
  parseAngle,
  parseNumber,
  radToDeg,
} from "./units";

/** The scale the core accepts (`invalid_scale` outside it). */
export const SCALE_MIN = 1e-6;
export const SCALE_MAX = 1e6;
/** The largest projected coordinate the core accepts, in metres. */
export const COORDINATE_LIMIT = 1e9;

export type AngleFormat = "decimal" | "dms";

/** What the user typed, field by field; numbers stay text until Apply. */
export type FormValues = {
  crs: CrsSummary | null;
  easting: string;
  northing: string;
  height: string;
  rotation: string;
  scale: string;
  surveyEasting: string;
  surveyNorthing: string;
  surveyHeight: string;
};

export type FieldKey = Exclude<keyof FormValues, "crs"> | "crs";
/** Fields in the order of the form, which is also the focus order. */
export const FIELD_ORDER: readonly FieldKey[] = [
  "crs",
  "easting",
  "northing",
  "height",
  "rotation",
  "scale",
  "surveyEasting",
  "surveyNorthing",
  "surveyHeight",
];
export type FieldErrors = Partial<Record<FieldKey, MessageKey | undefined>>;

const SURVEY_FIELDS = ["surveyEasting", "surveyNorthing", "surveyHeight"] as const;

export const formatLength = (value: number, language: Language) =>
  formatNumber(value, LENGTH_DECIMALS, language);

/** The rotation as shown in the field: radians in, degrees (or DMS) out. */
export function formatRotation(radians: number, format: AngleFormat, language: Language): string {
  const degrees = radToDeg(radians);
  return format === "dms"
    ? formatDms(degrees, null, language)
    : formatNumber(degrees, ANGLE_DECIMALS, language);
}

/** The form for the project's current georeference, or the empty form when it has none. */
export function initialValues(
  info: GeorefInfo | null,
  format: AngleFormat,
  language: Language,
): FormValues {
  if (info === null) {
    return {
      crs: null,
      easting: "",
      northing: "",
      height: "",
      rotation: formatRotation(0, format, language),
      scale: formatPlain(1, 6, language),
      surveyEasting: "",
      surveyNorthing: "",
      surveyHeight: "",
    };
  }
  const { value } = info;
  const survey = (index: number) =>
    value.survey_point === null ? "" : formatLength(value.survey_point[index] ?? 0, language);
  return {
    crs: info.crs,
    easting: formatLength(value.base_point[0], language),
    northing: formatLength(value.base_point[1], language),
    height: formatLength(value.base_point[2], language),
    rotation: formatRotation(value.grid_rotation, format, language),
    scale: formatPlain(value.scale, 6, language),
    surveyEasting: survey(0),
    surveyNorthing: survey(1),
    surveyHeight: survey(2),
  };
}

/** A length typed by the user: a finite number within the core's bound, or null. */
function length(text: string): number | null {
  const value = parseNumber(text);
  return value !== null && Math.abs(value) <= COORDINATE_LIMIT ? value : null;
}

/** The three typed values of a point, or null when one is not a valid length. */
function point(texts: readonly string[]): [number, number, number] | null {
  const [e, n, h] = texts.map(length);
  return e == null || n == null || h == null ? null : [e, n, h];
}

function scaleError(scale: number | null): MessageKey | undefined {
  if (scale === null) return "georef.error.number";
  return scale >= SCALE_MIN && scale <= SCALE_MAX ? undefined : "georef.error.invalid_scale";
}

/** A partly filled survey point is an error on each of its empty or invalid fields. */
function surveyErrors(values: FormValues): FieldErrors {
  const texts = SURVEY_FIELDS.map((key) => values[key]);
  if (texts.every((text) => text.trim() === "") || point(texts) !== null) return {};
  return Object.fromEntries(
    SURVEY_FIELDS.filter((key) => length(values[key]) === null).map((key) => [
      key,
      values[key].trim() === "" ? "georef.error.survey" : "georef.error.number",
    ]),
  );
}

/** The error of each invalid base-point length field. */
function baseErrors(values: FormValues): FieldErrors {
  return Object.fromEntries(
    (["easting", "northing", "height"] as const)
      .filter((key) => length(values[key]) === null)
      .map((key) => [key, "georef.error.number"]),
  );
}

/**
 * The parameters of `georef.set` for the typed `values`, or the message of each invalid field.
 * An empty survey point is none; a partly filled one is an error on its empty fields.
 */
export function toParams(
  values: FormValues,
): { params: GeoreferenceValue } | { errors: FieldErrors } {
  const rotation = parseAngle(values.rotation);
  const scale = parseNumber(values.scale);
  const base = point([values.easting, values.northing, values.height]);
  const errors: FieldErrors = {
    ...baseErrors(values),
    ...surveyErrors(values),
    ...(values.crs === null && { crs: "georef.error.required" as const }),
    ...(rotation === null && { rotation: "georef.error.angle" as const }),
    ...(scaleError(scale) !== undefined && { scale: scaleError(scale) }),
  };
  if (Object.keys(errors).length > 0 || values.crs === null || base === null) return { errors };
  return {
    params: {
      crs_epsg: values.crs.epsg,
      base_point: base,
      grid_rotation: degToRad(rotation ?? 0),
      scale: scale ?? 1,
      survey_point: point(SURVEY_FIELDS.map((key) => values[key])),
    },
  };
}

/** The first field with an error, in form order. */
export const firstInvalid = (errors: FieldErrors): FieldKey | undefined =>
  FIELD_ORDER.find((key) => errors[key] !== undefined);

const KNOWN_REASONS = [
  "unknown_crs",
  "not_projected",
  "invalid_scale",
  "outside_domain",
  "multiple_sites",
  "not_georeferenced",
] as const;
export type Reason = (typeof KNOWN_REASONS)[number];

/** The message for a refusal's `data.reason`; unknown reasons get the generic one. */
export function reasonMessage(reason: string | null): MessageKey {
  const known = KNOWN_REASONS.find((candidate) => candidate === reason);
  return known === undefined ? "georef.error.generic" : `georef.error.${known}`;
}

/** The text of the CRS row: code and name. */
export const crsLabel = (crs: CrsSummary): string => `${crs.epsg} – ${crs.name}`;
