import { expect, type Page, test } from "@playwright/test";

test.use({ locale: "en-GB" });

// There is no desktop core in a browser, so the page gets a stand-in for Tauri's IPC: the `api`
// command answers the few georef.*, materials.* and history.* methods of the flow from memory
// (one grade, C25/30), and `model.changed` is pushed after each command. It carries no test hook
// in the shipped code: the app only ever sees Tauri's own interface.
const FAKE_CORE = `(() => {
  const documentName = "EN 1992-1-1:2004";
  const positive = { min: 0, min_inclusive: false, max: null, max_inclusive: false };
  const GRADE = { key: "en1992-1-1/C25/30", name: "C25/30", category: "concrete", design_code: "en1992-1-1", values: { fck: 25e6, fcm: 33e6 } };
  const TEMPLATE = [
    { key: "fck", quantity: "stress", required: true },
    { key: "fcm", quantity: "stress", required: false },
  ];
  let project = null;
  let steps = 0;
  const callbacks = new Map();
  const handlers = [];
  let lastCallback = 0;
  const refusal = (reason) => ({ code: -32000, message: "refused", data: { reason } });
  const notify = () => {
    const payload = JSON.stringify({ jsonrpc: "2.0", method: "model.changed", params: { created: [], deleted: [], modified: [] } });
    for (const handler of handlers) callbacks.get(handler)?.({ event: "api://notification", id: handler, payload });
  };
  const properties = (values, library) => TEMPLATE.map((t) => ({
    key: t.key, quantity: t.quantity, required: t.required, range: positive,
    value: values[t.key]?.value ?? null,
    source: values[t.key] ? (values[t.key].user ? { entered_by_user: true } : { document: documentName, clause: "Table 3.1" }) : null,
    library_value: library[t.key] ?? null,
  }));
  const methods = {
    "georef.get": () => ({ georeference: null }),
    "materials.library": () => ({ total: 1, items: [{ key: GRADE.key, name: GRADE.name, category: GRADE.category, design_code: GRADE.design_code }] }),
    "materials.grade": () => ({
      key: GRADE.key, name: GRADE.name, category: GRADE.category, design_code: GRADE.design_code,
      law: { kind: "parabola_rectangle", source: { document: documentName, clause: "3.1.7" }, branch: null },
      properties: properties(Object.fromEntries(Object.entries(GRADE.values).map(([k, value]) => [k, { value, user: false }])), GRADE.values),
    }),
    "materials.list": () => ({ items: project ? [{ id: project.id, name: project.name, category: "concrete", grade: GRADE.key, used_by: 0 }] : [] }),
    "materials.get": () => ({
      id: project.id, name: project.name, category: "concrete", design_code: GRADE.design_code, grade: GRADE.key,
      law: { kind: "parabola_rectangle", source: { document: documentName, clause: "3.1.7" }, branch: null },
      properties: properties(project.values, GRADE.values), used_by: 0,
      edited: Object.values(project.values).some((v) => v.user),
    }),
    "materials.add_from_library": ({ id, name }) => {
      project = { id, name: name ?? GRADE.name, values: Object.fromEntries(Object.entries(GRADE.values).map(([k, value]) => [k, { value, user: false }])) };
      steps += 1; setTimeout(notify, 0);
      return { id };
    },
    "materials.update": ({ properties: inputs }) => {
      for (const input of inputs) {
        if (input.value !== null && input.value <= 0) throw refusal("invalid_value");
        project.values[input.key] = input.value === null ? { value: GRADE.values[input.key], user: false } : { value: input.value, user: true };
      }
      steps += 1; setTimeout(notify, 0);
      return {};
    },
    "history.state": () => ({ can_undo: steps > 0, can_redo: false, next: null, entries: Array.from({ length: steps }, () => ({ command: "materials.update", origin: { kind: "ui" }, undone: false })) }),
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

test("a grade is added from the library, edited, marked as entered by the user and reset", async ({
  page,
}) => {
  await start(page);
  await page.keyboard.press("Control+Shift+M");
  const dialog = page.getByRole("dialog", { name: "Materials" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText(/No materials yet/)).toBeVisible();

  await dialog.getByRole("option", { name: /C25\/30/ }).click();
  await dialog.getByRole("button", { name: "Add to project" }).click();
  await expect(dialog.getByRole("heading", { name: "C25/30" })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "C25/30 added" })).toBeVisible();

  const fck = dialog.getByRole("button", { name: /^Edit Characteristic compressive strength/ });
  await expect(fck).toContainText("25 MPa");
  await fck.click();
  const field = dialog.getByRole("textbox", { name: "Characteristic compressive strength (MPa)" });
  await field.fill("30,5");
  await field.press("Enter");
  await expect(fck).toContainText("30.5 MPa");
  await expect(dialog.getByText("Entered by user")).toBeVisible();

  await dialog.getByRole("button", { name: /Reset to library value/ }).click();
  await expect(fck).toContainText("25 MPa");
  await expect(dialog.getByText("Entered by user")).toBeHidden();

  await fck.click();
  await field.fill("-1");
  await field.press("Enter");
  await expect(field).toBeFocused();
  await expect(field).toHaveAccessibleDescription(/Enter a valid value/);

  await page.keyboard.press("Escape");
  await dialog.getByRole("button", { name: "Done" }).click();
  await expect(dialog).toBeHidden();
});
