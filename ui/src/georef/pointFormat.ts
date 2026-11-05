import type { Language, MessageKey } from "../i18n/i18n";
import { formatLength } from "./formModel";
import {
  degToRad,
  formatDms,
  formatNumber,
  GEOGRAPHIC_DECIMALS,
  LATITUDE,
  LONGITUDE,
  parseAngle,
  parseNumber,
  radToDeg,
} from "./units";

/** The three systems a point can be read and entered in. */
export const SYSTEMS = ["local", "projected", "geographic"] as const;
export type System = (typeof SYSTEMS)[number];
/** x/y/z, E/N/H or latitude/longitude/height; geographic angles are in radians. */
export type Point = [number, number, number];
export type GeographicFormat = "decimal" | "dms";
export type Texts = [string, string, string];

/** The title of each block. */
export const BLOCK_TITLE: Record<System, MessageKey> = {
  local: "georef.block.local",
  projected: "georef.block.map",
  geographic: "georef.block.geographic",
};

/** The label of each of the three fields of a block. */
export const FIELD_LABELS: Record<System, readonly [MessageKey, MessageKey, MessageKey]> = {
  local: ["georef.axis.x", "georef.axis.y", "georef.axis.z"],
  projected: ["georef.field.easting", "georef.field.northing", "georef.field.height"],
  geographic: ["georef.field.latitude", "georef.field.longitude", "georef.field.ellipsoidHeight"],
};

/** The point as shown in the fields of `system`. */
export function formatPoint(
  system: System,
  point: Point,
  format: GeographicFormat,
  language: Language,
): Texts {
  if (system !== "geographic") {
    return [
      formatLength(point[0], language),
      formatLength(point[1], language),
      formatLength(point[2], language),
    ];
  }
  const angle = (radians: number, letters: typeof LATITUDE) =>
    format === "dms"
      ? formatDms(radToDeg(radians), letters, language)
      : formatNumber(radToDeg(radians), GEOGRAPHIC_DECIMALS, language);
  return [angle(point[0], LATITUDE), angle(point[1], LONGITUDE), formatLength(point[2], language)];
}

/** What the three typed values mean in `system`: the point, and which fields are invalid. */
export function parsePoint(
  system: System,
  texts: Texts,
): { point: Point | null; invalid: [boolean, boolean, boolean] } {
  const angle = system === "geographic";
  const values = texts.map((text, index) => {
    if (angle && index < 2) {
      const degrees = parseAngle(text);
      return degrees === null ? null : degToRad(degrees);
    }
    return parseNumber(text);
  });
  const [a, b, c] = values;
  const invalid: [boolean, boolean, boolean] = [a == null, b == null, c == null];
  return { point: a == null || b == null || c == null ? null : [a, b, c], invalid };
}

/** The text copied for a block: tab-separated, so that it pastes into spreadsheet cells. */
export function copyText(
  system: System,
  point: Point,
  format: GeographicFormat,
  language: Language,
): string {
  return formatPoint(system, point, format, language).join("\t");
}
