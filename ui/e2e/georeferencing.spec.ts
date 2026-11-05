import { expect, type Page, test } from "@playwright/test";

test.use({ locale: "en-GB" });

// There is no desktop core in a browser, so the page gets a stand-in for Tauri's IPC: the
// `api` command answers the georef.* JSON-RPC methods from memory, with a linear model instead
// of a real projection, and `model.changed` is pushed after each command. It is deterministic
// and carries no test hook in the shipped code: the app only ever sees Tauri's own interface.
const FAKE_CORE = `(() => {
  const CRS = [
    { epsg: 2154, name: "RGF93 v1 / Lambert-93", kind: "projected", geographic_name: "RGF93 v1", area: "France - onshore and offshore, mainland and Corsica" },
    { epsg: 3948, name: "RGF93 v1 / CC48", kind: "projected", geographic_name: "RGF93 v1", area: "France - 48N" },
  ];
  let value = null;
  const callbacks = new Map();
  const handlers = [];
  let lastCallback = 0;
  const rotation = () => (value ? value.grid_rotation : 0);
  const origin = () => (value ? value.base_point : [0, 0, 0]);
  const toProjected = ([x, y, z]) => {
    const [c, s] = [Math.cos(rotation()), Math.sin(rotation())];
    const [e, n, h] = origin();
    return [e + c * x - s * y, n + s * x + c * y, h + z];
  };
  const toLocal = ([e, n, h]) => {
    const [c, s] = [Math.cos(rotation()), Math.sin(rotation())];
    const [e0, n0, h0] = origin();
    const [dx, dy] = [e - e0, n - n0];
    return [c * dx + s * dy, -s * dx + c * dy, h - h0];
  };
  const toGeographic = ([e, n, h]) => [n / 1e8, e / 1e8, h];
  const fromGeographic = ([lat, lon, h]) => [lon * 1e8, lat * 1e8, h];
  const refusal = (reason) => ({ code: -32000, message: "refused", data: { reason } });
  const notify = () => {
    const payload = JSON.stringify({ jsonrpc: "2.0", method: "model.changed", params: { created: [], deleted: [], modified: [] } });
    for (const handler of handlers) callbacks.get(handler)?.({ event: "api://notification", id: handler, payload });
  };
  const methods = {
    "georef.list_crs": () => ({ crs: CRS }),
    "georef.get": () => value === null ? { georeference: null } : {
      georeference: {
        site: "5".repeat(32), value, crs: CRS.find((c) => c.epsg === value.crs_epsg),
        derived: {
          true_north: -value.grid_rotation - 0.0082, convergence: -0.0082, point_scale_factor: 0.999925,
          base_point_geographic: toGeographic(value.base_point),
          survey_point_local: value.survey_point === null ? null : toLocal(value.survey_point),
        },
      },
    },
    "georef.set": (params) => {
      if (Math.abs(params.base_point[0]) > 1e7) throw refusal("outside_domain");
      value = { crs_epsg: params.crs_epsg, base_point: params.base_point, grid_rotation: params.grid_rotation, scale: params.scale, survey_point: params.survey_point };
      setTimeout(notify, 0);
      return { site: "5".repeat(32) };
    },
    "georef.clear": () => { value = null; setTimeout(notify, 0); return {}; },
    "georef.convert": ({ from, points }) => {
      if (value === null) throw refusal("not_georeferenced");
      return { points: points.map((p) => {
        const projected = from === "local" ? toProjected(p) : from === "projected" ? p : fromGeographic(p);
        return { local: toLocal(projected), projected, geographic: toGeographic(projected) };
      }) };
    },
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

test("the project can be placed on a coordinate system from the keyboard", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Control+Shift+L");
  const dialog = page.getByRole("dialog", { name: "Project location" });
  await expect(dialog).toBeVisible();
  const crs = dialog.getByRole("combobox", { name: "Coordinate system" });
  await expect(crs).toBeFocused();

  await crs.fill("lambert");
  await page.keyboard.press("Enter");
  await expect(crs).toHaveValue("2154 – RGF93 v1 / Lambert-93");
  await dialog.getByRole("textbox", { name: "Easting" }).first().fill("652000,5");
  await dialog.getByRole("textbox", { name: "Northing" }).first().fill("6862000");
  await dialog.getByRole("textbox", { name: "Height" }).first().fill("35");
  await dialog.getByRole("textbox", { name: "Grid rotation" }).fill("0");
  await dialog.getByRole("button", { name: "Apply" }).click();

  await expect(dialog).toBeHidden();
  await expect(
    page.getByRole("status").filter({ hasText: "Project location applied" }),
  ).toBeVisible();

  await page.keyboard.press("Control+Shift+L");
  await expect(dialog.getByRole("textbox", { name: "Easting" }).first()).toHaveValue("652000.500");
  await expect(dialog.getByRole("button", { name: "Remove georeference" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("a point entered in one system is read in the other two", async ({ page }) => {
  await start(page);
  await page.getByRole("tab", { name: "Coordinates" }).click();
  await expect(page.getByText(/no location yet/)).toBeVisible();

  await page.getByRole("button", { name: "Set project location…" }).click();
  const dialog = page.getByRole("dialog", { name: "Project location" });
  await dialog.getByRole("combobox", { name: "Coordinate system" }).fill("2154");
  await page.keyboard.press("Enter");
  await dialog.getByRole("textbox", { name: "Easting" }).first().fill("652000");
  await dialog.getByRole("textbox", { name: "Northing" }).first().fill("6862000");
  await dialog.getByRole("textbox", { name: "Height" }).first().fill("35");
  await dialog.getByRole("button", { name: "Apply" }).click();
  await expect(dialog).toBeHidden();

  // The panel shows the model origin first, in the three systems.
  const model = page.getByRole("region", { name: "Model (local)" });
  const map = page.getByRole("region", { name: "Map" });
  await expect(map.getByRole("textbox", { name: "Easting (E)" })).toHaveValue("652000.000");

  await model.getByRole("textbox", { name: "x", exact: true }).fill("10,5");
  await model.getByRole("textbox", { name: "y", exact: true }).fill("20");
  await model.getByRole("textbox", { name: "y", exact: true }).press("Enter");
  await expect(map.getByRole("textbox", { name: "Easting (E)" })).toHaveValue("652010.500");
  await expect(map.getByRole("textbox", { name: "Northing (N)" })).toHaveValue("6862020.000");
  await expect(model.getByText("Edited")).toBeVisible();
  await expect(map.getByText("Computed from Model (local)")).toBeVisible();

  await map.getByRole("textbox", { name: "Easting (E)" }).fill("652100");
  await map.getByRole("textbox", { name: "Easting (E)" }).press("Enter");
  await expect(model.getByRole("textbox", { name: "x", exact: true })).toHaveValue("100.000");
});

test("a base point the core refuses is explained at the field", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Control+Shift+L");
  const dialog = page.getByRole("dialog", { name: "Project location" });
  await dialog.getByRole("combobox", { name: "Coordinate system" }).fill("2154");
  await page.keyboard.press("Enter");
  await dialog.getByRole("textbox", { name: "Easting" }).first().fill("99999999");
  await dialog.getByRole("textbox", { name: "Northing" }).first().fill("1");
  await dialog.getByRole("textbox", { name: "Height" }).first().fill("1");
  await dialog.getByRole("button", { name: "Apply" }).click();
  await expect(dialog.getByRole("textbox", { name: "Easting" }).first()).toBeFocused();
  await expect(
    dialog.getByRole("textbox", { name: "Easting" }).first(),
  ).toHaveAccessibleDescription(/outside the area/);
  await expect(dialog).toBeVisible();
});
