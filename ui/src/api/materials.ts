import type { ApiClient } from "./client";
import { newEntityId } from "./georef";
import { ApiError } from "./jsonrpc";
import type {
  EntityId,
  MaterialCategory,
  MaterialProperty,
  MaterialPropertyInput,
  Queries,
} from "./protocol.gen";

/** One row of the project's material list (`materials.list`). */
export type MaterialSummary = Queries["materials.list"]["result"]["items"][number];
/** A project material with every property its category gives (`materials.get`). */
export type MaterialDetail = Queries["materials.get"]["result"];
/** A library grade with its values (`materials.grade`). */
export type GradeDetail = Queries["materials.grade"]["result"];
/** One row of a library page (`materials.library`). */
export type GradeSummary = Queries["materials.library"]["result"]["items"][number];
/** A library page: the grades found and how many there are in all. */
export type LibraryPage = Queries["materials.library"]["result"];

/** The categories a material can have, in the order the interface lists them. */
export const MATERIAL_CATEGORIES: readonly MaterialCategory[] = [
  "concrete",
  "reinforcement",
  "steel",
  "timber",
  "masonry",
  "insulation",
  "finish",
  "generic",
];

/** The refusal of a `materials.*` call: `data.reason` with the property or count it names. */
export type MaterialRefusal = { reason: string; key: string | null; usedBy: number | null };

const asString = (value: unknown): string | null => (typeof value === "string" ? value : null);
const asNumber = (value: unknown): number | null => (typeof value === "number" ? value : null);

const field = (data: object, name: string): unknown =>
  name in data ? Reflect.get(data, name) : undefined;

/** What the core refused with, or null when `error` is not a refusal carrying a reason. */
export function materialRefusal(error: unknown): MaterialRefusal | null {
  if (!(error instanceof ApiError)) return null;
  const { data } = error;
  if (typeof data !== "object" || data === null) return null;
  const reason = asString(field(data, "reason"));
  if (reason === null) return null;
  // `model.delete` says `count` where `materials.delete` says `used_by`.
  return {
    reason,
    key: asString(field(data, "key")),
    usedBy: asNumber(field(data, "used_by")) ?? asNumber(field(data, "count")),
  };
}

/** The filter of the project list. */
export type MaterialFilter = { search?: string; category?: MaterialCategory | null };

/** The project's materials, sorted by name; an empty search or no category filters nothing. */
export async function listMaterials(
  api: ApiClient,
  filter: MaterialFilter = {},
): Promise<MaterialSummary[]> {
  const search = filter.search?.trim();
  const { items } = await api.query("materials.list", {
    ...(search ? { search } : {}),
    ...(filter.category ? { category: filter.category } : {}),
  });
  return items;
}

/** One project material with all its properties. */
export const getMaterial = (api: ApiClient, material: EntityId): Promise<MaterialDetail> =>
  api.query("materials.get", { material });

/** One page of the library. */
export function searchLibrary(
  api: ApiClient,
  filter: MaterialFilter & { offset?: number; limit?: number } = {},
): Promise<LibraryPage> {
  const search = filter.search?.trim();
  return api.query("materials.library", {
    ...(search ? { search } : {}),
    ...(filter.category ? { category: filter.category } : {}),
    ...(filter.offset === undefined ? {} : { offset: filter.offset }),
    ...(filter.limit === undefined ? {} : { limit: filter.limit }),
  });
}

/** A library grade with its values. */
export const getGrade = (api: ApiClient, key: string): Promise<GradeDetail> =>
  api.query("materials.grade", { key });

/** Copies library grade `key` into the project, optionally under another name. */
export async function addFromLibrary(
  api: ApiClient,
  key: string,
  name?: string,
): Promise<EntityId> {
  const id = newEntityId();
  await api.command("materials.add_from_library", {
    id,
    key,
    ...(name === undefined ? {} : { name }),
  });
  return id;
}

/** Creates a custom material; its values are all entered by the user. */
export async function createMaterial(
  api: ApiClient,
  name: string,
  category: MaterialCategory,
  properties?: MaterialPropertyInput[],
): Promise<EntityId> {
  const id = newEntityId();
  await api.command("materials.create", {
    id,
    name,
    category,
    ...(properties ? { properties } : {}),
  });
  return id;
}

/** Sets or resets (`value: null`) one property value, in one undoable step. */
export async function setPropertyValue(
  api: ApiClient,
  material: EntityId,
  property: Pick<MaterialProperty, "key" | "annex">,
  value: number | null,
): Promise<void> {
  await api.command("materials.update", {
    material,
    properties: [
      {
        key: property.key,
        ...(property.annex === undefined ? {} : { annex: property.annex }),
        value,
      },
    ],
  });
}

/** Renames a material. */
export async function renameMaterial(
  api: ApiClient,
  material: EntityId,
  name: string,
): Promise<void> {
  await api.command("materials.update", { material, name });
}

/** Deletes a material that nothing uses; refuses with `in_use` otherwise. */
export async function deleteMaterial(api: ApiClient, material: EntityId): Promise<void> {
  await api.command("materials.delete", { material });
}

/** Undoes the last step of the model's history. */
export async function undoLastStep(api: ApiClient): Promise<void> {
  await api.command("history.undo", {});
}

const HISTORY_PAGE = 1000;

/** How many steps of the history are applied, so a caller can tell whether the model moved on. */
export async function historyDepth(api: ApiClient): Promise<number> {
  let depth = 0;
  let start: number | null = 0;
  while (start !== null) {
    const page: Queries["history.state"]["result"] = await api.query("history.state", {
      start,
      limit: HISTORY_PAGE,
    });
    depth += page.entries.filter((entry) => !entry.undone).length;
    start = page.next;
  }
  return depth;
}
