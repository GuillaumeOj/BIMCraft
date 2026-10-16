import { describe, expect, it, vi } from "vitest";
import { ApiClient, type Transport } from "./client";
import { ApiError } from "./jsonrpc";

/** A transport answering each request with `answer(method, params)`, and pushing notifications. */
function fakeTransport(answer: (method: string, params: unknown) => unknown) {
  const listeners = new Set<(message: string) => void>();
  const sent: string[] = [];
  const transport: Transport = {
    async send(message) {
      sent.push(message);
      const { id, method, params } = JSON.parse(message);
      return JSON.stringify({ jsonrpc: "2.0", id, result: answer(method, params) });
    },
    async listen(onMessage) {
      listeners.add(onMessage);
      return () => listeners.delete(onMessage);
    },
  };
  const push = (message: string) => {
    for (const listener of listeners) listener(message);
  };
  return { transport, sent, push };
}

describe("ApiClient", () => {
  it("sends commands and queries by name with their parameters, each with its own id", async () => {
    const { transport, sent } = fakeTransport((method) =>
      method === "history.state" ? { can_undo: false, can_redo: false } : null,
    );
    const client = new ApiClient(transport);
    await expect(client.command("model.delete", { id: "0".repeat(32) })).resolves.toBeNull();
    await expect(client.query("history.state", {})).resolves.toMatchObject({ can_undo: false });
    expect(sent.map((message) => JSON.parse(message))).toEqual([
      { jsonrpc: "2.0", id: 1, method: "model.delete", params: { id: "0".repeat(32) } },
      { jsonrpc: "2.0", id: 2, method: "history.state", params: {} },
    ]);
  });

  it("rejects with the server's error", async () => {
    const transport: Transport = {
      send: async (message) =>
        JSON.stringify({
          jsonrpc: "2.0",
          id: JSON.parse(message).id,
          error: { code: -32001, message: "nothing to undo" },
        }),
      listen: async () => () => {},
    };
    await expect(new ApiClient(transport).command("history.undo", {})).rejects.toBeInstanceOf(
      ApiError,
    );
  });

  it("passes model changes to listeners until they unsubscribe, ignoring other messages", async () => {
    const { transport, push } = fakeTransport(() => null);
    const client = new ApiClient(transport);
    const listener = vi.fn();
    const unsubscribe = await client.onChange(listener);
    const event = { created: [], modified: ["b"], deleted: [] };
    push(JSON.stringify({ jsonrpc: "2.0", method: "model.changed", params: event }));
    push("not a notification");
    unsubscribe();
    push(JSON.stringify({ jsonrpc: "2.0", method: "model.changed", params: event }));
    expect(listener.mock.calls).toEqual([[event]]);
  });
});
