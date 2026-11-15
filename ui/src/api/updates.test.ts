import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  appInfo,
  checkForUpdate,
  downloadUpdate,
  installUpdate,
  UpdateError,
  updatesApi,
} from "./updates";

const tauri = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

beforeEach(() => {
  tauri.invoke.mockReset();
  tauri.listen.mockReset();
});
afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

const withTauri = () => Object.assign(window, { __TAURI_INTERNALS__: {} });

describe("appInfo", () => {
  it("reports a development build that cannot update when the Tauri IPC is absent", async () => {
    await expect(appInfo()).resolves.toEqual({ version: "0.0.0-dev", updatable: false });
    expect(tauri.invoke).not.toHaveBeenCalled();
  });

  it("asks the host when the Tauri IPC is present", async () => {
    withTauri();
    tauri.invoke.mockResolvedValue({ version: "0.3.0", updatable: true });
    await expect(appInfo()).resolves.toEqual({ version: "0.3.0", updatable: true });
    expect(tauri.invoke).toHaveBeenCalledWith("app_info");
  });

  it("never rejects: a failing host means nothing can be updated", async () => {
    withTauri();
    tauri.invoke.mockRejectedValue("no such command");
    await expect(appInfo()).resolves.toEqual({ version: "0.0.0-dev", updatable: false });
  });
});

describe("checkForUpdate", () => {
  it("names the channel and returns the offer or null", async () => {
    const offer = { version: "0.4.0", date: null, notes: "Fixes" };
    tauri.invoke.mockResolvedValueOnce(offer).mockResolvedValueOnce(null);
    await expect(checkForUpdate("beta")).resolves.toEqual(offer);
    await expect(checkForUpdate("stable")).resolves.toBeNull();
    expect(tauri.invoke).toHaveBeenNthCalledWith(1, "update_check", { channel: "beta" });
    expect(tauri.invoke).toHaveBeenNthCalledWith(2, "update_check", { channel: "stable" });
  });

  it.each([
    "unavailable",
    "network",
    "manifest",
    "signature",
    "no_update",
    "not_downloaded",
    "install",
  ] as const)("maps the host's %s rejection to an UpdateError with that code", async (code) => {
    tauri.invoke.mockRejectedValue({ code, message: `host says ${code}` });
    const error = await checkForUpdate("stable").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(UpdateError);
    expect(error).toMatchObject({ code, message: `host says ${code}` });
  });

  it("falls back to the code as message when the host sends none", async () => {
    tauri.invoke.mockRejectedValue({ code: "network" });
    await expect(checkForUpdate("stable")).rejects.toMatchObject({
      code: "network",
      message: "network",
    });
  });

  it("maps an unknown code, an object without code and a string to the unknown code", async () => {
    tauri.invoke.mockRejectedValueOnce({ code: "boom", message: "x" });
    await expect(checkForUpdate("stable")).rejects.toMatchObject({ code: "unknown", message: "x" });
    tauri.invoke.mockRejectedValueOnce({ code: 7 });
    await expect(checkForUpdate("stable")).rejects.toMatchObject({ code: "unknown", message: "7" });
    tauri.invoke.mockRejectedValueOnce("the window is closing");
    await expect(checkForUpdate("stable")).rejects.toMatchObject({
      code: "unknown",
      message: "the window is closing",
    });
    tauri.invoke.mockRejectedValueOnce(new Error("ipc"));
    await expect(checkForUpdate("stable")).rejects.toMatchObject({
      code: "unknown",
      message: "update failed",
    });
    tauri.invoke.mockRejectedValueOnce(null);
    await expect(checkForUpdate("stable")).rejects.toMatchObject({ code: "unknown" });
  });
});

describe("downloadUpdate", () => {
  it("forwards progress events while the download runs, then stops listening", async () => {
    const unlisten = vi.fn();
    tauri.listen.mockResolvedValue(unlisten);
    let finish: () => void = () => undefined;
    tauri.invoke.mockReturnValue(new Promise<null>((resolve) => (finish = () => resolve(null))));
    const received: unknown[] = [];
    const done = downloadUpdate((progress) => received.push(progress));
    await vi.waitFor(() => expect(tauri.invoke).toHaveBeenCalledWith("update_download", undefined));
    expect(tauri.listen).toHaveBeenCalledWith("update://progress", expect.any(Function));
    const handler = tauri.listen.mock.calls[0]?.[1] as (e: { payload: unknown }) => void;
    handler({ payload: { downloaded: 10, total: 100 } });
    expect(received).toEqual([{ downloaded: 10, total: 100 }]);
    expect(unlisten).not.toHaveBeenCalled();
    finish();
    await done;
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("stops listening and maps the error when the download fails", async () => {
    const unlisten = vi.fn();
    tauri.listen.mockResolvedValue(unlisten);
    tauri.invoke.mockRejectedValue({ code: "signature", message: "bad" });
    await expect(downloadUpdate(() => undefined)).rejects.toMatchObject({ code: "signature" });
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("still downloads when the progress listener cannot be registered", async () => {
    tauri.listen.mockRejectedValue(new Error("no event system"));
    tauri.invoke.mockResolvedValue(null);
    await expect(downloadUpdate(() => undefined)).resolves.toBeUndefined();
  });
});

describe("installUpdate", () => {
  it("calls update_install and maps its failures", async () => {
    tauri.invoke.mockResolvedValueOnce(undefined);
    await expect(installUpdate()).resolves.toBeUndefined();
    expect(tauri.invoke).toHaveBeenCalledWith("update_install", undefined);
    tauri.invoke.mockRejectedValueOnce({ code: "not_downloaded", message: "gone" });
    await expect(installUpdate()).rejects.toMatchObject({ code: "not_downloaded" });
  });
});

describe("updatesApi", () => {
  it("bundles the four calls", () => {
    expect(updatesApi).toEqual({ appInfo, checkForUpdate, downloadUpdate, installUpdate });
  });
});
