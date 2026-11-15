#!/usr/bin/env bun
// Builds the BIMCraft app icons for every platform from the two SVG masters in
// crates/desktop-app/icons/source: logo.svg (48 px and up) and logo-small.svg (16 to 32 px, where
// the full drawing turns to noise). It writes the PNG images (Linux, window icon), icon.ico (Windows),
// icon.icns (macOS before 26) and BIMCraft.icon, the layered Icon Composer file that macOS 26
// renders in Liquid Glass. The pinned Tauri CLI rasterises the SVGs, so run `bun install` in ui/
// first. The outputs are committed; run this again after changing a master.
// Usage: bun scripts/build-icons.ts [repo-root]

import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const ICONS_DIR = "crates/desktop-app/icons";
export const GLASS_ICON = "BIMCraft.icon";
// Icon Composer layers, front to back. The background layer becomes the file's solid fill, so the
// system can tint it in the dark and tinted appearances.
export const GLASS_LAYERS = ["facade", "frame", "rebar"];
const SMALL_SIZES = [16, 24, 32];
const FULL_SIZES = [48, 64, 128, 256, 512, 1024];
const ICO_SMALL = [16, 24, 32];
const ICO_FULL = [48, 64, 256];
// Before macOS 26 an icon carries its own margin: the body is 824 of 1024 px, centred.
const MAC_MARGIN = 100;
const MAC_SCALE = 824 / 1024;
const LAYER = /<!-- layer:([a-z]+) -->[\s\S]*?<!-- \/layer -->\n?/g;
const ROOT = /^(<svg\b[^>]*>)([\s\S]*)(<\/svg>\s*)$/;

/** Runs the Tauri CLI with the given arguments; throws when it fails. */
export type Tauri = (args: string[]) => void;

/** Names of the layers marked in an SVG master, in document order. */
export function layerNames(svg: string): string[] {
  return [...svg.matchAll(LAYER)].map((match) => match[1] ?? "");
}

/** The master with every marked layer removed except `keep`; definitions stay. */
export function keepLayers(svg: string, keep: string[]): string {
  return svg.replace(LAYER, (section, name: string) => (keep.includes(name) ? section : ""));
}

/** The colour of the background layer, which must be a single `#RRGGBB` fill. */
export function backgroundColour(svg: string): string {
  const background = svg.match(/<!-- layer:background -->([\s\S]*?)<!-- \/layer -->/);
  const fill = background?.[1]?.match(/fill="(#[0-9A-Fa-f]{6})"/);
  if (!fill?.[1]) throw new Error("the master has no background layer with a #RRGGBB fill");
  return fill[1];
}

/** The master shrunk onto the pre-26 macOS icon grid, with a transparent margin. */
export function padForMacOS(svg: string): string {
  const parts = svg.match(ROOT);
  if (!parts) throw new Error("the master is not a single <svg> element");
  const transform = `translate(${MAC_MARGIN} ${MAC_MARGIN}) scale(${MAC_SCALE})`;
  return `${parts[1]}<g transform="${transform}">${parts[2]}</g>${parts[3]}`;
}

/** A Windows .ico holding PNG images (Windows Vista and later read them). */
export function encodeIco(images: { size: number; png: Uint8Array }[]): Uint8Array {
  const headerSize = 6 + 16 * images.length;
  const total = headerSize + images.reduce((sum, image) => sum + image.png.length, 0);
  const bytes = new Uint8Array(total);
  const view = new DataView(bytes.buffer);
  view.setUint16(2, 1, true);
  view.setUint16(4, images.length, true);
  let offset = headerSize;
  images.forEach((image, index) => {
    const entry = 6 + 16 * index;
    const side = image.size >= 256 ? 0 : image.size;
    view.setUint8(entry, side);
    view.setUint8(entry + 1, side);
    view.setUint16(entry + 4, 1, true);
    view.setUint16(entry + 6, 32, true);
    view.setUint32(entry + 8, image.png.length, true);
    view.setUint32(entry + 12, offset, true);
    bytes.set(image.png, offset);
    offset += image.png.length;
  });
  return bytes;
}

