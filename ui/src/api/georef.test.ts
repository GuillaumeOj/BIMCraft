import { describe, expect, it } from "vitest";
import { ApiClient, type Transport } from "./client";
import { apiReason, newEntityId, setGeoreference } from "./georef";
import { ApiError } from "./jsonrpc";

const VALUE = {
  crs_epsg: 2154,
  base_point: [1, 2, 3] as [number, number, number],
  grid_rotation: 0,
  scale: 1,
  survey_point: null,
};

/** A client whose `georef.set` answers come from `replies`, one per call. */
function clientReplying(replies: unknown[]) {
  const sent: { params: unknown }[] = [];
  const transport: Transport = {
    async send(message) {
      const request = JSON.parse(message);
      sent.push(request);
      const reply = replies.shift();
      const body =
        reply instanceof ApiError
          ? { error: { code: reply.code, message: reply.message, data: reply.data } }
          : { result: reply };
      return JSON.stringify({ jsonrpc: "2.0", id: request.id, ...body });
    },
    async listen() {
      return () => {};
    },
  };
  return { client: new ApiClient(transport), sent };
}

const refusal = (reason: string) => new ApiError(-32000, "refused", { reason });

describe("apiReason", () => {
  it("reads data.reason of an API error", () => {
    expect(apiReason(refusal("outside_domain"))).toBe("outside_domain");
  });

  it.each([
    ["a plain error", new Error("x")],
    ["an error without data", new ApiError(-32000, "x")],
    ["an error whose data is not an object", new ApiError(-32000, "x", "text")],
    ["an error whose data has no reason", new ApiError(-32000, "x", { index: 1 })],
    ["an error whose reason is not a string", new ApiError(-32000, "x", { reason: 3 })],
    ["a non-error", null],
  ])("is null for %s", (_name, error) => {
    expect(apiReason(error)).toBeNull();
  });
});

describe("newEntityId", () => {
  it("draws 32 lower-case hexadecimal digits, different each time", () => {
    const [a, b] = [newEntityId(), newEntityId()];
    expect(a).toMatch(/^[0-9a-f]{32}$/);
    expect(b).not.toBe(a);
  });
});

describe("setGeoreference", () => {
  it("sends the value without a site id", async () => {
    const { client, sent } = clientReplying([{ site: "a".repeat(32) }]);
    await expect(setGeoreference(client, VALUE)).resolves.toEqual({ site: "a".repeat(32) });
    expect(sent.map((request) => request.params)).toEqual([VALUE]);
  });

  it("retries once with a fresh site id when the project has no site", async () => {
    const { client, sent } = clientReplying([refusal("missing_site_id"), { site: "b".repeat(32) }]);
    await expect(setGeoreference(client, VALUE)).resolves.toEqual({ site: "b".repeat(32) });
    expect(sent).toHaveLength(2);
    expect(sent[1]?.params).toEqual({ ...VALUE, site: expect.stringMatching(/^[0-9a-f]{32}$/) });
  });

  it("does not retry any other refusal", async () => {
    const { client, sent } = clientReplying([refusal("outside_domain")]);
    await expect(setGeoreference(client, VALUE)).rejects.toMatchObject({ code: -32000 });
    expect(sent).toHaveLength(1);
  });

  it("gives up when the retry is refused too", async () => {
    const { client } = clientReplying([refusal("missing_site_id"), refusal("multiple_sites")]);
    await expect(setGeoreference(client, VALUE)).rejects.toSatisfy(
      (error) => apiReason(error) === "multiple_sites",
    );
  });
});
