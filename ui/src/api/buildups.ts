import type { ApiClient } from "./client";
import { apiReason, newEntityId } from "./georef";
import { ApiError } from "./jsonrpc";
import { field } from "./materials";
import type {
  BuildUpFunction,
  BuildUpLayerInput,
  BuildUpUsage,
  EntityId,
  Queries,
} from "./protocol.gen";

/** One row of the build-up list (`buildups.list`). */
export type BuildUpSummary = Queries["buildups.list"]["result"]["items"][number];
/** A build-up with its layers and the server's totals (`buildups.get`). */
export type BuildUpDetail = Queries["buildups.get"]["result"];
/** The server's check of a draft: totals and every error at once (`buildups.preview`). */
export type BuildUpPreview = Queries["buildups.preview"]["result"];

/** The functions a layer can have, in the order the interface lists them. */
export const BUILDUP_FUNCTIONS: readonly BuildUpFunction[] = [
  "structure",
  "substrate",
  "insulation",
  "air_gap",
  "membrane",
  "finish",
];

/** The most layers a build-up can have. */
export const MAX_LAYERS = 32;

/** What the core refused a build-up call with: the reason and the layer it is about, if any. */
export type BuildUpRefusal = { code: string; index: number | null };

/** The refusal in `error`, or null when it is not one. */
export function buildUpRefusal(error: unknown): BuildUpRefusal | null {
  const code = apiReason(error);
  if (code === null || !(error instanceof ApiError)) return null;
  // `apiReason` found a reason, so the data is an object.
  const index = field(error.data as object, "index");
  return { code, index: typeof index === "number" ? index : null };
}

/** The build-ups, sorted as the core does; `usage` limits them to walls or slabs. */
export async function listBuildUps(
  api: ApiClient,
  usage?: BuildUpUsage,
): Promise<BuildUpSummary[]> {
  const { items } = await api.query("buildups.list", usage === undefined ? {} : { usage });
  return items;
}

/** One build-up with its layers. */
export const getBuildUp = (api: ApiClient, buildup: EntityId): Promise<BuildUpDetail> =>
  api.query("buildups.get", { buildup });

/** Checks a draft without saving it: the totals and every error. */
export const previewBuildUp = (
  api: ApiClient,
  usage: BuildUpUsage,
  layers: BuildUpLayerInput[],
): Promise<BuildUpPreview> => api.query("buildups.preview", { usage, layers });

/** Creates a build-up under a fresh id. */
export async function createBuildUp(
  api: ApiClient,
  value: { name: string; usage: BuildUpUsage; description?: string; layers: BuildUpLayerInput[] },
): Promise<EntityId> {
  const id = newEntityId();
  await api.command("buildups.create", { id, ...value });
  return id;
}

/** Replaces the name, description and layers of a build-up, in one undoable step. */
export async function updateBuildUp(
  api: ApiClient,
  buildup: EntityId,
  value: { name?: string; description?: string | null; layers?: BuildUpLayerInput[] },
): Promise<void> {
  await api.command("buildups.update", { buildup, ...value });
}

/** Copies a build-up under `name`; resolves to the copy's id. */
export async function duplicateBuildUp(
  api: ApiClient,
  buildup: EntityId,
  name: string,
): Promise<EntityId> {
  const id = newEntityId();
  await api.command("buildups.duplicate", { buildup, id, name });
  return id;
}

/** Deletes a build-up with its layers. */
export async function deleteBuildUp(api: ApiClient, buildup: EntityId): Promise<void> {
  await api.command("buildups.delete", { buildup });
}
