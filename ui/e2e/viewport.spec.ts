import { expect, test } from "@playwright/test";
import { calls, open, selectAnElement, status } from "./support/viewport";

test.use({ locale: "en-GB", viewport: { width: 1440, height: 900 } });

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
