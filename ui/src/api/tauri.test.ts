import { beforeEach, describe, expect, it, vi } from "vitest";
import { TransportError } from "./jsonrpc";
import { COMMAND, NOTIFICATION, tauriTransport } from "./tauri";

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
    expect(tauri.invoke).toHaveBeenCalledWith(COMMAND, { message: "message" });
    expect(COMMAND).toBe("api");
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

  it("delivers notification events and stops on unsubscribe", async () => {
    const unlisten = vi.fn();
    tauri.listen.mockResolvedValue(unlisten);
    const received: string[] = [];
    const unsubscribe = await tauriTransport.listen((message) => received.push(message));
    expect(tauri.listen).toHaveBeenCalledWith(NOTIFICATION, expect.any(Function));
    expect(NOTIFICATION).toBe("api://notification");
    const handler = tauri.listen.mock.calls[0]?.[1] as (event: { payload: string }) => void;
    handler({ payload: "notification" });
    expect(received).toEqual(["notification"]);
    unsubscribe();
    expect(unlisten).toHaveBeenCalled();
  });
});
