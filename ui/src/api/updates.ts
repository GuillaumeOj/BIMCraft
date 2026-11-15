import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** The release channel a check looks at. The endpoint behind it lives in the host only. */
export type Channel = "stable" | "beta";

/** The running program (command `app_info`). */
export interface AppInfo {
  version: string;
  /** False in debug builds and builds without an updater key: the update controls stay hidden. */
  updatable: boolean;
}

/** A newer version found by a check. */
export interface UpdateOffer {
  version: string;
  /** Publication date, RFC 3339, when the manifest has one. */
  date: string | null;
  /** Release notes as plain text: never render them as HTML or Markdown. */
  notes: string;
}

/** Payload of the event `update://progress`. */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

/** The failure codes of the host's update commands, and `unknown` for anything else. */
export const UPDATE_ERROR_CODES = [
  "unavailable",
  "network",
  "manifest",
  "signature",
  "no_update",
  "not_downloaded",
  "install",
  "unknown",
] as const;
export type UpdateErrorCode = (typeof UPDATE_ERROR_CODES)[number];

/** A rejected update command. `message` is English, for logs: the UI words errors by `code`. */
export class UpdateError extends Error {
  readonly code: UpdateErrorCode;

  constructor(code: UpdateErrorCode, message: string) {
    super(message);
    this.name = "UpdateError";
    this.code = code;
  }
}

const PROGRESS_EVENT = "update://progress";

/** What `appInfo` reports without the Tauri IPC (browser, tests): nothing can be updated. */
const DEVELOPMENT_INFO: AppInfo = { version: "0.0.0-dev", updatable: false };

const isErrorCode = (value: unknown): value is UpdateErrorCode =>
  typeof value === "string" && (UPDATE_ERROR_CODES as readonly string[]).includes(value);

const hasTauriIpc = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Maps what Tauri rejects with (the host's `{ code, message }`, or a string) to an UpdateError. */
function toUpdateError(rejection: unknown): UpdateError {
  if (typeof rejection === "object" && rejection !== null && "code" in rejection) {
    const { code, message } = rejection as { code: unknown; message?: unknown };
    return new UpdateError(
      isErrorCode(code) ? code : "unknown",
      typeof message === "string" ? message : String(code),
    );
  }
  return new UpdateError("unknown", typeof rejection === "string" ? rejection : "update failed");
}

async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(name, args);
  } catch (rejection) {
    throw toUpdateError(rejection);
  }
}

/** The running version and whether this build can update itself. Never rejects. */
export async function appInfo(): Promise<AppInfo> {
  if (!hasTauriIpc()) return DEVELOPMENT_INFO;
  try {
    return await invoke<AppInfo>("app_info");
  } catch {
    return DEVELOPMENT_INFO;
  }
}

/** Looks for a newer version on `channel`; `null` when the running one is the newest. */
export const checkForUpdate = (channel: Channel): Promise<UpdateOffer | null> =>
  command<UpdateOffer | null>("update_check", { channel });

/**
 * Downloads and verifies the package of the last offer, reporting progress while it runs.
 * Resolves when the package is ready to install.
 */
export async function downloadUpdate(
  onProgress: (progress: UpdateProgress) => void,
): Promise<void> {
  // Progress is a courtesy: a download without a listener is still a download.
  const unlisten = await listen<UpdateProgress>(PROGRESS_EVENT, (event) =>
    onProgress(event.payload),
  ).catch(() => undefined);
  try {
    await command<null>("update_download");
  } finally {
    unlisten?.();
  }
}

/** Installs the downloaded package and restarts: on success the program exits, nothing returns. */
export const installUpdate = (): Promise<void> => command<void>("update_install");

/** The calls the update UI makes; tests and stories provide their own. */
export interface UpdatesApi {
  appInfo: typeof appInfo;
  checkForUpdate: typeof checkForUpdate;
  downloadUpdate: typeof downloadUpdate;
  installUpdate: typeof installUpdate;
}

export const updatesApi: UpdatesApi = { appInfo, checkForUpdate, downloadUpdate, installUpdate };
