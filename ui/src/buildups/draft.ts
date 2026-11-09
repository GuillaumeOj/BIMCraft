import type { BuildUpLayerInput } from "../api";
import {
  type BuildUpDetail,
  type BuildUpFunction,
  type BuildUpUsage,
  MAX_LAYERS,
  newEntityId,
} from "../api";
import { formatPlain, parseNumber } from "../georef/units";
import type { Language } from "../i18n/i18n";

/** One layer of a draft: what the user typed, with overrides kept apart from the defaults. */
export type DraftLayer = {
  /** Identifies the row while editing; the core's own id is `id`. */
  key: string;
  /** The layer's id when it already exists, so that Apply keeps it (ADR 0010). */
  id?: string;
  function: BuildUpFunction;
  material: string | null;
  /** Millimetres as typed, with `,` or `.`. */
  thickness: string;
  /** The user's join priority, or null while the function's default applies. */
  priority: number | null;
  /** The user's choice, or null while the function's default applies. */
  core: boolean | null;
};

/** A build-up being edited: nothing is saved until Apply. */
export type Draft = {
  name: string;
  description: string;
  usage: BuildUpUsage;
  layers: DraftLayer[];
};

/** The join priority a function gives a layer (api-contract.md); 1 runs through 5. */
export const DEFAULT_PRIORITY: Readonly<Record<BuildUpFunction, number>> = {
  structure: 1,
  substrate: 2,
  insulation: 3,
  air_gap: 3,
  membrane: 4,
  finish: 5,
};

/** The priorities a layer can be given. */
export const PRIORITIES: readonly number[] = [1, 2, 3, 4, 5];

/** Only a structural layer is part of the core by default. */
export const defaultCore = (layer: Pick<DraftLayer, "function">): boolean =>
  layer.function === "structure";

export const effectivePriority = (layer: DraftLayer): number =>
  layer.priority ?? DEFAULT_PRIORITY[layer.function];

export const effectiveCore = (layer: DraftLayer): boolean => layer.core ?? defaultCore(layer);

/** The starter layer of a new build-up, so that the table is never empty. */
export const newLayer = (): DraftLayer => ({
  key: newEntityId(),
  function: "structure",
  material: null,
  thickness: "",
  priority: null,
  core: null,
});

/** An empty draft of `usage` with its starter layer. */
export const newDraft = (usage: BuildUpUsage): Draft => ({
  name: "",
  description: "",
  usage,
  layers: [newLayer()],
});

/** The draft that edits a saved build-up; values equal to the defaults stay defaults. */
export function draftFromDetail(detail: BuildUpDetail, language: Language): Draft {
  return {
    name: detail.name,
    description: detail.description ?? "",
    usage: detail.usage,
    layers: detail.layers.map((layer) => ({
      key: layer.id,
      id: layer.id,
      function: layer.function,
      material: layer.material,
      thickness: formatPlain(roundMetres(layer.thickness * 1000), THICKNESS_DIGITS, language),
      priority: layer.priority === DEFAULT_PRIORITY[layer.function] ? null : layer.priority,
      core: layer.core === defaultCore(layer) ? null : layer.core,
    })),
  };
}

// Enough decimals for any thickness the core stores: Apply sends back what the field shows, so a
// rounded field would silently change the layer (0.25 mm must not become 0.3 mm).
const THICKNESS_DIGITS = 6;

// Multiplying by a power of ten can leave binary noise (0.0125 × 1000 = 12.500000000000002).
const roundMetres = (value: number): number => Number(value.toPrecision(12));

/** Metres from what the user typed in millimetres; 0 for text that is not a number. */
export function thicknessInMetres(text: string): number {
  const millimetres = parseNumber(text);
  return millimetres === null ? 0 : roundMetres(millimetres / 1000);
}

/** Whether `text` is a number of millimetres. */
export const isThickness = (text: string): boolean => parseNumber(text) !== null;

/**
 * The layers as the API takes them, in order; null while a layer still has no material, which the
 * core cannot be asked about. Defaults are left out so that a later change of function keeps them.
 */
export function layerInputs(draft: Draft): BuildUpLayerInput[] | null {
  const inputs: BuildUpLayerInput[] = [];
  for (const layer of draft.layers) {
    if (layer.material === null) return null;
    inputs.push({
      ...(layer.id === undefined ? {} : { id: layer.id }),
      material: layer.material,
      thickness: thicknessInMetres(layer.thickness),
      function: layer.function,
      ...(layer.priority === null ? {} : { priority: layer.priority }),
      ...(layer.core === null ? {} : { core: layer.core }),
    });
  }
  return inputs;
}

/** Whether the draft differs from `saved` (what Apply would change). */
export const isDirty = (draft: Draft, saved: Draft): boolean =>
  JSON.stringify(draft) !== JSON.stringify(saved);

export const canAddLayer = (draft: Draft): boolean => draft.layers.length < MAX_LAYERS;

/** The layers with the one at `from` moved to `to`; unchanged when `to` is outside the list. */
export function moveLayer(layers: readonly DraftLayer[], from: number, to: number): DraftLayer[] {
  const moved = layers[from];
  if (moved === undefined || to < 0 || to >= layers.length) return [...layers];
  const rest = layers.filter((_, index) => index !== from);
  return [...rest.slice(0, to), moved, ...rest.slice(to)];
}

/** The layers with a new one inserted after `index` (at the end when `index` is null). */
export function insertLayer(
  layers: readonly DraftLayer[],
  index: number | null,
  layer: DraftLayer,
): DraftLayer[] {
  const at = index === null ? layers.length : index + 1;
  return [...layers.slice(0, at), layer, ...layers.slice(at)];
}
