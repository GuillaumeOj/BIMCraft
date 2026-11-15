import { describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  backgroundColour,
  build,
  encodeIco,
  GLASS_ICON,
  GLASS_LAYERS,
  ICONS_DIR,
  iconJson,
  keepLayers,
  layerNames,
  main,
  padForMacOS,
  pinnedTauri,
  srgb,
  type Tauri,
  tauriCli,
} from "./build-icons";

const REPO = join(import.meta.dir, "..");
const MASTER = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
<defs><mask id="m"/></defs>
<!-- layer:background -->
<rect width="1024" height="1024" fill="#0F6B5E"/>
<!-- /layer -->
<!-- layer:rebar -->
<rect id="rebar"/>
<!-- /layer -->
<!-- layer:frame -->
<rect id="frame"/>
<!-- /layer -->
<!-- layer:facade -->
<rect id="facade"/>
<!-- /layer -->
</svg>
`;

describe("layers", () => {
  test("lists the marked layers in document order", () => {
    expect(layerNames(MASTER)).toEqual(["background", "rebar", "frame", "facade"]);
  });

  test("keeps only the asked layers and the definitions", () => {
    const frame = keepLayers(MASTER, ["frame"]);
    expect(layerNames(frame)).toEqual(["frame"]);
    expect(frame).toContain('<mask id="m"/>');
    expect(frame).toContain('id="frame"');
    expect(frame).not.toContain('id="facade"');
  });

  test("the real masters mark the background and every glass layer", () => {
    for (const file of ["logo.svg", "logo-small.svg"]) {
      const svg = readFileSync(join(REPO, ICONS_DIR, "source", file), "utf8");
      expect(layerNames(svg)).toEqual(["background", ...[...GLASS_LAYERS].reverse()]);
    }
  });
});

describe("backgroundColour", () => {
  test("reads the fill of the background layer", () => {
    expect(backgroundColour(MASTER)).toBe("#0F6B5E");
  });

  test("rejects a master without a plain background fill", () => {
    expect(() => backgroundColour(keepLayers(MASTER, ["frame"]))).toThrow("background layer");
  });
});

describe("padForMacOS", () => {
  test("shrinks the drawing onto the 824 px body of the macOS grid", () => {
    const padded = padForMacOS(MASTER);
    expect(padded.startsWith('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><g transform="translate(100 100) scale(0.8046875)">')).toBe(true);
    expect(padded.trimEnd().endsWith("</g></svg>")).toBe(true);
    expect(padded).toContain('id="facade"');
  });

  test("rejects anything but a single svg element", () => {
    expect(() => padForMacOS("<g/>")).toThrow("single <svg>");
  });
});

describe("encodeIco", () => {
  test("writes a header, one entry per image and the PNG data", () => {
    const ico = encodeIco([
      { size: 16, png: new Uint8Array([1, 2, 3]) },
      { size: 256, png: new Uint8Array([4, 5]) },
    ]);
    const view = new DataView(ico.buffer);
    expect([view.getUint16(0, true), view.getUint16(2, true), view.getUint16(4, true)]).toEqual([0, 1, 2]);
    expect([ico[6], ico[7], view.getUint16(10, true), view.getUint16(12, true)]).toEqual([16, 16, 1, 32]);
    expect([view.getUint32(14, true), view.getUint32(18, true)]).toEqual([3, 38]);
    expect([ico[22], ico[23], view.getUint32(30, true), view.getUint32(34, true)]).toEqual([0, 0, 2, 41]);
    expect([...ico.slice(38)]).toEqual([1, 2, 3, 4, 5]);
  });
});

describe("Icon Composer document", () => {
  test("writes colours as srgb channels", () => {
    expect(srgb("#0F6B5E")).toBe("srgb:0.05882,0.41961,0.36863,1.00000");
    expect(srgb("#FFFFFF")).toBe("srgb:1.00000,1.00000,1.00000,1.00000");
  });

  test("has the solid fill and one glass group per layer, front first", () => {
    const document = JSON.parse(iconJson("#0F6B5E", ["facade", "rebar"]));
    expect(document.fill).toEqual({ solid: "srgb:0.05882,0.41961,0.36863,1.00000" });
    expect(document.groups.map((group: { layers: { name: string }[] }) => group.layers[0]?.name)).toEqual(["facade", "rebar"]);
    expect(document.groups[0].layers[0]["image-name"]).toBe("facade.png");
    expect(document.groups[0].translucency.enabled).toBe(true);
    expect(document["supported-platforms"]).toEqual({ squares: ["macOS"] });
  });
});

describe("tauriCli", () => {
  test("runs the command with the arguments", () => {
    expect(() => tauriCli(process.execPath, ["-e"])(["process.exit(0)"])).not.toThrow();
  });

  test("throws with the command's error output when it fails", () => {
    expect(() => tauriCli(process.execPath, ["-e"])(["console.error('no input'); process.exit(3)"])).toThrow(
      "failed: no input",
    );
  });

  test("the pinned CLI lives in ui/node_modules", () => {
    expect(() => pinnedTauri(join(tmpdir(), "no-such-repo"))(["--version"])).toThrow();
  });
});

// Stands in for `tauri icon`: writes small fake files where the real CLI writes images.
const fakeTauri: Tauri = (args) => {
  const [command, source, flag, out, png, sizes] = args;
  expect([command, flag]).toEqual(["icon", "-o"]);
  expect(readFileSync(source ?? "", "utf8")).toContain("<svg");
  if (png === "--png") {
    for (const size of (sizes ?? "").split(",")) writeFileSync(join(out ?? "", `${size}x${size}.png`), `png ${size}`);
  } else {
    writeFileSync(join(out ?? "", "icon.icns"), "icns");
  }
};

function fakeRepo(): string {
  const root = mkdtempSync(join(tmpdir(), "build-icons-"));
  mkdirSync(join(root, ICONS_DIR, "source"), { recursive: true });
  writeFileSync(join(root, ICONS_DIR, "source", "logo.svg"), MASTER);
  writeFileSync(join(root, ICONS_DIR, "source", "logo-small.svg"), MASTER);
  return root;
}

describe("build", () => {
  test("writes every platform icon and the glass icon", () => {
    const root = fakeRepo();
    const written = build(root, fakeTauri);
    const icons = join(root, ICONS_DIR);
    expect(written).toContain(`${ICONS_DIR}/icon.ico`);
    for (const file of written) expect(existsSync(join(root, file))).toBe(true);
    expect(readFileSync(join(icons, "32x32.png"), "utf8")).toBe("png 32");
    expect(readFileSync(join(icons, "128x128@2x.png"), "utf8")).toBe("png 256");
    expect(readFileSync(join(icons, "icon.icns"), "utf8")).toBe("icns");
    const ico = readFileSync(join(icons, "icon.ico"));
    expect(ico.readUInt16LE(4)).toBe(6);
    for (const name of GLASS_LAYERS) {
      expect(readFileSync(join(icons, GLASS_ICON, "Assets", `${name}.png`), "utf8")).toBe("png 1024");
    }
    expect(JSON.parse(readFileSync(join(icons, GLASS_ICON, "icon.json"), "utf8")).groups).toHaveLength(3);
  });

  test("main logs each written file", () => {
    const root = fakeRepo();
    const lines: string[] = [];
    main([root], (line) => lines.push(line), () => fakeTauri);
    expect(lines).toContain(`wrote ${ICONS_DIR}/${GLASS_ICON}/icon.json`);
  });
});
