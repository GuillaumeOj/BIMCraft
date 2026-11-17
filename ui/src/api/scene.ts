import type { ApiClient } from "./client";

/** Where the scene feed hands its deltas: the 3D viewport. */
export interface SceneSink {
  /** Applies one delta, untouched; returns the revision now held. Throws when it rejects the bytes. */
  applyScene(bytes: Uint8Array): number;
  /** The revision held, or null before the first delta. */
  revision(): number | null;
}

/** Why the feed could not keep the view up to date. */
export type SceneFailure =
  /** The scene could not be fetched (the transport failed or the document is closed). */
  | { kind: "fetch"; error: unknown }
  /** The viewport refused the bytes it was given. */
  | { kind: "rejected"; error: unknown };

/** A running scene feed (see {@link startSceneFeed}). */
export interface SceneFeed {
  /**
   * Fetches what changed since the last delta; resolves once a delta fetched after this call has
   * been applied (or failed). Use it to act on an element the model just gained.
   */
  refresh(): Promise<void>;
  /** Stops listening to the model; a delta still in flight is dropped. */
  stop(): void;
}

/**
 * Keeps `sink` up to date with the model (api-contract.md §1): the full scene at once, then a
 * delta after each `model.changed` notification. One `scene` call is in flight at a time; any
 * number of notifications during a call schedule exactly one more call after it. JavaScript never
 * reads the bytes: they go to the viewport as they came.
 */
export function startSceneFeed(
  api: ApiClient,
  sink: SceneSink,
  onFailure: (failure: SceneFailure) => void,
): SceneFeed {
  let stopped = false;
  let running = false;
  // Callers of refresh() waiting for the next fetch to start, then for it to finish.
  let queued: (() => void)[] = [];
  let pending = false;

  const fetchOnce = async () => {
    let bytes: ArrayBuffer;
    try {
      bytes = await api.scene(sink.revision());
    } catch (error) {
      if (!stopped) onFailure({ kind: "fetch", error });
      return;
    }
    if (stopped) return;
    try {
      sink.applyScene(new Uint8Array(bytes));
    } catch (error) {
      onFailure({ kind: "rejected", error });
    }
  };

  const run = async () => {
    running = true;
    while (pending && !stopped) {
      pending = false;
      const waiting = queued;
      queued = [];
      await fetchOnce();
      for (const resolve of waiting) resolve();
    }
    running = false;
    // Stopped mid-way: nobody will fetch for the callers still waiting.
    for (const resolve of queued) resolve();
    queued = [];
  };

  const request = () => {
    pending = true;
    if (!running) void run();
  };

  const unsubscribe = api.onChange(request).catch(() => undefined);
  request();

  return {
    refresh() {
      if (stopped) return Promise.resolve();
      return new Promise<void>((resolve) => {
        queued.push(resolve);
        request();
      });
    },
    stop() {
      stopped = true;
      void unsubscribe.then((stop) => stop?.());
    },
  };
}
