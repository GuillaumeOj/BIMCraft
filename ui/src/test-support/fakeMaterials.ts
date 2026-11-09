import { ApiError, type MaterialCategory, type MaterialProperty } from "../api";

const refusal = (reason: string, extra: Record<string, unknown> = {}) =>
  new ApiError(-32000, "refused", { reason, ...extra });

type Template = Pick<MaterialProperty, "key" | "annex" | "quantity" | "required" | "range">;
const open = { min: null, min_inclusive: false, max: null, max_inclusive: false };
const positive = { min: 0, min_inclusive: false, max: null, max_inclusive: false };
const poisson = { min: 0, min_inclusive: true, max: 0.5, max_inclusive: false };

const TEMPLATES: Record<MaterialCategory, Template[]> = {
  concrete: [
    { key: "fck", quantity: "stress", required: true, range: positive },
    { key: "fcm", quantity: "stress", required: false, range: positive },
    { key: "e_modulus", quantity: "modulus", required: false, range: positive },
    { key: "poisson_ratio", quantity: "ratio", required: false, range: poisson },
    { key: "density", quantity: "density", required: false, range: positive },
    {
      key: "thermal_conductivity",
      quantity: "thermal_conductivity",
      required: false,
      range: positive,
    },
    { key: "gamma_c", annex: "recommended", quantity: "ratio", required: false, range: positive },
    { key: "gamma_c", annex: "fr", quantity: "ratio", required: false, range: positive },
  ],
  steel: [
    { key: "fy", quantity: "stress", required: true, range: positive },
    { key: "e_modulus", quantity: "modulus", required: false, range: positive },
    { key: "poisson_ratio", quantity: "ratio", required: false, range: poisson },
    { key: "density", quantity: "density", required: false, range: positive },
  ],
  reinforcement: [],
  timber: [],
  masonry: [],
  insulation: [
    {
      key: "thermal_conductivity",
      quantity: "thermal_conductivity",
      required: false,
      range: positive,
    },
  ],
  finish: [],
  generic: [{ key: "density", quantity: "density", required: false, range: open }],
};

type Grade = {
  key: string;
  name: string;
  category: MaterialCategory;
  design_code: string;
  values: Record<string, number>;
  document: string;
};

// Values are written under "key" or "key/annex".
export const GRADES: Grade[] = [
  {
    key: "en1992-1-1/C25/30",
    name: "C25/30",
    category: "concrete",
    design_code: "en1992-1-1",
    document: "EN 1992-1-1:2004",
    values: {
      fck: 25e6,
      fcm: 33e6,
      e_modulus: 31e9,
      density: 2500,
      thermal_conductivity: 2.3,
      "gamma_c/recommended": 1.5,
      "gamma_c/fr": 1.5,
    },
  },
  {
    key: "en1992-1-1/C30/37",
    name: "C30/37",
    category: "concrete",
    design_code: "en1992-1-1",
    document: "EN 1992-1-1:2004",
    values: { fck: 30e6, fcm: 38e6 },
  },
  {
    key: "en1993-1-1/S355",
    name: "S355",
    category: "steel",
    design_code: "en1993-1-1",
    document: "EN 1993-1-1:2005",
    values: { fy: 355e6, e_modulus: 210e9, poisson_ratio: 0.3 },
  },
];

type Stored = {
  id: string;
  name: string;
  category: MaterialCategory;
  grade: string | null;
  values: Record<string, { value: number; user: boolean }>;
  usedBy: number;
};

type Params = {
  id?: unknown;
  key?: unknown;
  name?: unknown;
  category?: unknown;
  material?: unknown;
  properties?: unknown;
  search?: string;
  offset?: number;
  limit?: number;
  start?: number;
};

const slot = (key: string, annex?: string) => (annex ? `${key}/${annex}` : key);