/** `#0F6B5E` as Icon Composer writes colours: `srgb:r,g,b,a` with five decimals. */
export function srgb(hex: string): string {
  const channels = [1, 3, 5].map((start) => Number.parseInt(hex.slice(start, start + 2), 16) / 255);
  return `srgb:${[...channels, 1].map((value) => value.toFixed(5)).join(",")}`;
}

/** The Icon Composer document: a solid fill and one glass group per layer image, front first. */
export function iconJson(fill: string, layers: string[]): string {
  const groups = layers.map((name) => ({
    layers: [{ "image-name": `${name}.png`, name }],
    shadow: { kind: "neutral", opacity: 0.5 },
    translucency: { enabled: true, value: 0.5 },
  }));
  const document = {
    fill: { solid: srgb(fill) },
    groups,
    "supported-platforms": { squares: ["macOS"] },
  };
  return `${JSON.stringify(document, null, 2)}\n`;
}

/** Runs the Tauri CLI at `bin`. */
export function tauriCli(bin: string, prefix: string[] = []): Tauri {
  return (args) => {
    const result = Bun.spawnSync([bin, ...prefix, ...args], { stdout: "ignore", stderr: "pipe" });
    if (result.exitCode !== 0) {
      throw new Error(`tauri ${args.join(" ")} failed: ${result.stderr.toString().trim()}`);
    }
  };
}

/** The pinned Tauri CLI installed by `bun install` in ui/. */
export function pinnedTauri(root: string): Tauri {
  const name = process.platform === "win32" ? "tauri.exe" : "tauri";
  return tauriCli(join(root, "ui", "node_modules", ".bin", name));
}

/** Builds every icon file into the icons folder of `root`; returns their paths, relative to it. */
export function build(root: string, tauri: Tauri): string[] {
  const source = join(root, ICONS_DIR, "source");
  const full = readFileSync(join(source, "logo.svg"), "utf8");
  const small = readFileSync(join(source, "logo-small.svg"), "utf8");
  const work = mkdtempSync(join(tmpdir(), "bimcraft-icons-"));
  try {
    const render = (name: string, svg: string, sizes?: number[]) => {
      const dir = join(work, name);
      mkdirSync(dir);
      const file = join(dir, "source.svg");
      writeFileSync(file, svg);
      tauri(["icon", file, "-o", dir, ...(sizes ? ["--png", sizes.join(",")] : [])]);
      return (output: string) => new Uint8Array(readFileSync(join(dir, output)));
    };
    const png = (read: (output: string) => Uint8Array, size: number) => read(`${size}x${size}.png`);
    const big = render("full", full, FULL_SIZES);
    const little = render("small", small, SMALL_SIZES);
    const mac = render("macos", padForMacOS(full));
    const outputs = new Map<string, Uint8Array | string>([
      ["32x32.png", png(little, 32)],
      ["128x128.png", png(big, 128)],
      ["128x128@2x.png", png(big, 256)],
      ["icon.png", png(big, 512)],
      ["icon.icns", mac("icon.icns")],
      [
        "icon.ico",
        encodeIco([
          ...ICO_SMALL.map((size) => ({ size, png: png(little, size) })),
          ...ICO_FULL.map((size) => ({ size, png: png(big, size) })),
        ]),
      ],
      [`${GLASS_ICON}/icon.json`, iconJson(backgroundColour(full), GLASS_LAYERS)],
    ]);
    for (const name of GLASS_LAYERS) {
      outputs.set(`${GLASS_ICON}/Assets/${name}.png`, png(render(`layer-${name}`, keepLayers(full, [name]), [1024]), 1024));
    }
    const target = join(root, ICONS_DIR);
    mkdirSync(join(target, GLASS_ICON, "Assets"), { recursive: true });
    for (const [name, content] of outputs) writeFileSync(join(target, name), content);
    return [...outputs.keys()].map((name) => `${ICONS_DIR}/${name}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

export function main(args: string[], log = console.log, tauriFor = pinnedTauri): void {
  const root = args[0] ?? join(import.meta.dir, "..");
  for (const file of build(root, tauriFor(root))) log(`wrote ${file}`);
}

if (import.meta.main) main(process.argv.slice(2));
