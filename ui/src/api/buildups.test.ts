import { describe, expect, it } from "vitest";
import {
  buildUpRefusal,
  createBuildUp,
  deleteBuildUp,
  duplicateBuildUp,
  getBuildUp,
  listBuildUps,
  previewBuildUp,
  updateBuildUp,
} from "./buildups";
import { ApiClient, type Transport } from "./client";
import { ApiError } from "./jsonrpc";

/** A client that records the requests and answers each with `result`. */
function recording(result: unknown = {}) {
  const sent: { method: string; params: Record<string, unknown> }[] = [];
  const transport: Transport = {
    async send(message) {
      const request = JSON.parse(message);
      sent.push({ method: request.method, params: request.params });
      return JSON.stringify({ jsonrpc: "2.0", id: request.id, result });
    },
    async listen() {
      return () => {};
    },
  };
  return { client: new ApiClient(transport), sent };
}

const layer = { material: "m", thickness: 0.1, function: "structure" } as const;

describe("buildUpRefusal", () => {
  it("reads the code and the layer index", () => {
    const error = new ApiError(-32000, "x", { reason: "invalid_thickness", index: 2 });
    expect(buildUpRefusal(error)).toEqual({ code: "invalid_thickness", index: 2 });
  });

  it("has no index for a refusal that is not about a layer", () => {
    expect(buildUpRefusal(new ApiError(-32000, "x", { reason: "no_layers" }))).toEqual({
      code: "no_layers",
      index: null,
    });
    expect(buildUpRefusal(new ApiError(-32000, "x", { reason: "r", index: "1" }))).toEqual({
      code: "r",
      index: null,
    });
  });

  it.each([
    ["a plain error", new Error("x")],
    ["data without a reason", new ApiError(-32000, "x", { index: 1 })],
    ["a non-error", null],
  ])("is null for %s", (_name, error) => {
    expect(buildUpRefusal(error)).toBeNull();
  });
});

describe("queries", () => {
  it("lists with or without a usage and returns the items", async () => {
    const { client, sent } = recording({ items: [{ id: "a" }] });
    expect(await listBuildUps(client)).toEqual([{ id: "a" }]);
    await listBuildUps(client, "slab");
    expect(sent.map((request) => request.params)).toEqual([{}, { usage: "slab" }]);
  });

  it("reads one and previews a draft", async () => {
    const { client, sent } = recording({});
    await getBuildUp(client, "b");
    await previewBuildUp(client, "wall", [layer]);
    expect(sent).toEqual([
      { method: "buildups.get", params: { buildup: "b" } },
      { method: "buildups.preview", params: { usage: "wall", layers: [layer] } },
    ]);
  });
});

describe("commands", () => {
  it("creates and duplicates under fresh ids", async () => {
    const { client, sent } = recording({});
    const created = await createBuildUp(client, { name: "W", usage: "wall", layers: [layer] });
    const copy = await duplicateBuildUp(client, "b", "W copy");
    expect(created).toMatch(/^[0-9a-f]{32}$/);
    expect(copy).not.toBe(created);
    expect(sent[0]?.params).toEqual({ id: created, name: "W", usage: "wall", layers: [layer] });
    expect(sent[1]?.params).toEqual({ buildup: "b", id: copy, name: "W copy" });
  });

  it("updates and deletes", async () => {
    const { client, sent } = recording({});
    await updateBuildUp(client, "b", { name: "N", description: null, layers: [layer] });
    await deleteBuildUp(client, "b");
    expect(sent).toEqual([
      {
        method: "buildups.update",
        params: { buildup: "b", name: "N", description: null, layers: [layer] },
      },
      { method: "buildups.delete", params: { buildup: "b" } },
    ]);
  });
});
