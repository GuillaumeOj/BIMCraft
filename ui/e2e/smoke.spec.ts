import { expect, test } from "@playwright/test";

test("the app shell opens", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle("Fabrilo");
  await expect(page.getByRole("heading", { level: 1, name: "Fabrilo" })).toBeVisible();
});
