import { expect, test } from "@playwright/test";

test.use({ locale: "en-GB" });

test("every panel of the shell is reachable from the keyboard", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Tab");
  await expect(page.getByRole("link", { name: "Skip to viewport" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("main", { name: "Viewport" })).toBeFocused();

  await page.keyboard.press("F6");
  await expect(page.getByRole("region", { name: "Properties" })).toBeFocused();
  await page.keyboard.press("Control+Backquote");
  await expect(page.getByRole("region", { name: "Console" })).toBeFocused();
  await page.keyboard.press("F6");
  await expect(page.getByRole("banner")).toBeFocused();
  await page.keyboard.press("F6");
  await expect(page.getByRole("region", { name: "Project" })).toBeFocused();
});
