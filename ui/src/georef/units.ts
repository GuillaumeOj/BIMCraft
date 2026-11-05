import type { Language } from "../i18n/i18n";

/** Decimals shown for lengths (mm), angles in degrees, and geographic degrees (about 1 mm). */
export const LENGTH_DECIMALS = 3;
export const ANGLE_DECIMALS = 4;
export const GEOGRAPHIC_DECIMALS = 8;
/** Decimals of an arc second: about 3 mm on the ground. */
export const SECOND_DECIMALS = 4;

export const radToDeg = (radians: number): number => (radians * 180) / Math.PI;
export const degToRad = (degrees: number): number => (degrees * Math.PI) / 180;

const NUMBER = /^[+-]?(\d+[.,]?\d*|[.,]\d+)$/;

/**
 * A number typed by the user, or null. A single separator is the decimal one; when `.` and `,`
 * both appear the last one is, and the other is grouping; a separator repeated is grouping. Spaces
 * (also the no-break ones) are ignored (ADR 0004, UI-04).
 */
export function parseNumber(text: string): number | null {
  let clean = text.replace(/[\s  ]/g, "").replace(/−/g, "-");
  const dot = clean.lastIndexOf(".");
  const comma = clean.lastIndexOf(",");
  if (dot !== -1 && comma !== -1) {
    const grouping = dot > comma ? "," : ".";
    clean = clean.replaceAll(grouping, "");
  } else if ((clean.match(/[.,]/g) ?? []).length > 1) {
    clean = clean.replace(/[.,]/g, "");
  }
  if (!NUMBER.test(clean)) return null;
  return Number(clean.replace(",", "."));
}

/** `value` with `decimals` decimals and the decimal separator of `language`, no grouping. */
export function formatNumber(value: number, decimals: number, language: Language): string {
  // Adding 0 turns a rounded -0 into 0, so that -0.0001 does not read "-0.000".
  const rounded = Number(value.toFixed(decimals)) + 0;
  return new Intl.NumberFormat(language, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
    useGrouping: false,
  }).format(rounded);
}

/** `value` with at most `decimals` decimals, none when it is whole: the scale. */
export function formatPlain(value: number, decimals: number, language: Language): string {
  return new Intl.NumberFormat(language, {
    maximumFractionDigits: decimals,
    useGrouping: false,
  }).format(value);
}

const MINUS = "−";

/** `value` signed as the user reads it, with a true minus sign, for the lists of derived values. */
export const formatSigned = (value: number, decimals: number, language: Language): string =>
  formatNumber(value, decimals, language).replace("-", MINUS);

/** Hemisphere letters: positive first. */
export type Hemispheres = readonly [positive: string, negative: string];
export const LATITUDE: Hemispheres = ["N", "S"];
export const LONGITUDE: Hemispheres = ["E", "W"];

/**
 * Degrees, minutes and seconds of `degrees`, such as `48°51′10,8000″N`; with no `hemispheres`, a
 * negative angle starts with a minus sign instead.
 */
export function formatDms(
  degrees: number,
  hemispheres: Hemispheres | null,
  language: Language,
): string {
  const unit = 10 ** SECOND_DECIMALS;
  // Integer ten-thousandths of a second, so that rounding cannot produce 60 seconds.
  const total = Math.round(Math.abs(degrees) * 3600 * unit);
  const whole = Math.floor(total / (3600 * unit));
  const minutes = Math.floor((total - whole * 3600 * unit) / (60 * unit));
  const seconds = (total - whole * 3600 * unit - minutes * 60 * unit) / unit;
  const negative = degrees < 0 && total !== 0;
  const sign = negative && hemispheres === null ? "-" : "";
  const letter = hemispheres?.[negative ? 1 : 0] ?? "";
  return `${sign}${whole}°${minutes}′${formatNumber(seconds, SECOND_DECIMALS, language)}″${letter}`;
}

const NEGATIVE_LETTERS = "SWO";
const DMS_MARKS = /[°′″'"]/;
const HEMISPHERE = /^([NSEWO])|([NSEWO])$/i;

/** Degrees of an angle typed as decimal degrees or as degrees, minutes and seconds; or null. */
export function parseAngle(text: string): number | null {
  let body = text.trim();
  const letter = (HEMISPHERE.exec(body)?.slice(1).find(Boolean) ?? "").toUpperCase();
  body = body.replace(HEMISPHERE, "").trim();
  const sign = letter !== "" && NEGATIVE_LETTERS.includes(letter) ? -1 : 1;
  const value = DMS_MARKS.test(body) ? parseDms(body) : parseNumber(body);
  return value === null ? null : sign * value;
}

function parseDms(body: string): number | null {
  const negative = /^[-−]/.test(body);
  const parts = body
    .replace(/^[-+−]/, "")
    .split(/[°′″'"\s]+/)
    .filter((part) => part !== "");
  const [degrees, minutes = "0", seconds = "0"] = parts;
  if (parts.length > 3 || degrees === undefined) return null;
  const [d, m, s] = [parseNumber(degrees), parseNumber(minutes), parseNumber(seconds)];
  if (d === null || m === null || s === null || m >= 60 || s >= 60) return null;
  if (!Number.isInteger(d) || !Number.isInteger(m)) return null;
  const value = d + m / 60 + s / 3600;
  return negative ? -value : value;
}
