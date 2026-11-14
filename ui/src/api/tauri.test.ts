import { beforeEach, describe, expect, it, vi } from "vitest";
import { TransportError } from "./jsonrpc";
import { tauriTransport } from "./tauri";

const tauri = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

beforeEach(() => {
  tauri.invoke.mockReset();
  tauri.listen.mockReset();
});

describe("tauriTransport", () => {
  it("sends each message through the window's api command", async () => {
    tauri.invoke.mockResolvedValue('{"jsonrpc":"2.0","id":1,"result":null}');
    await expect(tauriTransport.send("message")).resolves.toBe(
      '{"jsonrpc":"2.0","id":1,"result":null}',
    );
    // The names host_desktop::ipc serves.
    expect(tauri.invoke).toHaveBeenCalledWith("api", { message: "message" });
  });

  it("reports a failed IPC call as a TransportError", async () => {
    tauri.invoke.mockRejectedValue("the window is closing");
    await expect(tauriTransport.send("message")).rejects.toEqual(
      new TransportError("the window is closing"),
    );
    tauri.invoke.mockRejectedValue({ message: "api not allowed" });
    await expect(tauriTransport.send("message")).rejects.toEqual(
      new TransportError('{"message":"api not allowed"}'),
    );
  });

  it("fetches a scene delta as raw bytes through the scene command", async () => {
    const buffer = new Uint8Array([66, 67]).buffer;
    tauri.invoke.mockResolvedValue(buffer);
    await expect(tauriTransport.scene?.(7)).resolves.toBe(buffer);
    expect(tauri.invoke).toHaveBeenCalledWith("scene", { since: 7 });
  });

  it("reports a failed or malformed scene call as a TransportError", async () => {
    tauri.invoke.mockRejectedValue("document closed");
    await expect(tauriTransport.scene?.(null)).rejects.toEqual(
      new TransportError("document closed"),
    );
    tauri.invoke.mockResolvedValue("not bytes");
    await expect(tauriTransport.scene?.(null)).rejects.toEqual(
      new TransportError("malformed scene response"),
    );
  });

  it("delivers notification events and stops on unsubscribe", async () => {
    const unlisten = vi.fn();
    tauri.listen.mockResolvedValue(unlisten);
    const received: string[] = [];
    const unsubscribe = await tauriTransport.listen((message) => received.push(message));
    expect(tauri.listen).toHaveBeenCalledWith("api://notification", expect.any(Function));
    const handler = tauri.listen.mock.calls[0]?.[1] as (event: { payload: string }) => void;
    handler({ payload: "notification" });
    expect(received).toEqual(["notification"]);
    unsubscribe();
    expect(unlisten).toHaveBeenCalled();
  });
});
