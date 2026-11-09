import type { MessageKey } from "../i18n/i18n";
import { type Draft, effectiveCore, isThickness } from "./draft";
import { errorMessage } from "./messages";

/** An error of the core: its code and the layer it is about (from 0), if any. */
export type CoreError = { code: string; index?: number | undefined };

/** The messages of one layer row, by control. */
export type RowErrors = {
  thickness?: MessageKey;
  material?: MessageKey;
  priority?: MessageKey;
  /** Shown in the message row: the layer sits between core layers. */
  core?: MessageKey;
  /** The layer is part of the core span, or sits inside it: it gets the error rule. */
  inCoreSpan: boolean;
};

export type EditorErrors = {
  name: MessageKey | null;
  /** About the layers as a group, shown above the table. */
  group: MessageKey[];
  rows: RowErrors[];
  /** Messages that belong to no field: shown in the banner. */
  banner: MessageKey | null;
  /** How many messages there are, for the announcement. */
  count: number;
};

const NAME_CODES = new Set(["invalid_name", "duplicate_name"]);
const GROUP_CODES = new Set(["no_layers", "too_many_layers"]);
const ROW_FIELDS: Readonly<Record<string, "thickness" | "material" | "priority">> = {
  invalid_thickness: "thickness",
  too_thick: "thickness",
  unknown_material: "material",
  invalid_material: "material",
  invalid_priority: "priority",
};

/** The span of core layers, from the first to the last, as indexes; null without a core. */
function coreSpan(draft: Draft): [number, number] | null {
  const core = draft.layers.flatMap((layer, index) => (effectiveCore(layer) ? [index] : []));
  const [first, last] = [core[0], core.at(-1)];
  return first === undefined || last === undefined ? null : [first, last];
}

function clientErrors(draft: Draft): CoreError[] {
  const name = draft.name.trim();
  const errors: CoreError[] =
    name === "" || [...name].length > 120 ? [{ code: "invalid_name" }] : [];
  draft.layers.forEach((layer, index) => {
    if (layer.material === null) errors.push({ code: "material_required", index });
    if (!isThickness(layer.thickness)) errors.push({ code: "invalid_thickness", index });
  });
  return errors;
}

/**
 * Every message for `draft`: its own checks (name, material, number) and the core's `errors`
 * (from `buildups.preview` or a refusal), mapped to the field they are about.
 */
export function buildErrors(draft: Draft, coreErrors: readonly CoreError[]): EditorErrors {
  const rows: RowErrors[] = draft.layers.map(() => ({ inCoreSpan: false }));
  const result: EditorErrors = { name: null, group: [], rows, banner: null, count: 0 };
  let broken = false;
  for (const error of [...clientErrors(draft), ...coreErrors]) {
    if (error.code === "core_not_contiguous") broken = true;
    else place(result, error);
  }
  const span = coreSpan(draft);
  if (broken && span !== null) markCore(draft, rows, span, result);
  result.group = [...new Set(result.group)];
  result.count = countMessages(result);
  return result;
}

/** Puts one error where its field is: the name, the group, a row, or else the banner. */
function place(result: EditorErrors, error: CoreError) {
  const row = error.index === undefined ? undefined : result.rows[error.index];
  const field = ROW_FIELDS[error.code];
  if (NAME_CODES.has(error.code)) result.name = errorMessage(error.code);
  else if (GROUP_CODES.has(error.code)) result.group.push(errorMessage(error.code));
  else if (error.code === "material_required" && row)
    row.material = "buildups.error.material_required";
  else if (field !== undefined && row) row[field] = errorMessage(error.code);
  else result.banner = errorMessage(error.code);
}

function markCore(
  draft: Draft,
  rows: RowErrors[],
  [first, last]: [number, number],
  result: EditorErrors,
) {
  result.group.push("buildups.error.core_not_contiguous");
  draft.layers.forEach((layer, index) => {
    const row = rows[index];
    if (row === undefined || index < first || index > last) return;
    row.inCoreSpan = true;
    if (!effectiveCore(layer)) row.core = "buildups.error.core_between";
  });
}

function countMessages(errors: EditorErrors): number {
  const rowMessages = errors.rows.flatMap((row) => [
    row.thickness,
    row.material,
    row.priority,
    row.core,
  ]);
  const all = [errors.name, errors.banner, ...errors.group, ...rowMessages];
  return all.filter((message) => message !== null && message !== undefined).length;
}

/** No message at all: what an editor shows before the first Apply. */
export const noErrors = (draft: Draft): EditorErrors => ({
  name: null,
  group: [],
  rows: draft.layers.map(() => ({ inCoreSpan: false })),
  banner: null,
  count: 0,
});
