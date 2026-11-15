import { expect, type Locator, type Page, test } from "@playwright/test";
import { open, reportViewStops, selectAnElement, status } from "./support/viewport";

test.use({ locale: "en-GB", viewport: { width: 1440, height: 900 } });

reportViewStops();

const extents = ["Min X", "Max X", "Min Y", "Max Y", "Min Z", "Max Z"];

async function showViewPanel(page: Page) {
  await page.getByRole("tab", { name: "View" }).click();
  await expect(page.getByRole("tabpanel", { name: "View" })).toBeVisible();
}

const field = (page: Page, name: string) => page.getByRole("textbox", { name });
const values = (page: Page) => Promise.all(extents.map((name) => field(page, name).inputValue()));

/**
 * Drags the section box's +X face outwards. The handles sit where the renderer puts them, so the
 * test looks at the model from the top in parallel projection, where the +X face is the right-hand
 * edge of the box, and tries points from the right of the canvas inwards along the middle row
 * until one grabs a handle: the status then reports a moved face.
 */
async function dragAFace(page: Page, canvas: Locator) {
  const box = await canvas.boundingBox();
  if (box === null) throw new Error("the canvas has no box");
  const y = box.y + box.height / 2;
  for (let x = box.x + box.width - 20; x > box.x + box.width / 2; x -= 10) {
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 8, y, { steps: 2 });
    await page.mouse.move(x - 24, y, { steps: 4 });
    await page.mouse.up();
    if (/face(s)? moved/.test((await status(page).textContent()) ?? "")) return;
  }
  throw new Error("no drag grabbed a handle of the section box");
}

test("cut the model with a section box: type an extent, drag a face, turn it off", async ({
  page,
}) => {
  const canvas = await open(page, "blocks");
  await expect(status(page)).toHaveText("No selection · Snap on");
  await showViewPanel(page);

  // Off to begin with: no extents.
  const active = page.getByRole("checkbox", { name: /^Active/ });
  await expect(active).not.toBeChecked();
  await expect(field(page, "Max X")).toBeHidden();

  // The checkbox follows the state the view reports a frame later, so it is clicked, then awaited.
  await active.click();
  await expect(active).toBeChecked();
  await expect(page.getByText("Section box active")).toBeVisible();
  await expect(
    page.getByText(/Section box · drag a handle or type values in the View panel/),
  ).toBeVisible();
  await expect(field(page, "Max X")).toBeVisible();

  // A typed extent moves the box, but only a drag counts as a moved face: the status stays.
  await field(page, "Max X").fill("12.5");
  await field(page, "Max X").press("Enter");
  await expect(field(page, "Max X")).toHaveValue("12.500");
  await expect(status(page)).toHaveText("No selection · Snap on");

  // A minimum that is not below the maximum is refused with a reason.
  await field(page, "Min Z").fill("99");
  await field(page, "Min Z").press("Enter");
  await expect(page.getByText("Minimum must be below maximum")).toBeVisible();
  await field(page, "Min Z").press("Escape");
  await expect(page.getByText("Minimum must be below maximum")).toBeHidden();

  // Dragging a face of the box changes an extent.
  await page.getByRole("button", { name: "View presets" }).click();
  await page.getByRole("menuitem", { name: "Top" }).click();
  await page
    .getByRole("group", { name: "Projection" })
    .getByRole("button", { name: "Parallel" })
    .click();
  await page.getByRole("button", { name: "Zoom fit" }).click();
  await page.getByRole("button", { name: "Reset" }).click();
  const before = await values(page);
  await dragAFace(page, canvas);
  await expect(status(page)).toHaveText("Section box · 1 face moved");
  expect(await values(page)).not.toEqual(before);

  // Alt+B switches the box off from anywhere.
  await page.keyboard.press("Alt+b");
  await expect(active).not.toBeChecked();
});

test("hide a category from the Visibility list, then reset", async ({ page }) => {
  await open(page, "blocks");
  await expect(status(page)).toHaveText("No selection · Snap on");
  await showViewPanel(page);
  await expect(page.getByText("Everything is visible", { exact: true }).last()).toBeVisible();

  const opener = page.getByRole("button", { name: "Visibility", exact: true });
  await opener.click();
  const popover = page.getByRole("dialog", { name: "Visibility" });
  const row = popover.getByRole("switch", { name: /^Block/ });
  await expect(row).toBeFocused();
  await page.keyboard.press(" ");
  await expect(row).not.toBeChecked();
  await expect(popover.getByText("Hidden")).toBeVisible();
  await expect(page.getByText("Hidden categories")).toBeVisible();

  // Esc closes the popover and the focus returns to the button.
  await page.keyboard.press("Escape");
  await expect(popover).toBeHidden();
  await expect(opener).toBeFocused();

  await page.getByRole("button", { name: /^Reset visibility/ }).click();
  await expect(page.getByText("Hidden categories")).toBeHidden();

  // Alt+H resets from the keyboard too.
  await opener.click();
  await page.keyboard.press(" ");
  await expect(page.getByText("Hidden categories")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Alt+h");
  await expect(page.getByText("Hidden categories")).toBeHidden();
});

test("isolate the selection, end the isolate with the button and with Esc", async ({ page }) => {
  const canvas = await open(page, "blocks");
  await expect(status(page)).toHaveText("No selection · Snap on");
  await page.getByRole("button", { name: "Zoom fit" }).click();
  await showViewPanel(page);
  await selectAnElement(page, canvas);

  await page.keyboard.press("i");
  await expect(page.getByText("Isolated", { exact: true }).first()).toBeVisible();
  await expect(
    page.getByText(/Isolate · only the selection is shown · Esc or “End isolate” to return/),
  ).toBeVisible();

  await page.getByRole("button", { name: "End isolate" }).click();
  await expect(page.getByRole("button", { name: "End isolate" })).toBeHidden();

  // Esc, from outside the canvas, ends it as well.
  await page.keyboard.press("i");
  await expect(page.getByRole("button", { name: "End isolate" })).toBeVisible();
  await page.getByRole("tab", { name: "View" }).focus();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "End isolate" })).toBeHidden();
});

test("clipping planes: arm a face pick, then cancel it with Esc", async ({ page }) => {
  await open(page, "blocks");
  await showViewPanel(page);
  await page.getByRole("button", { name: "Add plane on face" }).click();
  await expect(page.getByText("Click a face to cut at it · Esc to cancel")).toBeVisible();
  await expect(status(page)).toHaveText("Clipping plane · choose a face");
  await page.keyboard.press("Escape");
  await expect(page.getByText("Click a face to cut at it · Esc to cancel")).toBeHidden();
});
