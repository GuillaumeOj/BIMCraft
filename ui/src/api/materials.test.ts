import { describe, expect, it } from "vitest";
import { fakeCore } from "../test-support/fakeCore";
import { ApiClient, type Transport } from "./client";
import { ApiError } from "./jsonrpc";
import {
  addFromLibrary,
  createMaterial,
  deleteMaterial,
  getGrade,
  getMaterial,
  historyDepth,
  listMaterials,
  materialRefusal,
  renameMaterial,
  searchLibrary,
  setPropertyValue,
  undoLastStep,
} from "./materials";

const C25 = "en1992-1-1/C25/30";

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

describe("materialRefusal", () => {
  it("reads the reason, the property and the count of a refusal", () => {
    const error = new ApiError(-32000, "x", { reason: "in_use", key: "fck", used_by: 3 });
    expect(materialRefusal(error)).toEqual({ reason: "in_use", key: "fck", usedBy: 3 });
  });

  it("reads `count`, which model.delete sends instead of used_by", () => {
    const error = new ApiError(-32000, "x", { reason: "in_use", count: 2 });
    expect(materialRefusal(error)).toEqual({ reason: "in_use", key: null, usedBy: 2 });
  });

  it.each([
    ["a plain error", new Error("x")],
    ["an error without data", new ApiError(-32000, "x")],
    ["data that is not an object", new ApiError(-32000, "x", "text")],
    ["data without a reason", new ApiError(-32000, "x", { key: "a" })],
    ["a reason that is not a string", new ApiError(-32000, "x", { reason: 3 })],
    ["a non-error", null],
  ])("is null for %s", (_name, error) => {
    expect(materialRefusal(error)).toBeNull();
  });

  it("ignores a key and a count of the wrong type", () => {
    const error = new ApiError(-32000, "x", { reason: "r", key: 1, used_by: "2" });
    expect(materialRefusal(error)).toEqual({ reason: "r", key: null, usedBy: null });
  });
});

describe("queries", () => {
  it("lists with the trimmed search and the category, and nothing when there is none", async () => {
    const { client, sent } = recording({ items: [] });
    await listMaterials(client);
    await listMaterials(client, { search: "  c25 ", category: "concrete" });
    await listMaterials(client, { search: "   ", category: null });
    expect(sent.map((request) => request.params)).toEqual([
      {},
      { search: "c25", category: "concrete" },
      {},
    ]);
  });

  it("returns the items of the list", async () => {
    const { client } = recording({ items: [{ id: "a" }] });
    expect(await listMaterials(client)).toEqual([{ id: "a" }]);
  });

  it("searches the library with its paging, leaving out what is not given", async () => {
    const { client, sent } = recording({ items: [], total: 0 });
    await searchLibrary(client);
    await searchLibrary(client, { search: " c3", category: "concrete", offset: 0, limit: 50 });
    expect(sent.map((request) => request.params)).toEqual([
      {},
      { search: "c3", category: "concrete", offset: 0, limit: 50 },
    ]);
  });

  it("reads a material and a grade", async () => {
    const { client, sent } = recording({});
    await getMaterial(client, "m1");
    await getGrade(client, C25);
    expect(sent).toEqual([
      { method: "materials.get", params: { material: "m1" } },
      { method: "materials.grade", params: { key: C25 } },
    ]);
  });
});

describe("commands", () => {
  it("adds a grade under a fresh id, with the name only when it is given", async () => {
    const { client, sent } = recording();
    const first = await addFromLibrary(client, C25);
    const second = await addFromLibrary(client, C25, "Slab concrete");
    expect(first).toMatch(/^[0-9a-f]{32}$/);
    expect(second).not.toBe(first);
    expect(sent[0]?.params).toEqual({ id: first, key: C25 });
    expect(sent[1]?.params).toEqual({ id: second, key: C25, name: "Slab concrete" });
  });

  it("creates a custom material, with its values only when given", async () => {
    const { client, sent } = recording();
    const id = await createMaterial(client, "Mix", "concrete");
    await createMaterial(client, "Mix 2", "steel", [{ key: "fy", value: 355e6 }]);
    expect(sent[0]?.params).toEqual({ id, name: "Mix", category: "concrete" });
    expect(sent[1]?.params).toMatchObject({ properties: [{ key: "fy", value: 355e6 }] });
  });

  it("sets a value, naming the annex of a partial factor only", async () => {
    const { client, sent } = recording();
    await setPropertyValue(client, "m1", { key: "fck" }, 30e6);
    await setPropertyValue(client, "m1", { key: "gamma_c", annex: "fr" }, null);
    expect(sent.map((request) => request.params)).toEqual([
      { material: "m1", properties: [{ key: "fck", value: 30e6 }] },
      { material: "m1", properties: [{ key: "gamma_c", annex: "fr", value: null }] },
    ]);
  });

  it("renames, deletes and undoes", async () => {
    const { client, sent } = recording();
    await renameMaterial(client, "m1", "New");
    await deleteMaterial(client, "m1");
    await undoLastStep(client);
    expect(sent).toEqual([
      { method: "materials.update", params: { material: "m1", name: "New" } },
      { method: "materials.delete", params: { material: "m1" } },
      { method: "history.undo", params: {} },
    ]);
  });

  it("lets a refusal through", async () => {
    const core = fakeCore();
    await expect(deleteMaterial(core.client, "missing")).rejects.toBeInstanceOf(ApiError);
  });
});

describe("historyDepth", () => {
  it("counts the applied steps over every page", async () => {
    const pages = [
      { entries: [{ undone: false }, { undone: true }], next: 2 },
      { entries: [{ undone: false }], next: null },
    ];
    const requests: unknown[] = [];
    const transport: Transport = {
      async send(message) {
        const request = JSON.parse(message);
        requests.push(request.params);
        return JSON.stringify({ jsonrpc: "2.0", id: request.id, result: pages.shift() });
      },
      async listen() {
        return () => {};
      },
    };
    expect(await historyDepth(new ApiClient(transport))).toBe(2);
    expect(requests).toEqual([
      { start: 0, limit: 1000 },
      { start: 2, limit: 1000 },
    ]);
  });
});
