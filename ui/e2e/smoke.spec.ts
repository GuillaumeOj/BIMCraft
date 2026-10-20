import { expect, test } from "@playwright/test";

test("the app shell opens", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle("BIMCraft");
  await expect(page.getByRole("heading", { level: 1, name: "BIMCraft" })).toBeVisible();
});