/** An in-memory `materials.*` and `history.*` for component tests. */
export function fakeMaterials() {
  let materials: Stored[] = [];
  let history: string[] = [];
  let snapshots: Stored[][] = [];
  // The project as it was before the first step of the history.
  let base: Stored[] = [];

  const commit = (command: string) => {
    snapshots.push(structuredClone(materials));
    history.push(command);
  };
  const find = (id: unknown) => {
    const material = materials.find((entry) => entry.id === id);
    if (!material) throw refusal("not_found");
    return material;
  };
  const gradeOf = (key: unknown) => {
    const grade = GRADES.find((entry) => entry.key === key);
    if (!grade) throw refusal("unknown_grade");
    return grade;
  };
  const checkName = (name: string, except?: string) => {
    const trimmed = name.trim();
    if (trimmed === "" || trimmed.length > 120) throw refusal("invalid_name");
    const clash = materials.some(
      (entry) => entry.id !== except && entry.name.toLowerCase() === trimmed.toLowerCase(),
    );
    if (clash) throw refusal("duplicate_name");
    return trimmed;
  };
  const source = (grade: Grade | undefined, user: boolean) =>
    user || !grade
      ? { entered_by_user: true as const }
      : { document: grade.document, clause: "Table 3.1" };

  const properties = (material: Stored, grade?: Grade): MaterialProperty[] =>
    TEMPLATES[material.category].map((template) => {
      const id = slot(template.key, template.annex);
      const stored = material.values[id];
      const library = grade?.values[id] ?? null;
      return {
        key: template.key,
        ...(template.annex ? { annex: template.annex } : {}),
        quantity: template.quantity,
        value: stored?.value ?? null,
        source: stored ? source(grade, stored.user) : null,
        library_value: library,
        range: template.range,
        required: template.required,
      };
    });

  const law = (category: MaterialCategory, graded: boolean) => ({
    kind: category === "concrete" ? ("parabola_rectangle" as const) : ("linear_elastic" as const),
    source: graded ? { document: "EN 1992-1-1:2004", clause: "3.1.7" } : null,
    branch: null,
  });

  const outsideRange = (template: Template, value: number) => {
    const { min, max, max_inclusive: maxInclusive, min_inclusive: minInclusive } = template.range;
    const tooLow = min !== null && (minInclusive ? value < min : value <= min);
    const tooHigh = max !== null && (maxInclusive ? value > max : value >= max);
    return tooLow || tooHigh || !Number.isFinite(value);
  };

  type Input = { key: string; annex?: string; value: number | null };

  const applyOne = (material: Stored, grade: Grade | undefined, input: Input) => {
    const template = TEMPLATES[material.category].find(
      (entry) => entry.key === input.key && entry.annex === input.annex,
    );
    if (!template) throw refusal("unknown_property", { key: input.key });
    const id = slot(input.key, input.annex);
    const library = grade?.values[id];
    if (input.value === null) {
      if (template.required && library === undefined) {
        throw refusal("required_property", { key: input.key });
      }
      if (library === undefined) delete material.values[id];
      else material.values[id] = { value: library, user: false };
      return;
    }
    if (outsideRange(template, input.value)) throw refusal("invalid_value", { key: input.key });
    material.values[id] = { value: input.value, user: true };
  };

  const apply = (material: Stored, inputs: Input[]) => {
    const grade = GRADES.find((entry) => entry.key === material.grade);
    for (const input of inputs) applyOne(material, grade, input);
  };

  const handlers: Record<string, (params: Params) => unknown> = {
    "materials.library": (params) => {
      const search = String(params.search ?? "").toLowerCase();
      const category = params.category;
      const found = GRADES.filter(
        (grade) =>
          (category === undefined || grade.category === category) &&
          (grade.name.toLowerCase().includes(search) || grade.key.toLowerCase().includes(search)),
      );
      const offset = Number(params.offset ?? 0);
      const limit = Number(params.limit ?? 100);
      return {
        total: found.length,
        items: found.slice(offset, offset + limit).map(({ key, name, category, design_code }) => ({
          key,
          name,
          category,
          design_code,
        })),
      };
    },
    "materials.grade": ({ key }) => {
      const grade = gradeOf(key);
      const stored: Stored = {
        id: "",
        name: grade.name,
        category: grade.category,
        grade: grade.key,
        values: Object.fromEntries(
          Object.entries(grade.values).map(([id, value]) => [id, { value, user: false }]),
        ),
        usedBy: 0,
      };
      return {
        key: grade.key,
        name: grade.name,
        category: grade.category,
        design_code: grade.design_code,
        law: law(grade.category, true),
        properties: properties(stored, grade),
      };
    },
    "materials.list": (params) => {
      const search = String(params.search ?? "").toLowerCase();
      return {
        items: materials
          .filter(
            (entry) =>
              (params.category === undefined || entry.category === params.category) &&
              entry.name.toLowerCase().includes(search),
          )
          .sort((a, b) => a.name.localeCompare(b.name))
          .map((entry) => ({
            id: entry.id,
            name: entry.name,
            category: entry.category,
            grade: entry.grade,
            used_by: entry.usedBy,
          })),
      };
    },
    "materials.get": ({ material }) => {
      const entry = find(material);
      const grade = GRADES.find((candidate) => candidate.key === entry.grade);
      const list = properties(entry, grade);
      return {
        id: entry.id,
        name: entry.name,
        category: entry.category,
        design_code: grade?.design_code ?? null,
        grade: entry.grade,
        law: law(entry.category, grade !== undefined),
        properties: list,
        used_by: entry.usedBy,
        edited: Object.values(entry.values).some((value) => value.user) && grade !== undefined,
      };
    },
    "materials.add_from_library": ({ id, key, name }) => {
      const grade = gradeOf(key);
      const chosen = checkName(typeof name === "string" ? name : grade.name);
      materials.push({
        id: String(id),
        name: chosen,
        category: grade.category,
        grade: grade.key,
        values: Object.fromEntries(
          Object.entries(grade.values).map(([slotId, value]) => [slotId, { value, user: false }]),
        ),
        usedBy: 0,
      });
      commit("materials.add_from_library");
      return { id };
    },
    "materials.create": ({ id, name, category }) => {
      const entry: Stored = {
        id: String(id),
        name: checkName(String(name)),
        category: category as MaterialCategory,
        grade: null,
        values: {},
        usedBy: 0,
      };
      materials.push(entry);
      commit("materials.create");
      return { id };
    },
    "materials.update": ({ material, name, properties: inputs }) => {
      const entry = find(material);
      const before = structuredClone(entry);
      try {
        if (typeof name === "string") entry.name = checkName(name, entry.id);
        apply(entry, (inputs ?? []) as { key: string; annex?: string; value: number | null }[]);
      } catch (error) {
        Object.assign(entry, before);
        throw error;
      }
      commit("materials.update");
      return {};
    },
    "materials.delete": ({ material }) => {
      const entry = find(material);
      if (entry.usedBy > 0) throw refusal("in_use", { used_by: entry.usedBy });
      materials = materials.filter((candidate) => candidate !== entry);
      commit("materials.delete");
      return {};
    },
    "history.state": (params) => {
      const start = Number(params.start ?? 0);
      const limit = Number(params.limit ?? 100);
      const entries = history
        .slice(start, start + limit)
        .map((command) => ({ command, origin: { kind: "ui" }, undone: false }));
      return {
        can_undo: history.length > 0,
        can_redo: false,
        entries,
        next: start + limit < history.length ? start + limit : null,
      };
    },
    "history.undo": () => {
      history.pop();
      snapshots.pop();
      materials = structuredClone(snapshots.at(-1) ?? base);
      return null;
    },
  };

  return {
    handlers,
    /** Makes `usedBy` objects use the material named `name`, as a build-up would. */
    setUsedBy(name: string, usedBy: number) {
      const entry = materials.find((candidate) => candidate.name === name);
      if (entry) entry.usedBy = usedBy;
    },
    /** The names of the project's materials. */
    names: () => materials.map((entry) => entry.name),
    /** Adds a material as if it had been in the project from the start. */
    seed(key: string, name?: string) {
      const grade = gradeOf(key);
      handlers["materials.add_from_library"]?.({
        id: `${materials.length + 1}`.padStart(32, "0"),
        key,
        name: name ?? grade.name,
      });
      history = [];
      snapshots = [];
      base = structuredClone(materials);
    },
    commands: [
      "materials.add_from_library",
      "materials.create",
      "materials.update",
      "materials.delete",
      "history.undo",
    ],
  };
}
