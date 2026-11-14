import type { ApiClient } from "./client";
import { apiReason, newEntityId } from "./georef";
import type { EntityId } from "./protocol.gen";

type Vector = [number, number, number];

/** Why `elements.move` refused a move (api-contract.md §3). */
export type MoveRefusal = "unknown_element" | "not_movable" | "out_of_range" | "empty" | "too_many";

const MOVE_REFUSALS: readonly string[] = [
  "unknown_element",
  "not_movable",
  "out_of_range",
  "empty",
  "too_many",
] satisfies MoveRefusal[];

/** The reason `elements.move` refused with, or null for any other failure. */
export function moveRefusal(error: unknown): MoveRefusal | null {
  const reason = apiReason(error);
  return reason !== null && MOVE_REFUSALS.includes(reason) ? (reason as MoveRefusal) : null;
}

/** Moves the elements `ids` by `translation` (metres), in one undoable step. */
export async function moveElements(
  api: ApiClient,
  ids: readonly EntityId[],
  translation: Vector,
): Promise<void> {
  await api.command("elements.move", { ids: [...ids], translation });
}

/** A block to place: the centre of its base, its turn about z (radians) and its sizes (metres). */
export type BlockPlacement = { location: Vector; rotation: number; size: Vector };

const real = (value: number) => ({ Real: value });

/**
 * Creates a generic block (spec D1) in one undoable step and resolves to its id. The attribute
 * encoding is the schema's (api-contract.md §3): `location` is a list of three lengths.
 */
export async function placeBlock(api: ApiClient, block: BlockPlacement): Promise<EntityId> {
  const id = newEntityId();
  const [sizeX, sizeY, sizeZ] = block.size;
  await api.command("model.create", {
    entity: "Block",
    id,
    values: [
      ["location", { List: block.location.map(real) }],
      ["rotation", real(block.rotation)],
      ["size_x", real(sizeX)],
      ["size_y", real(sizeY)],
      ["size_z", real(sizeZ)],
    ],
  });
  return id;
}
