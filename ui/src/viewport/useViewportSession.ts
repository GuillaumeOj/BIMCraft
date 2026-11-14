import { useCallback, useEffect, useRef, useState } from "react";
import { type SceneFeed, startSceneFeed, useApi } from "../api";
import { loadViewportModule } from "./module";
import { readTheme, watchColourScheme } from "./theme";
import type { ViewportErrorKey, ViewportEvent, ViewportHandle, ViewportModule } from "./types";

/** Why the view stopped: an error of the viewport, a module that did not load, or Cancel. */
export type FailureReason = ViewportErrorKey | "load_failed" | "cancelled";

export type Phase =
  | { kind: "loading"; slow: boolean }
  | { kind: "unavailable" }
  | { kind: "ready" }
  | { kind: "failed"; reason: FailureReason; repeated: boolean };

/** After this long, the loading state says it is still working and offers Cancel. */
export const SLOW_LOADING_MS = 10_000;
/** A second failure within this long adds "restart BIMCraft" to the error card. */
export const REPEATED_FAILURE_MS = 30_000;

export type ViewportSession = {
  phase: Phase;
  /** The canvas to mount; its key changes on Reload, so a new viewport gets a fresh canvas. */
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  canvasKey: number;
  /** The live viewport, or null while there is none. */
  handle: React.RefObject<ViewportHandle | null>;
  /** The scene feed of the live viewport, or null. */
  feed: React.RefObject<SceneFeed | null>;
  /** Whether a first scene has been applied. */
  sceneLoaded: boolean;
  /** How many deltas were applied: a change of the model may have changed what is drawn. */
  sceneVersion: number;
  reload: () => void;
  cancel: () => void;
};

/**
 * Runs one viewport on a canvas (spec D2): loads the module lazily, checks WebGPU, creates the
 * viewport, follows the canvas size and the colour scheme, feeds it the scene, and passes its
 * events to `onEvent`. Errors of the viewport and of the feed stop it; `reload` starts a new one.
 */
export function useViewportSession(onEvent: (event: ViewportEvent) => void): ViewportSession {
  const api = useApi();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const handle = useRef<ViewportHandle | null>(null);
  const feed = useRef<SceneFeed | null>(null);
  const listener = useRef(onEvent);
  const stopCurrent = useRef<(() => void) | null>(null);
  const lastFailure = useRef<number | null>(null);
  // A new viewport waits until the previous one freed its GPU surface on the same page.
  const previous = useRef<Promise<void>>(Promise.resolve());
  const [attempt, setAttempt] = useState(0);
  const [phase, setPhase] = useState<Phase>({ kind: "loading", slow: false });
  const [sceneLoaded, setSceneLoaded] = useState(false);
  const [sceneVersion, setSceneVersion] = useState(0);

  useEffect(() => {
    listener.current = onEvent;
  });

  const fail = useCallback((reason: FailureReason) => {
    stopCurrent.current?.();
    const now = Date.now();
    const repeated =
      lastFailure.current !== null && now - lastFailure.current < REPEATED_FAILURE_MS;
    lastFailure.current = now;
    setPhase({ kind: "failed", reason, repeated });
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: a reload changes `attempt`, which remounts the canvas this effect starts a viewport on.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null) return;
    let stopped = false;
    let release: () => void = () => undefined;
    const prior = previous.current;
    previous.current = new Promise<void>((resolve) => {
      release = resolve;
    });
    let observer: ResizeObserver | undefined;
    let unwatch: () => void = () => undefined;
    let created: ViewportHandle | null = null;

    setPhase({ kind: "loading", slow: false });
    setSceneLoaded(false);
    const slow = setTimeout(
      () =>
        setPhase((phase) => (phase.kind === "loading" ? { kind: "loading", slow: true } : phase)),
      SLOW_LOADING_MS,
    );

    const stop = () => {
      if (stopped) return;
      stopped = true;
      clearTimeout(slow);
      feed.current?.stop();
      feed.current = null;
      observer?.disconnect();
      unwatch();
      handle.current = null;
      created?.free();
      created = null;
      release();
    };
    stopCurrent.current = stop;

    const attach = (viewport: ViewportHandle) => {
      created = viewport;
      handle.current = viewport;
      viewport.onEvent((event) => {
        // Later, not now: the viewport cannot be freed from inside its own call.
        if (event.kind === "error") queueMicrotask(() => fail(event.key));
        else listener.current(event);
      });
      observer = new ResizeObserver(() => {
        const { width, height } = canvas.getBoundingClientRect();
        viewport.resize(Math.max(width, 1), Math.max(height, 1), window.devicePixelRatio || 1);
      });
      observer.observe(canvas);
      unwatch = watchColourScheme(() => viewport.setTheme(readTheme()));
      feed.current = startSceneFeed(
        api,
        {
          applyScene: (bytes) => {
            const revision = viewport.applyScene(bytes);
            setSceneLoaded(true);
            setSceneVersion((version) => version + 1);
            return revision;
          },
          revision: () => viewport.revision(),
        },
        () => fail("scene_rejected"),
      );
      clearTimeout(slow);
      setPhase({ kind: "ready" });
    };

    const start = async () => {
      await prior;
      const opened = stopped ? null : await openViewport(canvas, () => stopped);
      if (opened === null) return;
      if (opened.kind === "unavailable") {
        stop();
        setPhase({ kind: "unavailable" });
      } else if (opened.kind === "failed") {
        fail(opened.reason);
      } else {
        attach(opened.viewport);
      }
    };
    void start();
    return stop;
  }, [api, attempt, fail]);

  const reload = useCallback(() => setAttempt((count) => count + 1), []);
  const cancel = useCallback(() => fail("cancelled"), [fail]);

  return {
    phase,
    canvasRef,
    canvasKey: attempt,
    handle,
    feed,
    sceneLoaded,
    sceneVersion,
    reload,
    cancel,
  };
}

type Opened =
  | { kind: "open"; viewport: ViewportHandle }
  | { kind: "unavailable" }
  | { kind: "failed"; reason: FailureReason };

/**
 * Loads the module and creates a viewport on `canvas`: null when the session `stopped` meanwhile
 * (a viewport created too late is freed at once).
 */
async function openViewport(
  canvas: HTMLCanvasElement,
  stopped: () => boolean,
): Promise<Opened | null> {
  let module: ViewportModule;
  try {
    module = await loadViewportModule();
  } catch {
    return stopped() ? null : { kind: "failed", reason: "load_failed" };
  }
  const available = await module.webgpuAvailable().catch(() => false);
  if (stopped()) return null;
  if (!available) return { kind: "unavailable" };
  let viewport: ViewportHandle;
  try {
    viewport = await module.create(canvas, { theme: readTheme() });
  } catch {
    return stopped() ? null : { kind: "failed", reason: "device_lost" };
  }
  if (!stopped()) return { kind: "open", viewport };
  viewport.free();
  return null;
}
