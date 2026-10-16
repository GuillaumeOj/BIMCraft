import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { I18nProvider } from "./i18n/I18nProvider";

describe("App", () => {
  it("shows the application name as the main heading and a language choice", () => {
    render(
      <I18nProvider>
        <App />
      </I18nProvider>,
    );
    expect(screen.getByRole("heading", { level: 1, name: "BIMCraft" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Language" })).toBeInTheDocument();
  });
});
