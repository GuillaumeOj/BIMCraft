import { expect, type Page, test } from "@playwright/test";

test.use({ locale: "en-GB" });

type Scenario = {
  /** The offer `update_check` returns, or null for "up to date". */
  offer: { version: string; date: string | null; notes: string } | null;
  /** When set, `update_check` rejects with this code. */
  checkError?: string;
};

const OFFER = {
  version: "0.4.0",
  date: "2026-11-12T12:00:00Z",
  notes: "- Faster viewport\n- Fixes",
};

// There is no desktop host in a browser, so the page gets a stand-in for Tauri's IPC that answers
// the update commands of docs api-contract.md. The shipped code only ever sees Tauri's interface.
// Tests read what the app called from `window.__stub` and settle the download with `finish()`.
const stub = (scenario: Scenario) => `(() => {
  const scenario = ${JSON.stringify(scenario)};
  const callbacks = new Map();
  const listeners = [];
  let lastCallback = 0;
  const stub = { checks: [], installs: 0, downloads: 0, finish: () => {} };
  window.__stub = stub;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback(callback) { lastCallback += 1; callbacks.set(lastCallback, callback); return lastCallback; },
    unregisterCallback(id) { callbacks.delete(id); },
    async invoke(command, args) {
      switch (command) {
        case "plugin:event|listen":
          listeners.push({ event: args.event, handler: args.handler });
          return args.handler;
        case "plugin:event|unlisten":
          return null;
        case "app_info":
          return { version: "0.3.0", updatable: true };
        case "update_check":
          stub.checks.push(args.channel);
          if (scenario.checkError) throw { code: scenario.checkError, message: "host message" };
          return scenario.offer;
        case "update_download": {
          stub.downloads += 1;
          const payload = { downloaded: 17000000, total: 40000000 };
          for (const l of listeners.filter((l) => l.event === "update://progress")) {
            callbacks.get(l.handler)?.({ event: l.event, id: l.handler, payload });
          }
          await new Promise((resolve) => { stub.finish = resolve; });
          return null;
        }
        case "update_install":
          stub.installs += 1;
          return new Promise(() => {});
        default:
          throw "unexpected command " + command;
      }
    },
  };
})();`;

async function start(page: Page, scenario: Scenario) {
  await page.addInitScript({ content: stub(scenario) });
  await page.clock.install();
  await page.goto("/");
}

const stubbed = (page: Page) =>
  page.evaluate(
    () => (window as unknown as { __stub: { checks: string[]; installs: number } }).__stub,
  );

test("start-up check → notice → download progress → restart confirmation → install", async ({
  page,
}) => {
  await start(page, { offer: OFFER });
  await expect(page.getByRole("button", { name: "Updates", exact: true })).toBeVisible();
  // Nothing happens before the delay, and the check is the app's own.
  expect((await stubbed(page)).checks).toEqual([]);
  await page.clock.runFor(6000);

  const pill = page.getByRole("button", { name: "Update available: Fabrilo 0.4.0" });
  await expect(pill).toBeVisible();
  await expect(page.getByRole("dialog")).toBeHidden();
  await expect(
    page.getByRole("status").filter({ hasText: "Fabrilo 0.4.0 is available. Open Updates" }),
  ).toBeAttached();

  await pill.click();
  const dialog = page.getByRole("dialog", { name: "Updates" });
  await expect(dialog.getByText("Released Nov 12, 2026.")).toBeVisible();
  await expect(dialog.getByRole("region", { name: "What's new in 0.4.0" })).toContainText(
    "Faster viewport",
  );
  await expect(dialog.getByRole("button", { name: "Download and install" })).toBeFocused();
  await dialog.getByRole("button", { name: "Download and install" }).click();

  const bar = dialog.getByRole("progressbar", { name: "Download progress" });
  await expect(bar).toHaveAttribute("aria-valuetext", "17 MB of 40 MB");
  await expect(bar).toHaveAttribute("aria-valuenow", "42");
  await page.evaluate(() =>
    (window as unknown as { __stub: { finish: () => void } }).__stub.finish(),
  );

  await expect(dialog.getByText("Fabrilo 0.4.0 is ready to install.")).toBeVisible();
  await dialog.getByRole("button", { name: "Restart now" }).click();
  const confirm = page.getByRole("dialog", { name: "Restart Fabrilo now?" });
  await expect(confirm.getByRole("button", { name: "Cancel" })).toBeFocused();
  expect((await stubbed(page)).installs).toBe(0);
  await confirm.getByRole("button", { name: "Restart now" }).click();
  await expect(page.getByText("Restarting Fabrilo…")).toBeVisible();
  expect((await stubbed(page)).installs).toBe(1);
});

test("Check now with no update says the program is up to date", async ({ page }) => {
  await start(page, { offer: null });
  await page.keyboard.press("Control+Shift+U");
  const dialog = page.getByRole("dialog", { name: "Updates" });
  await dialog.getByRole("button", { name: "Check now" }).click();
  await expect(dialog.getByText("Fabrilo 0.3.0 is up to date.")).toBeVisible();
  await expect(dialog.getByText(/^Last checked /)).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Close" }).last()).toBeFocused();
});

test("the channel and the automatic check are remembered across a reload", async ({ page }) => {
  await start(page, { offer: null });
  await page.getByRole("button", { name: "Updates", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Updates" });
  await dialog.getByRole("button", { name: "Beta" }).click();
  await expect(dialog.getByText("Fabrilo 0.3.0 is up to date.")).toBeVisible();
  expect((await stubbed(page)).checks).toEqual(["beta"]);
  await dialog.getByRole("checkbox", { name: "Check for updates automatically" }).uncheck();

  await page.reload();
  await page.getByRole("button", { name: "Updates", exact: true }).click();
  const again = page.getByRole("dialog", { name: "Updates" });
  await expect(again.getByRole("button", { name: "Beta" })).toHaveAttribute("aria-pressed", "true");
  await expect(
    again.getByRole("checkbox", { name: "Check for updates automatically" }),
  ).not.toBeChecked();
  // The switch is off: no check at start-up, even with no dialog in the way.
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toBeHidden();
  await page.clock.runFor(10_000);
  expect((await stubbed(page)).checks).toEqual([]);
});

test("an on-demand check offline shows the error; the start-up check stays silent", async ({
  page,
}) => {
  await start(page, { offer: null, checkError: "network" });
  // The timer is set once the host has said that the build can update.
  await expect(page.getByRole("button", { name: "Updates", exact: true })).toBeVisible();
  await page.clock.runFor(6000);
  await expect.poll(async () => (await stubbed(page)).checks).toEqual(["stable"]);
  await expect(page.getByRole("alert")).toBeHidden();

  await page.getByRole("button", { name: "Updates", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Updates" });
  await dialog.getByRole("button", { name: "Check now" }).click();
  await expect(dialog.getByRole("alert")).toContainText("Could not reach the update server.");
  await expect(dialog.getByRole("button", { name: "Try again" })).toBeFocused();
});

test("the update controls are in French when the language is", async ({ page }) => {
  await start(page, { offer: null });
  await page.getByRole("combobox", { name: "Language" }).selectOption({ label: "Français" });
  await page.getByRole("button", { name: "Mises à jour", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Mises à jour" });
  await expect(dialog.getByRole("button", { name: /^Rechercher/ })).toBeVisible();
});
