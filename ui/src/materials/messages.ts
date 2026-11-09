import { materialRefusal } from "../api";
import type { MessageKey } from "../i18n/i18n";

const BY_REASON: Readonly<Record<string, MessageKey>> = {
  invalid_name: "materials.error.invalid_name",
  duplicate_name: "materials.error.duplicate_name",
  invalid_value: "materials.error.invalid_value",
  required_property: "materials.error.required_property",
  unknown_grade: "materials.error.unknown_grade",
  not_found: "materials.error.not_found",
};

const NAME_ERRORS: readonly MessageKey[] = [
  "materials.error.invalid_name",
  "materials.error.duplicate_name",
];

/** The message for a refusal of a name, which is shown under the name field; else null. */
export function nameFailureMessage(error: unknown): MessageKey | null {
  const key = failureMessage(error);
  return NAME_ERRORS.includes(key) ? key : null;
}

/** The message for a failed call: the refusal's own, or the generic one. Never echoes values. */
export function failureMessage(error: unknown): MessageKey {
  const reason = materialRefusal(error)?.reason;
  return (reason !== undefined && BY_REASON[reason]) || "materials.error.generic";
}
