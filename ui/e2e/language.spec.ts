import { expect, test } from "@playwright/test";

test.use({ locale: "en-GB" });

test("the interface language can be switched to French and is remembered", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("combobox", { name: "Language" }).selectOption({ label: "Français" });
  await expect(page.getByRole("combobox", { name: "Langue" })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "fr");
  await page.reload();
  await expect(page.getByRole("combobox", { name: "Langue" })).toHaveValue("fr");
});

test.describe("with a French system", () => {
  test.use({ locale: "fr-FR" });

  test("the interface follows the system language", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("combobox", { name: "Langue" })).toHaveValue("system");
    await expect(page.locator("html")).toHaveAttribute("lang", "fr");
  });
});
