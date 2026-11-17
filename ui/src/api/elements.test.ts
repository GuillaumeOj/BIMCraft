import { describe, expect, it } from "vitest";
import { ApiClient, type Transport } from "./client";
import { moveElements, moveRefusal, placeBlock } from "./elements";
import { ApiError } from "./jsonrpc";

/** A transport that records the requests and answers null. */
function recorder(result: unknown = null) {
  const sent: { method: string; params: unknown }[] = [];
  const transport: Transport = {
    async send(message) {
      const { id, method, params } = JSON.parse(message);
      sent.push({ method, params });
      return JSON.stringify({ jsonrpc: "2.0", id, result });
    },
    listen: async () => () => undefined,
  };
  return { client: new ApiClient(transport), sent };
}

describe("moveElements", () => {
  it("sends one elements.move with the ids and the translation", async () => {
    const { client, sent } = recorder();
    const ids = ["1".padStart(32, "0"), "2".padStart(32, "0")] as const;
    await moveElements(client, ids, [0, 3, 0]);
    expect(sent).toEqual([
      { method: "elements.move", params: { ids: [...ids], translation: [0, 3, 0] } },
    ]);
  });
});

describe("moveRefusal", () => {
  it("reads the reason of a refusal", () => {
    expect(moveRefusal(new ApiError(-32000, "refused", { reason: "not_movable" }))).toBe(
      "not_movable",
    );
    expect(moveRefusal(new ApiError(-32000, "refused", { reason: "too_many" }))).toBe("too_many");
  });

  it("is null for an unknown reason or another failure", () => {
    expect(moveRefusal(new ApiError(-32000, "refused", { reason: "weather" }))).toBeNull();
    expect(moveRefusal(new ApiError(-32602, "invalid params"))).toBeNull();
    expect(moveRefusal(new Error("boom"))).toBeNull();
  });
});

describe("placeBlock", () => {
  it("creates a Block with the schema's attribute encoding and resolves to its new id", async () => {
    const { client, sent } = recorder();
    const id = await placeBlock(client, { location: [1, 2, 0], rotation: 0.5, size: [6, 4, 3] });
    expect(id).toMatch(/^[0-9a-f]{32}$/);
    expect(sent).toEqual([
      {
        method: "model.create",
        params: {
          entity: "Block",
          id,
          values: [
            ["location", { List: [{ Real: 1 }, { Real: 2 }, { Real: 0 }] }],
            ["rotation", { Real: 0.5 }],
            ["size_x", { Real: 6 }],
            ["size_y", { Real: 4 }],
            ["size_z", { Real: 3 }],
          ],
        },
      },
    ]);
  });
});
