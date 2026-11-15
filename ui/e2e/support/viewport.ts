import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, type Locator, type Page, type TestInfo, test } from "@playwright/test";

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

/** How long a click has to show its selection in the status bar: the event comes a frame later. */
const SELECTION_WAIT_MS = 250;
/** The floating panels and the tool bar cover these margins of the canvas. */
const MARGIN = { left: 280, right: 320, top: 60, bottom: 130 };
const STEP = 40;

/**
 * Clicks the canvas on a grid, from its centre outwards, until one click selects an element. The
 * camera frames the whole model, but where each block lands on screen (and which gaps lie
 * between them, for example in a top view) is the renderer's business, so the grid covers the
 * whole free area. Each click is given a moment to show its selection before the next one, which
 * would clear it on empty space.
 */
export async function selectAnElement(page: Page, canvas: Locator) {
  const box = await canvas.boundingBox();
  if (box === null) throw new Error("the canvas has no box");
  const centre = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const reach = {
    x: Math.min(centre.x - box.x - MARGIN.left, box.x + box.width - MARGIN.right - centre.x),
    y: Math.min(centre.y - box.y - MARGIN.top, box.y + box.height - MARGIN.bottom - centre.y),
  };
  const offsets: [number, number][] = [];
  for (let dy = -reach.y; dy <= reach.y; dy += STEP) {
    for (let dx = -reach.x; dx <= reach.x; dx += STEP) offsets.push([dx, dy]);
  }
  offsets.sort((a, b) => Math.hypot(...a) - Math.hypot(...b));
  const tried: string[] = [];
  for (const [dx, dy] of offsets) {
    const point = { x: centre.x + dx, y: centre.y + dy };
    await page.mouse.click(point.x, point.y);
    const selected = await expect(status(page))
      .toHaveText(/1 element selected/, { timeout: SELECTION_WAIT_MS })
      .then(
        () => true,
        () => false,
      );
    if (selected) return point;
    tried.push(`${Math.round(point.x)},${Math.round(point.y)}`);
  }
  // What the failure needs to be understood: where it clicked, and what the view said.
  const text = await status(page).textContent();
  throw new Error(
    `no click selected an element: canvas ${JSON.stringify(box)}, ${tried.length} points tried ` +
      `(${tried.slice(0, 12).join(" ")}…), status "${text}"`,
  );
}

const logs = new WeakMap<Page, { kept: string[]; dropped: number }>();
/** A GPU fault can log on every frame: the first lines name the cause, the rest only repeat it. */
const KEPT_LINES = 200;

/**
 * Keeps the warnings and errors the page logs: the viewport logs GPU validation errors as
 * warnings.
 */
function watchViewStops(page: Page) {
  const logged = { kept: [] as string[], dropped: 0 };
  logs.set(page, logged);
  const keep = (line: string) => {
    if (logged.kept.length < KEPT_LINES) logged.kept.push(line);
    else logged.dropped += 1;
  };
  page.on("console", (message) => {
    const type = message.type();
    if (type === "warning" || type === "error") keep(`${type}: ${message.text()}`);
  });
  page.on("pageerror", (error) => keep(`page error: ${error.message}`));
  page.on("crash", () => keep("the page crashed"));
}

/**
 * Attaches to a failed test the 3D view's status and error card, and what the page logged. The
 * View panel shows only what a running view reports, so a view that stopped makes its controls
 * vanish: the attachment says why, where the failure itself only says "element(s) not found".
 * A crashed page can no longer be read: the log then says so.
 */
async function attachViewStop(page: Page, testInfo: TestInfo) {
  if (testInfo.status === testInfo.expectedStatus) return;
  const read = (locator: Locator) =>
    locator
      .allInnerTexts()
      .then((texts) => texts.join(" / "))
      .catch(() => "(the page can no longer be read)");
  const [shown, card] = await Promise.all([
    read(status(page)),
    read(page.locator(".vp-card-error")),
  ]);
  const state = [`Status: ${shown || "none"}`, `Error card: ${card || "none"}`];
  const logged = logs.get(page);
  const lines = logged?.kept ?? [];
  const dropped = logged?.dropped ? [`(${logged.dropped} more lines not kept)`] : [];
  await testInfo.attach("3D view", {
    body: [...state, "", ...lines, ...dropped].join("\n"),
    contentType: "text/plain",
  });
}

/**
 * Has every test of the calling spec report, when it fails, whether the 3D view stopped and why:
 * a GPU fault on CI's software renderer otherwise fails far from its cause.
 */
export function reportViewStops() {
  test.beforeEach(({ page }) => watchViewStops(page));
  test.afterEach(({ page }, testInfo) => attachViewStop(page, testInfo));
}
