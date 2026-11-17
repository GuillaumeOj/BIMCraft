import type { ViewportTheme } from "./types";

// The CSS tokens of theme.css each theme colour comes from, and the light value used when the
// stylesheet is not there (tests) or a token is not a #rrggbb colour.
const TOKENS: Record<keyof ViewportTheme, [token: string, fallback: string]> = {
  background: ["--bc-vp-bg", "#f4f5f6"],
  selection: ["--bc-vp-select", "#0f6b5e"],
  hover: ["--bc-vp-hover", "#b25e00"],
  snap: ["--bc-vp-snap", "#b4137b"],
  preview: ["--bc-vp-ghost", "#5c6470"],
  cut: ["--bc-vp-cut", "#b4410a"],
};

const HEX = /^#[0-9a-f]{6}$/i;

/** The viewport colours of the current theme, read from the CSS tokens on `element`. */
export function readTheme(element: Element = document.documentElement): ViewportTheme {
  const style = getComputedStyle(element);
  const colour = ([token, fallback]: [string, string]) => {
    const value = style.getPropertyValue(token).trim();
    return HEX.test(value) ? value : fallback;
  };
  const entries = Object.entries(TOKENS) as [keyof ViewportTheme, [string, string]][];
  return Object.fromEntries(entries.map(([name, token]) => [name, colour(token)])) as ViewportTheme;
}

/** Calls `onChange` whenever the system switches between light and dark; returns the stop call. */
export function watchColourScheme(onChange: () => void): () => void {
  const query = window.matchMedia?.("(prefers-color-scheme: dark)");
  if (query === undefined) return () => undefined;
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}
