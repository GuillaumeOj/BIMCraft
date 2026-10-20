import { readChange, readResponse, request } from "./jsonrpc";
import type { ChangeEvent, Commands, Queries } from "./protocol.gen";

/** Carries JSON-RPC messages to the core and back (API-04): Tauri's IPC in the desktop window. */
export interface Transport {
  /** Sends one message and resolves to the response, or null when there is none. */
  send(message: string): Promise<string | null>;
  /** Calls `onMessage` with each notification from the core; resolves to the unsubscribe call. */
  listen(onMessage: (message: string) => void): Promise<() => void>;
}

/**
 * The typed client of the command and query API: the only way UI code reaches the core (UI-01).
 * Names, parameters and results come from the published schema (protocol.gen.ts).
 */
export class ApiClient {
  readonly #transport: Transport;
  #lastId = 0;

  constructor(transport: Transport) {
    this.#transport = transport;
  }

  /** Runs command `name`; resolves to its result, rejects with an `ApiError` or `TransportError`. */
  command<N extends keyof Commands>(
    name: N,
    params: Commands[N]["params"],
  ): Promise<Commands[N]["result"]> {
    // The core answers with the result its schema declares, from which Commands is generated.
    return this.#call(name, params) as Promise<Commands[N]["result"]>;
  }

  /** Runs query `name`; resolves to its result, rejects with an `ApiError` or `TransportError`. */
  query<N extends keyof Queries>(
    name: N,
    params: Queries[N]["params"],
  ): Promise<Queries[N]["result"]> {
    return this.#call(name, params) as Promise<Queries[N]["result"]>;
  }

  /** Calls `listener` with every change to the model; resolves to the unsubscribe call. */
  onChange(listener: (event: ChangeEvent) => void): Promise<() => void> {
    return this.#transport.listen((message) => {
      const event = readChange(message);
      if (event !== null) listener(event);
    });
  }

  async #call(method: string, params: unknown): Promise<unknown> {
    this.#lastId += 1;
    const id = this.#lastId;
    return readResponse(await this.#transport.send(request(id, method, params)), id);
  }
}
