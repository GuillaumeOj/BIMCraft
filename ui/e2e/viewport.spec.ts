import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, type Locator, type Page, test } from "@playwright/test";

test.use({ locale: "en-GB", viewport: { width: 1440, height: 900 } });

// Scene deltas written by a Rust test from known documents (fixtures/README.md).
const fixture = (name: string) => [...readFileSync(join(import.meta.dirname, "fixtures", name))];
const SCENES = {
  empty: fixture("scene-empty.bin"),
  blocks: fixture("scene-blocks.bin"),
  move: fixture("scene-move.bin"),
};

type Scenes = typeof SCENES;
type Recorded = { method: string; params: unknown };

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
function fakeCore({ scenes, start }: { scenes: Scenes; start: "empty" | "blocks" }) {
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

async function open(page: Page, start: "empty" | "blocks") {
  // The navigation hint would cover part of the model.
  await page.addInitScript(() =>
    localStorage.setItem("bimcraft.viewport.navigationHint", "hidden"),
  );
  await page.addInitScript(fakeCore, { scenes: SCENES, start });
  await page.goto("/");
  return page.getByRole("application", { name: "3D view" });
}

const status = (page: Page) => page.locator("#vp-status");
const calls = (page: Page, method: string) =>
  page.evaluate((name) => window.__core.calls.filter((call) => call.method === name), method);

/**
 * Clicks the canvas on a grid of points around its centre until one selects an element: the
 * camera frames the whole model, but where each block lands on screen is the renderer's business.
 */
async function selectAnElement(page: Page, canvas: Locator) {
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

test("navigate the model and select an element", async ({ page }) => {
  const canvas = await open(page, "blocks");
  await expect(canvas).toBeVisible();
  await expect(status(page)).toHaveText("No selection · Snap on");

  await page.getByRole("button", { name: "View presets" }).click();
  await page.getByRole("menuitem", { name: "Top" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Top view" })).toBeAttached();
  await page
    .getByRole("group", { name: "Projection" })
    .getByRole("button", { name: "Parallel" })
    .click();
  await expect(
    page.getByRole("group", { name: "Projection" }).getByRole("button", { name: "Parallel" }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "Zoom fit" }).click();

  await selectAnElement(page, canvas);
  await expect(status(page)).toHaveText("1 element selected · Esc to clear · Snap on");
  await expect(page.getByRole("button", { name: "Zoom to selection" })).toHaveAttribute(
    "aria-disabled",
    "false",
  );

  // Keyboard: the canvas takes arrows and Esc, and never keeps Tab.
  await canvas.focus();
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Escape");
  await expect(status(page)).toHaveText("No selection · Snap on");
  await page.keyboard.press("Tab");
  await expect(canvas).not.toBeFocused();
});

test("place a block with typed sizes, then undo it", async ({ page }) => {
  const canvas = await open(page, "empty");
  await expect(page.getByRole("heading", { name: "Nothing to see yet" })).toBeVisible();
  await page.getByRole("button", { name: "Place a block" }).click();
  await expect(page.getByRole("button", { name: /^Block/ })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  const box = await canvas.boundingBox();
  if (box === null) throw new Error("the canvas has no box");
  const centre = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await page.mouse.click(centre.x, centre.y);
  await page.mouse.move(centre.x + 60, centre.y + 10);
  await canvas.focus();
  for (const value of ["6", "4", "3"]) {
    await page.keyboard.type(value);
    await page.keyboard.press("Enter");
  }

  await expect(
    page.getByRole("status").filter({ hasText: "Block placed · 6.000 × 4.000 × 3.000 m" }),
  ).toBeVisible();
  const [created] = await calls(page, "model.create");
  expect(created?.params).toMatchObject({
    entity: "Block",
    values: expect.arrayContaining([
      ["size_x", { Real: 6 }],
      ["size_y", { Real: 4 }],
      ["size_z", { Real: 3 }],
    ]),
  });
  await expect(page.getByRole("heading", { name: "Nothing to see yet" })).toBeHidden();

  await page.getByRole("button", { name: "Undo" }).click();
  await expect.poll(async () => (await calls(page, "history.undo")).length).toBe(1);
});

test("move an element with a snap and a typed distance, then undo", async ({ page }) => {
  const canvas = await open(page, "blocks");
  await expect(status(page)).toHaveText("No selection · Snap on");
  await page.getByRole("button", { name: "Zoom fit" }).click();
  const point = await selectAnElement(page, canvas);

  await page.keyboard.press("m");
  await expect(page.getByRole("button", { name: /^Move/ })).toHaveAttribute("aria-pressed", "true");
  // The base point snaps to the selected block.
  await page.mouse.move(point.x + 1, point.y + 1);
  await expect(page.getByRole("status").filter({ hasText: /^Snap: / })).toBeAttached();
  await page.mouse.click(point.x, point.y);
  await expect(page.getByText(/click the target point or type a distance/)).toBeVisible();
  await page.mouse.move(point.x + 80, point.y);
  await canvas.focus();
  await page.keyboard.type("2.5");
  await page.keyboard.press("Enter");

  await expect(
    page.getByRole("status").filter({ hasText: "Moved 1 element by 2.500 m" }),
  ).toBeVisible();
  const [move] = await calls(page, "elements.move");
  const { translation } = (move?.params ?? { translation: [] }) as { translation: number[] };
  expect(Math.hypot(...translation)).toBeCloseTo(2.5, 6);

  await page.getByRole("button", { name: "Undo" }).click();
  await expect.poll(async () => (await calls(page, "history.undo")).length).toBe(1);
});

test("explains that the 3D view needs WebGPU", async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, "gpu", { value: undefined }));
  await open(page, "blocks");
  await expect(
    page.getByRole("heading", { name: "The 3D view isn't available on this computer" }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "Open the troubleshooting guide" })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Select/ })).toBeDisabled();
});
