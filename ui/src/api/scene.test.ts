import { describe, expect, it, vi } from "vitest";
import { ApiClient, type Transport } from "./client";
import { TransportError } from "./jsonrpc";
import { type SceneFailure, type SceneSink, startSceneFeed } from "./scene";

const CHANGED = JSON.stringify({
  jsonrpc: "2.0",
  method: "model.changed",
  params: { created: [], deleted: [], modified: [] },
});

/** A deferred `scene` answer the test settles by hand. */
type Call = {
  since: number | null;
  resolve: (bytes: ArrayBuffer) => void;
  reject: (e: unknown) => void;
};

/** A transport whose `scene` calls wait for the test, and which pushes notifications on demand. */
function sceneTransport() {
  const calls: Call[] = [];
  const listeners = new Set<(message: string) => void>();
  const unlisten = vi.fn();
  const transport: Transport = {
    send: async () => null,
    async listen(onMessage) {
      listeners.add(onMessage);
      return () => {
        unlisten();
        listeners.delete(onMessage);
      };
    },
    scene: (since) =>
      new Promise<ArrayBuffer>((resolve, reject) => {
        calls.push({ since, resolve, reject });
      }),
  };
  const changed = () => {
    for (const listener of listeners) listener(CHANGED);
  };
  return { client: new ApiClient(transport), calls, changed, unlisten };
}

/** A viewport stand-in: each delta is one byte holding the revision it brings. */
function fakeSink() {
  let revision: number | null = null;
  const applied: number[][] = [];
  const sink: SceneSink = {
    applyScene(bytes) {
      applied.push([...bytes]);
      if (bytes[0] === 255) throw new Error("bad scene");
      revision = bytes[0] ?? 0;
      return revision;
    },
    revision: () => revision,
  };
  return { sink, applied };
}

const bytes = (...values: number[]) => new Uint8Array(values).buffer;
// Lets the feed's promise chains run.
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("startSceneFeed", () => {
  it("fetches the full scene first, then a delta since the held revision after each change", async () => {
    const { client, calls, changed } = sceneTransport();
    const { sink, applied } = fakeSink();
    startSceneFeed(client, sink, vi.fn());
    expect(calls.map((call) => call.since)).toEqual([null]);
    calls[0]?.resolve(bytes(3));
    await settle();
    expect(applied).toEqual([[3]]);

    changed();
    await settle();
    expect(calls.map((call) => call.since)).toEqual([null, 3]);
    calls[1]?.resolve(bytes(4));
    await settle();
    expect(applied).toEqual([[3], [4]]);
  });

  it("keeps one call in flight and coalesces the changes during it into one more call", async () => {
    const { client, calls, changed } = sceneTransport();
    const { sink, applied } = fakeSink();
    startSceneFeed(client, sink, vi.fn());
    await settle();
    changed();
    changed();
    changed();
    await settle();
    expect(calls).toHaveLength(1);
    calls[0]?.resolve(bytes(3));
    await settle();
    expect(calls.map((call) => call.since)).toEqual([null, 3]);
    calls[1]?.resolve(bytes(4));
    await settle();
    expect(calls).toHaveLength(2);
    expect(applied).toEqual([[3], [4]]);
  });

  it("reports a failed fetch, and fetches again on the next change", async () => {
    const { client, calls, changed } = sceneTransport();
    const { sink } = fakeSink();
    const failures: SceneFailure[] = [];
    startSceneFeed(client, sink, (failure) => failures.push(failure));
    const error = new TransportError("closed");
    calls[0]?.reject(error);
    await settle();
    expect(failures).toEqual([{ kind: "fetch", error }]);
    changed();
    await settle();
    expect(calls.map((call) => call.since)).toEqual([null, null]);
  });

  it("reports bytes the viewport rejects", async () => {
    const { client, calls } = sceneTransport();
    const { sink } = fakeSink();
    const failures: SceneFailure[] = [];
    startSceneFeed(client, sink, (failure) => failures.push(failure));
    calls[0]?.resolve(bytes(255));
    await settle();
    expect(failures).toEqual([{ kind: "rejected", error: new Error("bad scene") }]);
  });

  it("resolves refresh once a delta fetched after the call is applied", async () => {
    const { client, calls } = sceneTransport();
    const { sink, applied } = fakeSink();
    const feed = startSceneFeed(client, sink, vi.fn());
    const refreshed = vi.fn();
    // The full scene is already in flight: refresh waits for the call after it.
    void feed.refresh().then(refreshed);
    calls[0]?.resolve(bytes(3));
    await settle();
    expect(refreshed).not.toHaveBeenCalled();
    calls[1]?.resolve(bytes(4));
    await settle();
    expect(applied).toEqual([[3], [4]]);
    expect(refreshed).toHaveBeenCalled();
  });

  it("resolves refresh after a failed fetch too", async () => {
    const { client, calls } = sceneTransport();
    const { sink } = fakeSink();
    const feed = startSceneFeed(client, sink, vi.fn());
    calls[0]?.resolve(bytes(3));
    await settle();
    const refreshed = feed.refresh();
    calls[1]?.reject(new TransportError("closed"));
    await expect(refreshed).resolves.toBeUndefined();
  });

  it("stops listening, drops the delta in flight and releases the waiting callers", async () => {
    const { client, calls, changed, unlisten } = sceneTransport();
    const { sink, applied } = fakeSink();
    const onFailure = vi.fn();
    const feed = startSceneFeed(client, sink, onFailure);
    const refreshed = feed.refresh();
    feed.stop();
    await settle();
    expect(unlisten).toHaveBeenCalled();
    calls[0]?.resolve(bytes(3));
    await expect(refreshed).resolves.toBeUndefined();
    expect(applied).toEqual([]);
    changed();
    await settle();
    expect(calls).toHaveLength(1);
    await expect(feed.refresh()).resolves.toBeUndefined();
  });

  it("stays quiet about a fetch that fails after it stopped", async () => {
    const { client, calls } = sceneTransport();
    const onFailure = vi.fn();
    const feed = startSceneFeed(client, fakeSink().sink, onFailure);
    feed.stop();
    calls[0]?.reject(new TransportError("closed"));
    await settle();
    expect(onFailure).not.toHaveBeenCalled();
  });

  it("still fetches the scene when the notifications cannot be subscribed to", async () => {
    const { client, calls } = sceneTransport();
    vi.spyOn(client, "onChange").mockRejectedValue(new TransportError("no events"));
    const feed = startSceneFeed(client, fakeSink().sink, vi.fn());
    expect(calls).toHaveLength(1);
    feed.stop();
    await settle();
  });
});

describe("ApiClient.scene", () => {
  it("rejects when the transport has no scene channel", async () => {
    const client = new ApiClient({ send: async () => null, listen: async () => () => undefined });
    await expect(client.scene(null)).rejects.toEqual(
      new TransportError("this transport has no scene channel"),
    );
  });
});
