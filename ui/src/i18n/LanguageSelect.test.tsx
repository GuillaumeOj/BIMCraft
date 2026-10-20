import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "./I18nProvider";
import { LanguageSelect } from "./LanguageSelect";

// jsdom reports an English system language (navigator.languages is ["en-US", "en"]).
function renderSelect() {
  render(<LanguageSelect />, { wrapper: I18nProvider });
  return screen.getByRole("combobox", { name: "Language" });
}

describe("LanguageSelect", () => {
  it("is labelled and follows the system language by default", () => {
    const select = renderSelect();
    expect(select).toHaveValue("system");
    expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual([
      "System language",
      "English",
      "Français",
    ]);
  });

  it("switches the interface language from the keyboard and back to the system", async () => {
    const user = userEvent.setup();
    const select = renderSelect();
    await user.tab();
    expect(select).toHaveFocus();
    await user.selectOptions(select, "fr");
    const french = screen.getByRole("combobox", { name: "Langue" });
    expect(french).toHaveValue("fr");
    expect(document.documentElement.lang).toBe("fr");
    await user.selectOptions(french, "system");
    expect(screen.getByRole("combobox", { name: "Language" })).toHaveValue("system");
  });
});
