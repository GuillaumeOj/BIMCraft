import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { contrastRatio, parseThemes } from "./contrast";

// Read from disk: Vitest replaces CSS imports, even ?raw ones, with empty modules.
const themeCss = readFileSync(join(import.meta.dirname, "theme.css"), "utf8");

// Text and the surface it sits on, by token name (WCAG 2.2 AA: 4.5:1 for text).
const TEXT_PAIRS = [
  ["ink", "surface"],
  ["ink", "raised"],
  ["ink", "paper"],
  ["ink-2", "surface"],
  ["ink-2", "raised"],
  ["muted", "surface"],
  ["muted", "raised"],
  ["ink-2", "hover"],
  ["on-verdigris", "verdigris"],
] as const;

// Interface parts against their surroundings (WCAG 2.2 AA: 3:1): the focus outline.
const NON_TEXT_PAIRS = [
  ["verdigris", "surface"],
  ["verdigris", "paper"],
] as const;

describe("contrastRatio", () => {
  it("is 21 for black on white and 1 for equal colours, in either order", () => {
    expect(contrastRatio("#000000", "#FFFFFF")).toBeCloseTo(21, 5);
    expect(contrastRatio("#ffffff", "#000000")).toBeCloseTo(21, 5);
    expect(contrastRatio("#777777", "#777777")).toBe(1);
  });

  it("matches a published value", () => {
    // #767676 on white is the classic 4.54:1 grey.
    expect(contrastRatio("#767676", "#ffffff")).toBeCloseTo(4.54, 2);
  });

  it("rejects colours that are not six-digit hex", () => {
    expect(() => contrastRatio("red", "#ffffff")).toThrow('not a #rrggbb colour: "red"');
  });
});

describe("parseThemes", () => {
  it("reads the light tokens and the dark overrides", () => {
    const css =
      ":root { --bc-ink: #111111; --bc-paper: #eeeeee; }\n@media (prefers-color-scheme: dark) { :root { --bc-ink: #eeeeee; } }";
    expect(parseThemes(css)).toEqual({
      light: { ink: "#111111", paper: "#eeeeee" },
      dark: { ink: "#eeeeee" },
    });
  });

  it("returns no dark tokens when the stylesheet has no dark theme", () => {
    expect(parseThemes(":root { --bc-ink: #111111; }")).toEqual({
      light: { ink: "#111111" },
      dark: {},
    });
  });
});

describe("theme tokens", () => {
  const themes = parseThemes(themeCss);

  it("defines every light token again for the dark theme", () => {
    expect(Object.keys(themes.dark).sort()).toEqual(Object.keys(themes.light).sort());
  });

  for (const theme of ["light", "dark"] as const) {
    it.each(TEXT_PAIRS)(`${theme}: %s on %s meets WCAG AA`, (text, background) => {
      const foreground = themes[theme][text];
      const surface = themes[theme][background];
      expect(foreground).toBeDefined();
      expect(surface).toBeDefined();
      expect(contrastRatio(foreground ?? "", surface ?? "")).toBeGreaterThanOrEqual(4.5);
    });

    it.each(NON_TEXT_PAIRS)(
      `${theme}: %s against %s meets WCAG AA for interface parts`,
      (part, background) => {
        const colour = themes[theme][part];
        const surface = themes[theme][background];
        expect(colour).toBeDefined();
        expect(surface).toBeDefined();
        expect(contrastRatio(colour ?? "", surface ?? "")).toBeGreaterThanOrEqual(3);
      },
    );
  }
});
