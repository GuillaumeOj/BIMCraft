import { describe, expect, it } from "vitest";
import { type Messages, resolveLanguage, translateWith } from "./i18n";
import en from "./messages/en.json";
import fr from "./messages/fr.json";

describe("message files", () => {
  it("has a French translation for every English key, and no extra key", () => {
    // On failure, the diff shows the keys to translate or to delete.
    expect(Object.keys(fr).sort()).toEqual(Object.keys(en).sort());
  });
});

describe("translateWith", () => {
  const english: Messages = { greeting: "Hello {name}", plain: "Plain" };

  it("returns the message of the requested language", () => {
    expect(
      translateWith({ greeting: "Bonjour {name}" }, english, "greeting", { name: "Ada" }),
    ).toBe("Bonjour Ada");
  });

  it("falls back to English when the translation lacks the key", () => {
    expect(translateWith({}, english, "plain")).toBe("Plain");
  });

  it("falls back to the key itself when no file has it", () => {
    expect(translateWith({}, english, "unknown.key")).toBe("unknown.key");
  });

  it("ignores inherited properties of the message files and parameters", () => {
    expect(translateWith({}, { m: "{constructor}" }, "m")).toBe("{constructor}");
    expect(translateWith({}, {}, "toString")).toBe("toString");
  });

  it("fills every occurrence of a parameter and leaves unknown placeholders", () => {
    expect(translateWith({ m: "{a}-{a}-{b}" }, english, "m", { a: 1 })).toBe("1-1-{b}");
  });
});

describe("resolveLanguage", () => {
  it("prefers the manual override", () => {
    expect(resolveLanguage("fr", ["en-GB"])).toBe("fr");
  });

  it("follows the first supported system language, ignoring region and case", () => {
    expect(resolveLanguage(null, ["de-DE", "FR-ca", "en"])).toBe("fr");
  });

  it("uses English when no system language is supported", () => {
    expect(resolveLanguage(null, ["de", "it"])).toBe("en");
    expect(resolveLanguage(null, [])).toBe("en");
  });
});
