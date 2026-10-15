const HEX = /^#[0-9a-f]{6}$/i;
const DARK_THEME = "@media (prefers-color-scheme: dark)";

/** Linear value of one 0–255 sRGB channel, as defined by WCAG 2.2. */
function linear(channel: number): number {
  const c = (channel & 0xff) / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** Relative luminance of a `#rrggbb` colour. */
function luminance(colour: string): number {
  if (!HEX.test(colour)) throw new Error(`not a #rrggbb colour: "${colour}"`);
  const rgb = Number.parseInt(colour.slice(1), 16);
  return 0.2126 * linear(rgb >> 16) + 0.7152 * linear(rgb >> 8) + 0.0722 * linear(rgb);
}

/** WCAG contrast ratio between two `#rrggbb` colours, from 1 to 21. */
export function contrastRatio(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

export type Themes = { light: Record<string, string>; dark: Record<string, string> };

const tokens = (css: string) =>
  Object.fromEntries(
    [...css.matchAll(/--bc-([\w-]+):\s*(#[0-9a-f]{6})/gi)].map((m) => [m[1], m[2]]),
  );

/** Reads the `--bc-*` colour tokens of theme.css: the light ones, then the dark overrides. */
export function parseThemes(css: string): Themes {
  const at = css.indexOf(DARK_THEME);
  const split = at === -1 ? css.length : at;
  return { light: tokens(css.slice(0, split)), dark: tokens(css.slice(split)) };
}
