import type { ApiClient } from "./client";
import { ApiError } from "./jsonrpc";
import type { Commands, EntityId } from "./protocol.gen";

/** The parameters of `georef.set` the caller provides; the `Site` id is handled here. */
export type GeoreferenceValue = Omit<Commands["georef.set"]["params"], "site">;

/** `data.reason` of a refusal by the georeference commands, or null for any other failure. */
export function apiReason(error: unknown): string | null {
  if (!(error instanceof ApiError)) return null;
  const { data } = error;
  if (typeof data !== "object" || data === null || !("reason" in data)) return null;
  return typeof data.reason === "string" ? data.reason : null;
}

/** A fresh id for a new object: 128 random bits as 32 lower-case hexadecimal digits. */
export function newEntityId(): EntityId {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * Sets the project's georeference in one undoable step. The core wants the id of the `Site` only
 * when the project has none, so the first attempt sends none and a `missing_site_id` refusal is
 * retried once with a fresh id.
 */
export async function setGeoreference(
  api: ApiClient,
  value: GeoreferenceValue,
): Promise<{ site: EntityId }> {
  try {
    return await api.command("georef.set", value);
  } catch (error) {
    if (apiReason(error) !== "missing_site_id") throw error;
    return api.command("georef.set", { ...value, site: newEntityId() });
  }
}
