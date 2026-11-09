import { buildUpRefusal } from "../api";
import type { Language, MessageKey } from "../i18n/i18n";

const KNOWN = new Set([
  "invalid_name",
  "duplicate_name",
  "no_layers",
  "too_many_layers",
  "invalid_thickness",
  "too_thick",
  "core_not_contiguous",
  "unknown_material",
  "invalid_material",
  "invalid_priority",
  "unknown_layer",
  "not_found",
]);

/** The message for an error code of the core; the generic one for a code the UI has no text for. */
export const errorMessage = (code: string): MessageKey =>
  KNOWN.has(code) ? (`buildups.error.${code}` as MessageKey) : "buildups.error.generic";

/** The message for a failed call, from its refusal's code. */
export const failureMessage = (error: unknown): MessageKey =>
  errorMessage(buildUpRefusal(error)?.code ?? "");

/** `base` + "One" or "Other" by the plural rules of `language` (0 is singular in French). */
export function pluralKey(language: Language, count: number, base: string): MessageKey {
  const form = new Intl.PluralRules(language).select(count) === "one" ? "One" : "Other";
  return `${base}${form}` as MessageKey;
}
