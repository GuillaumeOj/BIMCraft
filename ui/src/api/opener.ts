import { invoke, isTauri } from "@tauri-apps/api/core";

/** Whether the code runs in the desktop window rather than in a browser. */
export const inDesktopWindow = (): boolean => isTauri();

/**
 * Opens `url` in the system browser through the desktop window's opener plugin. The window
 * allows only the published user guide pages; any other address, and any failure, rejects.
 */
export async function openInSystemBrowser(url: string): Promise<void> {
  await invoke("plugin:opener|open_url", { url });
}
