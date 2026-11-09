import type { MaterialProperty } from "../api";
import { formatPlain, parseNumber } from "../georef/units";
import type { Language } from "../i18n/i18n";

/** How a property is shown: its unit symbol and the factor from the SI value on the wire. */
export type DisplayUnit = { unit: string; factor: number };

const BY_QUANTITY: Readonly<Record<MaterialProperty["quantity"], DisplayUnit>> = {
  stress: { unit: "MPa", factor: 1e-6 },
  modulus: { unit: "GPa", factor: 1e-9 },
  density: { unit: "kg/m³", factor: 1 },
  strain: { unit: "‰", factor: 1000 },
  ratio: { unit: "", factor: 1 },
  thermal_conductivity: { unit: "W/(m·K)", factor: 1 },
  specific_heat: { unit: "J/(kg·K)", factor: 1 },
};

// The linear thermal expansion is a ratio per kelvin, of the order of 10⁻⁵.
const THERMAL_EXPANSION: DisplayUnit = { unit: "10⁻⁶/K", factor: 1e6 };

/** The decimals shown: three, trimmed of trailing zeros (UX decision 5). */
const DECIMALS = 3;

/** The unit a property is shown and edited in; fixed per property, never chosen by magnitude. */
export const displayUnit = (property: Pick<MaterialProperty, "key" | "quantity">): DisplayUnit =>
  property.key === "thermal_expansion" ? THERMAL_EXPANSION : BY_QUANTITY[property.quantity];

// Multiplying by a power of ten can leave binary noise (0.0025 × 1000 = 2.5000000000000004).
const clean = (value: number): number => Number(value.toPrecision(12));

/** `si` in the property's unit, as text with the decimal separator of `language`. */
export function formatValue(
  property: Pick<MaterialProperty, "key" | "quantity">,
  si: number,
  language: Language,
): string {
  return formatPlain(clean(si * displayUnit(property).factor), DECIMALS, language);
}

/** `si` with its unit, such as "25 MPa" (a non-breaking space keeps them together). */
export function formatWithUnit(
  property: Pick<MaterialProperty, "key" | "quantity">,
  si: number,
  language: Language,
): string {
  const { unit } = displayUnit(property);
  const text = formatValue(property, si, language);
  return unit === "" ? text : `${text} ${unit}`;
}

/** The SI value of what the user typed in the property's unit (`,` or `.`), or null. */
export function parseValue(
  property: Pick<MaterialProperty, "key" | "quantity">,
  text: string,
): number | null {
  const typed = parseNumber(text);
  return typed === null ? null : clean(typed / displayUnit(property).factor);
}

/** The accepted range as "min and max" texts; null for a bound the range does not have. */
export function formatBounds(
  property: Pick<MaterialProperty, "key" | "quantity" | "range">,
  language: Language,
): { min: string | null; max: string | null } {
  const { min, max } = property.range;
  return {
    min: min === null ? null : formatValue(property, min, language),
    max: max === null ? null : formatValue(property, max, language),
  };
}

/** The properties of the Thermal tab; all the others are mechanical. */
export const THERMAL_KEYS: readonly string[] = [
  "thermal_conductivity",
  "specific_heat",
  "thermal_expansion",
];

export type PropertyTab = "mechanical" | "thermal";

/** The tab a property belongs to. */
export const tabOf = (property: Pick<MaterialProperty, "key">): PropertyTab =>
  THERMAL_KEYS.includes(property.key) ? "thermal" : "mechanical";

/** Symbols of the properties: standard notation, so they are not translated. */
export const SYMBOLS: Readonly<Record<string, string>> = {
  density: "ρ",
  e_modulus: "E",
  shear_modulus: "G",
  poisson_ratio: "ν",
  thermal_expansion: "α",
  thermal_conductivity: "λ",
  specific_heat: "c",
  fck: "fck",
  fck_cube: "fck,cube",
  fcm: "fcm",
  fctm: "fctm",
  eps_c2: "εc2",
  eps_cu2: "εcu2",
  n: "n",
  eps_c3: "εc3",
  eps_cu3: "εcu3",
  fy: "fy",
  fu: "fu",
  k: "k",
  eps_u: "εu",
  fm_k: "fm,k",
  ft0_k: "ft,0,k",
  fc0_k: "fc,0,k",
  fv_k: "fv,k",
  e0_mean: "E0,mean",
  e0_05: "E0,05",
  g_mean: "Gmean",
  rho_k: "ρk",
  rho_mean: "ρmean",
  fk: "fk",
  gamma_c: "γc",
  alpha_cc: "αcc",
  gamma_s: "γs",
  gamma_m0: "γM0",
  gamma_m1: "γM1",
  gamma_m2: "γM2",
  gamma_m: "γM",
};

/** A design code key such as "en1992-1-1" written as people do: "EN 1992-1-1". */
export const designCodeLabel = (key: string): string => key.replace(/^en(?=\d)/, "EN ");
