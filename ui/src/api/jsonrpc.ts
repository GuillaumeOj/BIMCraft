import type { ChangeEvent } from "./protocol.gen";

/** An error the core returned for a request: its JSON-RPC code (docs/dev/api.md) and message. */
export class ApiError extends Error {
  override readonly name = "ApiError";
  readonly code: number;
  readonly data: unknown;

  constructor(code: number, message: string, data?: unknown) {
    super(message);
    this.code = code;
    this.data = data;
  }
}

/** A request that did not reach the core, or whose response could not be read. */
export class TransportError extends Error {
  override readonly name = "TransportError";
}

// The fields a parsed message may hold, each unknown until checked.
type Message = {
  jsonrpc?: unknown;
  id?: unknown;
  result?: unknown;
  error?: unknown;
  method?: unknown;
  params?: unknown;
};
type ErrorObject = { code?: unknown; message?: unknown; data?: unknown };
type ChangeObject = { created?: unknown; modified?: unknown; deleted?: unknown };

/** Whether `value` is a JSON object; its fields are then read as `unknown`. */
const isObject = <T extends object>(value: unknown): value is T =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function parse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

/** The JSON-RPC 2.0 request text calling `method` with `params`. */
export const request = (id: number, method: string, params: unknown): string =>
  JSON.stringify({ jsonrpc: "2.0", id, method, params });

/**
 * The result of the response `text` to request `id`.
 *
 * @throws {ApiError} when the core answered with an error.
 * @throws {TransportError} when there is no response, or one that is not a JSON-RPC 2.0
 * response to this request.
 */
export function readResponse(text: string | null, id: number): unknown {
  if (text === null) throw new TransportError(`no response to request ${id}`);
  const response = parse(text);
  const malformed = new TransportError(`malformed response to request ${id}`);
  if (!isObject<Message>(response) || response.jsonrpc !== "2.0" || response.id !== id) {
    throw malformed;
  }
  if ("result" in response) return response.result;
  const { error } = response;
  if (
    isObject<ErrorObject>(error) &&
    typeof error.code === "number" &&
    typeof error.message === "string"
  ) {
    throw new ApiError(error.code, error.message, error.data);
  }
  throw malformed;
}

const isIdList = (value: unknown): boolean =>
  Array.isArray(value) && value.every((id) => typeof id === "string");

/** The change a `model.changed` notification `text` reports, or null for any other message. */
export function readChange(text: string): ChangeEvent | null {
  const message = parse(text);
  if (!isObject<Message>(message) || message.method !== "model.changed") return null;
  const { params } = message;
  if (!isObject<ChangeObject>(params)) return null;
  if (!isIdList(params.created) || !isIdList(params.modified) || !isIdList(params.deleted)) {
    return null;
  }
  return params as ChangeEvent;
}
