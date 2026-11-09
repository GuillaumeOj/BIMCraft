import { expect, type Page, test } from "@playwright/test";

test.use({ locale: "en-GB" });

// There is no desktop core in a browser, so the page gets a stand-in for Tauri's IPC: the `api`
// command answers the few georef.*, materials.*, buildups.* and history.* methods of the flow from
// memory (two materials), and `model.changed` is pushed after each command. It carries no test
// hook in the shipped code: the app only ever sees Tauri's own interface.
const FAKE_CORE = `(() => {
  const MATERIALS = [
    { id: "c".repeat(32), name: "Concrete C25/30", category: "concrete", grade: "en1992-1-1/C25/30", used_by: 0 },
    { id: "w".repeat(32), name: "Mineral wool", category: "insulation", grade: null, used_by: 0 },
  ];
  let buildUps = [];
  let steps = 0;
  let layerCount = 0;
  const callbacks = new Map();
  const handlers = [];
  let lastCallback = 0;
  const refusal = (reason, extra) => ({ code: -32000, message: "refused", data: { reason, ...extra } });
  const notify = () => {
    const payload = JSON.stringify({ jsonrpc: "2.0", method: "model.changed", params: { created: [], deleted: [], modified: [] } });
    for (const handler of handlers) callbacks.get(handler)?.({ event: "api://notification", id: handler, payload });
  };
  const defaults = { structure: 1, substrate: 2, insulation: 3, air_gap: 3, membrane: 4, finish: 5 };
  const toLayer = (l) => ({ id: l.id ?? "layer-" + ++layerCount, material: l.material, thickness: l.thickness, function: l.function, priority: l.priority ?? defaults[l.function], core: l.core ?? l.function === "structure" });
  const errorsOf = (layers) => layers.flatMap((l, index) => l.thickness > 0 ? [] : [{ code: "invalid_thickness", index }]);
  const sum = (layers) => layers.reduce((total, l) => total + l.thickness, 0);
  const methods = {
    "georef.get": () => ({ georeference: null }),
    "materials.list": () => ({ items: MATERIALS }),
    "buildups.list": () => ({ items: buildUps.map((b) => ({ id: b.id, name: b.name, usage: b.usage, layer_count: b.layers.length, total_thickness: sum(b.layers) })) }),
    "buildups.get": ({ buildup }) => {
      const b = buildUps.find((x) => x.id === buildup);
      return { id: b.id, name: b.name, usage: b.usage, description: null, total_thickness: sum(b.layers), thermal_resistance: null, layers_without_thermal_data: b.layers.length,
        layers: b.layers.map((l) => ({ ...l, material_name: MATERIALS.find((m) => m.id === l.material).name, name: null, thermal_resistance: null })) };
    },
    "buildups.preview": ({ layers }) => ({ errors: errorsOf(layers), layers: layers.map((l) => ({ priority: l.priority ?? defaults[l.function], core: l.core ?? l.function === "structure", thermal_resistance: null })), total_thickness: sum(layers), thermal_resistance: null, layers_without_thermal_data: layers.length }),
    "buildups.create": ({ id, name, usage, layers }) => {
      const errors = errorsOf(layers);
      if (errors.length) throw refusal(errors[0].code, { index: errors[0].index });
      buildUps.push({ id, name, usage, layers: layers.map(toLayer) });
      steps += 1; setTimeout(notify, 0);
      return { id, layers: [] };
    },
    "history.state": () => ({ can_undo: steps > 0, can_redo: false, next: null, entries: Array.from({ length: steps }, () => ({ command: "buildups.create", origin: { kind: "ui" }, undone: false })) }),
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback(callback) { lastCallback += 1; callbacks.set(lastCallback, callback); return lastCallback; },
    unregisterCallback(id) { callbacks.delete(id); },
    async invoke(command, args) {
      if (command === "plugin:event|listen") { handlers.push(args.handler); return args.handler; }
      if (command === "plugin:event|unlisten") return null;
      if (command !== "api") throw new Error("unexpected command " + command);
      const { id, method, params } = JSON.parse(args.message);
      try {
        return JSON.stringify({ jsonrpc: "2.0", id, result: methods[method](params) });
      } catch (error) {
        return JSON.stringify({ jsonrpc: "2.0", id, error });
      }
    },
  };
})();`;

async function start(page: Page) {
  await page.addInitScript({ content: FAKE_CORE });
  await page.goto("/");
}

test("a wall build-up is built, reordered from the keyboard and applied", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Control+Shift+B");
  const dialog = page.getByRole("dialog", { name: "Build-ups" });
  await expect(dialog.getByText(/No build-ups yet/)).toBeVisible();
  await dialog.getByRole("button", { name: "New wall build-up" }).click();

  const name = dialog.getByRole("textbox", { name: "Name" });
  await expect(name).toBeFocused();
  await dialog.getByRole("button", { name: "Apply" }).click();
  await expect(dialog.getByText("Choose a material.").first()).toBeVisible();

  await name.fill("Wall 200 + insulation");
  await dialog
    .getByRole("combobox", { name: "Material of layer 1" })
    .selectOption("Concrete C25/30");
  await dialog.getByRole("textbox", { name: "Thickness of layer 1 (mm)" }).fill("200");
  await dialog.getByRole("button", { name: "Add layer" }).click();
  await expect(dialog.getByRole("combobox", { name: "Function of layer 2" })).toBeFocused();
  await dialog.getByRole("combobox", { name: "Function of layer 2" }).selectOption("insulation");
  await dialog.getByRole("combobox", { name: "Material of layer 2" }).selectOption("Mineral wool");
  await dialog.getByRole("textbox", { name: "Thickness of layer 2 (mm)" }).fill("100,5");
  await expect(dialog.getByText("Total thickness: 300.5 mm")).toBeVisible();

  const up = dialog.getByRole("button", { name: "Move layer 2 up" });
  await up.focus();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(dialog.getByRole("combobox", { name: "Function of layer 1" })).toHaveValue(
    "insulation",
  );
  await expect(dialog.getByRole("button", { name: "Move layer 1 up" })).toBeFocused();
  await expect(
    dialog.getByText("Layer moved to position 1 of 2: Mineral wool, 100.5 mm."),
  ).toBeAttached();

  await dialog.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Build-up applied" })).toBeVisible();
  const row = dialog.getByRole("button", { name: /^Wall 200 \+ insulation/ });
  await expect(row).toContainText("2 layers · 300.5 mm");
  await expect(row).toBeFocused();
});
