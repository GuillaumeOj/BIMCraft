import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// Read from disk: Vitest replaces CSS imports with empty modules (see contrast.test.ts).
const css = (path: string) => readFileSync(join(import.meta.dirname, "..", path), "utf8");
const shell = css("shell/shell.css");
const theme = css("shell/theme.css");

/** The declarations of the rule for `selector`, or "" when there is none. */
const rule = (source: string, selector: string) =>
  new RegExp(`(?:^|\\n)${selector.replace(".", "\\.")}\\s*\\{([^}]*)\\}`).exec(source)?.[1] ?? "";
const token = (name: string) => Number(new RegExp(`--bc-z-${name}:\\s*(\\d+)`).exec(theme)?.[1]);

describe("layers of the shell", () => {
  it("puts menus and popovers above the panels, below dialogs", () => {
    expect(token("menu")).toBeGreaterThan(token("panel"));
    expect(token("menu")).toBeLessThan(20);
  });

  it("takes the panel layer from the token", () => {
    expect(rule(shell, ".panel")).toContain("z-index: var(--bc-z-panel)");
  });

  it("gives the menus of the controls and the visibility popover the menu layer", () => {
    expect(rule(css("materials/materials.css"), ".menu")).toContain("z-index: var(--bc-z-menu)");
    expect(rule(css("viewport/viewport.css"), ".vp-popover")).toContain(
      "z-index: var(--bc-z-menu)",
    );
  });

  it("keeps the 3D view out of a stacking context, which would trap its menus under the panels", () => {
    expect(rule(shell, ".viewport")).not.toContain("z-index");
  });
});
