import { ApiError, type BuildUpFunction, type BuildUpLayerInput, type BuildUpUsage } from "../api";

const refusal = (reason: string, extra: Record<string, unknown> = {}) =>
  new ApiError(-32000, "refused", { reason, ...extra });

type Material = { id: string; name: string; category: string } | undefined;

type Layer = {
  id: string;
  material: string;
  thickness: number;
  function: BuildUpFunction;
  priority: number;
  core: boolean;
};

type Stored = {
  id: string;
  name: string;
  usage: BuildUpUsage;
  description: string | null;
  layers: Layer[];
};

type Params = {
  id?: string;
  buildup?: string;
  name?: string;
  usage?: BuildUpUsage;
  description?: string | null;
  layers?: BuildUpLayerInput[];
};

const PRIORITY: Record<BuildUpFunction, number> = {
  structure: 1,
  substrate: 2,
  insulation: 3,
  air_gap: 3,
  membrane: 4,
  finish: 5,
};

/**
 * An in-memory `buildups.*` for component tests. Its rules are the contract's, in miniature: names,
 * thickness, a contiguous core, generic materials in air gaps. Concrete has a conductivity of
 * 0.1 W/(m·K) for the thermal totals, other materials none.
 */
export function fakeBuildUps(deps: {
  lookup: (id: unknown) => Material;
  record: (command: string) => void;
}) {
  let buildUps: Stored[] = [];
  let counter = 0;

  const toLayer = (input: BuildUpLayerInput): Layer => ({
    id: input.id ?? `layer-${++counter}`,
    material: input.material,
    thickness: input.thickness,
    function: input.function,
    priority: input.priority ?? PRIORITY[input.function],
    core: input.core ?? input.function === "structure",
  });

  const layerErrors = (layer: BuildUpLayerInput, index: number) => {
    const errors: { code: string; index?: number }[] = [];
    const material = deps.lookup(layer.material);
    if (material === undefined) errors.push({ code: "unknown_material", index });
    else if (layer.function === "air_gap" && material.category !== "generic") {
      errors.push({ code: "invalid_material", index });
    }
    const membraneOk = layer.function === "membrane" && layer.thickness === 0;
    if (layer.thickness <= 0 && !membraneOk) errors.push({ code: "invalid_thickness", index });
    if (layer.priority !== undefined && (layer.priority < 1 || layer.priority > 5)) {
      errors.push({ code: "invalid_priority", index });
    }
    return errors;
  };

  const errorsOf = (layers: BuildUpLayerInput[]) => {
    const errors = layers.flatMap(layerErrors);
    if (layers.length === 0) errors.push({ code: "no_layers" });
    const core = layers.flatMap((layer, index) =>
      (layer.core ?? layer.function === "structure") ? [index] : [],
    );
    const [first, last] = [core[0], core.at(-1)];
    if (first !== undefined && last !== undefined && last - first + 1 !== core.length) {
      errors.push({ code: "core_not_contiguous" });
    }
    return errors;
  };

  const totals = (layers: Layer[]) => {
    const resistances = layers.map((layer) =>
      deps.lookup(layer.material)?.category === "concrete" ? layer.thickness / 0.1 : null,
    );
    const known = resistances.filter((value): value is number => value !== null);
    return {
      total_thickness: layers.reduce((sum, layer) => sum + layer.thickness, 0),
      thermal_resistance: known.length === 0 ? null : known.reduce((a, b) => a + b, 0),
      layers_without_thermal_data: resistances.length - known.length,
      resistances,
    };
  };

  const find = (id: unknown) => {
    const found = buildUps.find((entry) => entry.id === id);
    if (!found) throw refusal("not_found");
    return found;
  };

  const checkName = (name: string, except?: string) => {
    const trimmed = name.trim();
    if (trimmed === "" || trimmed.length > 120) throw refusal("invalid_name");
    if (
      buildUps.some(
        (entry) => entry.id !== except && entry.name.toLowerCase() === trimmed.toLowerCase(),
      )
    ) {
      throw refusal("duplicate_name");
    }
    return trimmed;
  };

  const checked = (layers: BuildUpLayerInput[]) => {
    const [first] = errorsOf(layers);
    if (first) throw refusal(first.code, first.index === undefined ? {} : { index: first.index });
    return layers.map(toLayer);
  };

  const handlers: Record<string, (params: Params) => unknown> = {
    "buildups.list": ({ usage }) => ({
      items: buildUps
        .filter((entry) => usage === undefined || entry.usage === usage)
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((entry) => ({
          id: entry.id,
          name: entry.name,
          usage: entry.usage,
          layer_count: entry.layers.length,
          total_thickness: totals(entry.layers).total_thickness,
        })),
    }),
    "buildups.get": ({ buildup }) => {
      const entry = find(buildup);
      const sums = totals(entry.layers);
      return {
        id: entry.id,
        name: entry.name,
        usage: entry.usage,
        description: entry.description,
        total_thickness: sums.total_thickness,
        thermal_resistance: sums.thermal_resistance,
        layers_without_thermal_data: sums.layers_without_thermal_data,
        layers: entry.layers.map((layer, index) => ({
          ...layer,
          material_name: deps.lookup(layer.material)?.name ?? "",
          name: null,
          thermal_resistance: sums.resistances[index] ?? null,
        })),
      };
    },
    "buildups.preview": ({ layers = [] }) => {
      const errors = errorsOf(layers);
      const stored = layers.map(toLayer);
      const sums = totals(stored);
      return {
        errors,
        layers: stored.map((layer, index) => ({
          priority: layer.priority,
          core: layer.core,
          thermal_resistance: sums.resistances[index] ?? null,
        })),
        total_thickness: sums.total_thickness,
        thermal_resistance: sums.thermal_resistance,
        layers_without_thermal_data: sums.layers_without_thermal_data,
      };
    },
    "buildups.create": ({ id = "", name = "", usage = "wall", description, layers = [] }) => {
      const made: Stored = {
        id,
        name: checkName(name),
        usage,
        description: description ?? null,
        layers: checked(layers),
      };
      buildUps.push(made);
      deps.record("buildups.create");
      return { id, layers: made.layers.map((layer) => layer.id) };
    },
    "buildups.update": ({ buildup, name, description, layers }) => {
      const entry = find(buildup);
      const update: Partial<Stored> = {};
      if (name !== undefined) update.name = checkName(name, entry.id);
      if (description !== undefined) update.description = description;
      if (layers !== undefined) {
        if (
          layers.some(
            (layer) => layer.id !== undefined && !entry.layers.some((own) => own.id === layer.id),
          )
        ) {
          throw refusal("unknown_layer");
        }
        update.layers = checked(layers);
      }
      Object.assign(entry, update);
      deps.record("buildups.update");
      return { layers: entry.layers.map((layer) => layer.id) };
    },
    "buildups.duplicate": ({ buildup, id = "", name = "" }) => {
      const entry = find(buildup);
      const copy: Stored = {
        ...entry,
        id,
        name: checkName(name),
        layers: entry.layers.map((layer) => ({ ...layer, id: `layer-${++counter}` })),
      };
      buildUps.push(copy);
      deps.record("buildups.duplicate");
      return { id, layers: copy.layers.map((layer) => layer.id) };
    },
    "buildups.delete": ({ buildup }) => {
      buildUps = buildUps.filter((entry) => entry !== find(buildup));
      deps.record("buildups.delete");
      return {};
    },
  };

  return {
    handlers,
    /** A build-up that was in the project from the start. */
    seed(name: string, usage: BuildUpUsage, layers: BuildUpLayerInput[]) {
      buildUps.push({
        id: `b${buildUps.length + 1}`.padEnd(32, "0"),
        name,
        usage,
        description: null,
        layers: layers.map(toLayer),
      });
    },
    names: () => buildUps.map((entry) => entry.name),
    /** The layers of the build-up named `name`, as stored. */
    layersOf: (name: string) => buildUps.find((entry) => entry.name === name)?.layers,
    commands: ["buildups.create", "buildups.update", "buildups.duplicate", "buildups.delete"],
  };
}
