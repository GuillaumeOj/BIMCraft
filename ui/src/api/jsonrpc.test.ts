import { describe, expect, it } from "vitest";
import { ApiError, readChange, readResponse, request, TransportError } from "./jsonrpc";

describe("request", () => {
  it("writes a JSON-RPC 2.0 request", () => {
    expect(JSON.parse(request(3, "history.undo", {}))).toEqual({
      jsonrpc: "2.0",
      id: 3,
      method: "history.undo",
      params: {},
    });
  });
});

describe("readResponse", () => {
  it("returns the result of the matching response", () => {
    expect(readResponse('{"jsonrpc":"2.0","id":4,"result":{"can_undo":true}}', 4)).toEqual({
      can_undo: true,
    });
    expect(readResponse('{"jsonrpc":"2.0","id":5,"result":null}', 5)).toBeNull();
  });

  it("throws the server's error as an ApiError", () => {
    const text = '{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"nothing to undo"}}';
    expect(() => readResponse(text, 1)).toThrow(ApiError);
    expect(() => readResponse(text, 1)).toThrow(
      expect.objectContaining({
        name: "ApiError",
        code: -32001,
        message: "nothing to undo",
        data: undefined,
      }),
    );
    expect(() =>
      readResponse(
        '{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"invalid","data":{"detail":"x"}}}',
        1,
      ),
    ).toThrow(expect.objectContaining({ code: -32602, data: { detail: "x" } }));
  });

  it.each([
    ["no response", null, "no response to request 1"],
    ["not JSON", "{", "malformed response to request 1"],
    ["not an object", "[]", "malformed response to request 1"],
    ["another protocol", '{"jsonrpc":"1.0","id":1,"result":1}', "malformed response to request 1"],
    ["another request", '{"jsonrpc":"2.0","id":2,"result":1}', "malformed response to request 1"],
    ["neither result nor error", '{"jsonrpc":"2.0","id":1}', "malformed response to request 1"],
    [
      "a malformed error",
      '{"jsonrpc":"2.0","id":1,"error":{"code":"x"}}',
      "malformed response to request 1",
    ],
    ["a null error", '{"jsonrpc":"2.0","id":1,"error":null}', "malformed response to request 1"],
  ])("rejects %s as a TransportError", (_case, text, message) => {
    expect(() => readResponse(text, 1)).toThrow(new TransportError(message));
  });
});

describe("readChange", () => {
  const event = { created: ["a"], modified: [], deleted: [] };

  it("reads a model.changed notification", () => {
    expect(
      readChange(JSON.stringify({ jsonrpc: "2.0", method: "model.changed", params: event })),
    ).toEqual(event);
  });

  it.each([
    ["not JSON", "{"],
    ["not an object", "1"],
    ["another method", JSON.stringify({ jsonrpc: "2.0", method: "other", params: event })],
    ["no params", JSON.stringify({ jsonrpc: "2.0", method: "model.changed" })],
    [
      "params that are no change",
      JSON.stringify({ jsonrpc: "2.0", method: "model.changed", params: { created: "a" } }),
    ],
  ])("ignores %s", (_case, text) => {
    expect(readChange(text)).toBeNull();
  });
});
