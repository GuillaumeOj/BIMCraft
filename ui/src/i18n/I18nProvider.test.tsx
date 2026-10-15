import { act, render, renderHook, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider, OVERRIDE_STORAGE_KEY, useI18n } from "./I18nProvider";

const wrapper = ({ children }: { children: ReactNode }) => <I18nProvider>{children}</I18nProvider>;

function setSystemLanguages(languages: string[]) {
  vi.spyOn(navigator, "languages", "get").mockReturnValue(languages);
}

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  document.documentElement.lang = "";
});

describe("I18nProvider", () => {
  it("follows the system language and sets the document language", () => {
    setSystemLanguages(["fr-FR"]);
    const { result } = renderHook(() => useI18n(), { wrapper });
    expect(result.current.language).toBe("fr");
    expect(result.current.t("app.name")).toBe("BIMCraft");
    expect(document.documentElement.lang).toBe("fr");
  });

  it("applies and remembers a manual override, and clears it", () => {
    setSystemLanguages(["en-US"]);
    const { result } = renderHook(() => useI18n(), { wrapper });
    act(() => result.current.setLanguage("fr"));
    expect(result.current.language).toBe("fr");
    expect(localStorage.getItem(OVERRIDE_STORAGE_KEY)).toBe("fr");
    act(() => result.current.setLanguage(null));
    expect(result.current.language).toBe("en");
    expect(localStorage.getItem(OVERRIDE_STORAGE_KEY)).toBeNull();
  });

  it("starts from a stored override and ignores an unsupported stored value", () => {
    setSystemLanguages(["en-US"]);
    localStorage.setItem(OVERRIDE_STORAGE_KEY, "fr");
    expect(renderHook(() => useI18n(), { wrapper }).result.current.language).toBe("fr");
    localStorage.setItem(OVERRIDE_STORAGE_KEY, "xx");
    expect(renderHook(() => useI18n(), { wrapper }).result.current.language).toBe("en");
  });

  it("keeps working when storage is unavailable", () => {
    setSystemLanguages(["en-US"]);
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const { result } = renderHook(() => useI18n(), { wrapper });
    act(() => result.current.setLanguage("fr"));
    expect(result.current.language).toBe("fr");
  });

  it("follows a change of system language when there is no override", () => {
    setSystemLanguages(["en-US"]);
    const { result } = renderHook(() => useI18n(), { wrapper });
    setSystemLanguages(["fr-FR"]);
    act(() => {
      window.dispatchEvent(new Event("languagechange"));
    });
    expect(result.current.language).toBe("fr");
  });

  it("renders translated text for its children", () => {
    function Name() {
      return <p>{useI18n().t("app.name")}</p>;
    }
    render(<Name />, { wrapper });
    expect(screen.getByText("BIMCraft")).toBeInTheDocument();
  });
});

describe("useI18n", () => {
  it("fails outside an I18nProvider", () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    expect(() => renderHook(() => useI18n())).toThrow("useI18n must be used inside <I18nProvider>");
  });
});
