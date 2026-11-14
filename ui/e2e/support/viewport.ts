import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { Locator, Page } from "@playwright/test";

// Scene deltas written by a Rust test from known documents (fixtures/README.md).
const fixture = (name: string) => [
  ...readFileSync(join(import.meta.dirname, "..", "fixtures", name)),
];
export const SCENES = {
  empty: fixture("scene-empty.bin"),
  blocks: fixture("scene-blocks.bin"),
  move: fixture("scene-move.bin"),
};

export type Scenes = typeof SCENES;
export type Recorded = { method: string; params: unknown };

declare global {
  interface Window {
    __core: { calls: Recorded[] };
  }
}

/**
 * A stand-in for Tauri's IPC, as in the other specs: the `api` command answers the few methods
 * of these flows from memory and pushes `model.changed` after each command, and the `scene`
 * command answers with the fixture bytes: the full scene first, the move delta after a move,
 * the full scene again after an undo (as the host does for a revision it no longer serves), and
 * an empty delta otherwise. The shipped code sees only Tauri's own interface.
 */
export function fakeCore({ scenes, start }: { scenes: Scenes; start: "empty" | "blocks" }) {
  const bytes = (values: number[]) => new Uint8Array(values).buffer;
  // An empty delta from `since` to itself: the 32-byte header and five empty sections.
  const emptyDelta = (since: number) => {
    const view = new DataView(new ArrayBuffer(52));
    // "BCSW", version 1.
    view.setUint32(0, 0x57534342, true);
    view.setUint16(4, 1, true);
    view.setBigUint64(8, BigInt(since), true);
    view.setBigUint64(16, BigInt(since), true);
    return view.buffer;
  };
  const blocks = new Set<string>(
    start === "blocks" ? ["1", "2", "3"].map((n) => n.padStart(32, "0")) : [],
  );
  let steps = 0;
  let next: ArrayBuffer | null = null;
  const calls: Recorded[] = [];
  const callbacks = new Map<number, (event: unknown) => void>();
  const handlers: number[] = [];
  let lastCallback = 0;
  const notify = () => {
    const payload = JSON.stringify({
      jsonrpc: "2.0",
      method: "model.changed",
      params: { created: [], deleted: [], modified: [] },
    });
    for (const handler of handlers) {
      callbacks.get(handler)?.({ event: "api://notification", id: handler, payload });
    }
  };
  const step = (scene: ArrayBuffer) => {
    steps += 1;
    next = scene;
    setTimeout(notify, 0);
    return null;
  };
  const methods: Record<string, (params: never) => unknown> = {
    "model.list_objects": () => ({ items: [...blocks].slice(0, 1), next: null }),
    "model.create": ({ id }: { id: string }) => {
      blocks.add(id);
      return step(bytes(scenes.blocks));
    },
    "elements.move": () => step(bytes(scenes.move)),
    "history.state": () => ({
      can_undo: steps > 0,
      can_redo: false,
      next: null,
      entries: Array.from({ length: steps }, () => ({
        command: "model.batch",
        origin: { kind: "ui" },
        undone: false,
      })),
    }),
    "history.undo": () => {
      steps -= 1;
      next = bytes(scenes[start]);
      setTimeout(notify, 0);
      return null;
    },
  };
  window.__core = { calls };
  Object.assign(window, {
    __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener() {} },
    __TAURI_INTERNALS__: {
      transformCallback(callback: (event: unknown) => void) {
        lastCallback += 1;
        callbacks.set(lastCallback, callback);
        return lastCallback;
      },
      unregisterCallback(id: number) {
        callbacks.delete(id);
      },
      async invoke(
        command: string,
        args: { handler?: number; message?: string; since?: number | null },
      ) {
        if (command === "plugin:event|listen") {
          handlers.push(args.handler ?? 0);
          return args.handler;
        }
        if (command === "plugin:event|unlisten") return null;
        if (command === "scene") {
          if (args.since === null || args.since === undefined) return bytes(scenes[start]);
          const scene = next ?? emptyDelta(args.since);
          next = null;
          return scene;
        }
        const { id, method, params } = JSON.parse(args.message ?? "{}");
        calls.push({ method, params });
        const handler = methods[method];
        if (handler === undefined) {
          return JSON.stringify({ jsonrpc: "2.0", id, error: { code: -32601, message: "no" } });
        }
        return JSON.stringify({ jsonrpc: "2.0", id, result: handler(params as never) });
      },
    },
  });
}

export async function open(page: Page, start: "empty" | "blocks") {
  // The navigation hint would cover part of the model.
  await page.addInitScript(() =>
    localStorage.setItem("bimcraft.viewport.navigationHint", "hidden"),
  );
  await page.addInitScript(fakeCore, { scenes: SCENES, start });
  await page.goto("/");
  return page.getByRole("application", { name: "3D view" });
}

export const status = (page: Page) => page.locator("#vp-status");
export const calls = (page: Page, method: string) =>
  page.evaluate((name) => window.__core.calls.filter((call) => call.method === name), method);

/**
 * Clicks the canvas on a grid of points around its centre until one selects an element: the
 * camera frames the whole model, but where each block lands on screen is the renderer's business.
 */
export async function selectAnElement(page: Page, canvas: Locator) {
  const box = await canvas.boundingBox();
  if (box === null) throw new Error("the canvas has no box");
  for (const dy of [0, -40, 40, -80, 80]) {
    for (const dx of [0, -40, 40, -80, 80, -120, 120]) {
      const point = { x: box.x + box.width / 2 + dx, y: box.y + box.height / 2 + dy };
      await page.mouse.click(point.x, point.y);
      if (/1 element selected/.test((await status(page).textContent()) ?? "")) return point;
    }
  }
  throw new Error("no click selected an element");
}
