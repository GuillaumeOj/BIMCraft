import { describe, expect, spyOn, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { assetUrl, buildManifest, findPackage, main } from "./update-manifest";

const NOW = new Date("2026-11-15T10:00:00Z");
const MAC = "BIMCraft_universal.app.tar.gz";
const WIN = "BIMCraft_0.4.0-beta.1_x64-setup.exe";

describe("buildManifest", () => {
  const input = {
    version: "0.4.0-beta.1",
    tag: "v0.4.0-beta.1",
    repo: "GuillaumeOj/BIMCraft",
    pubDate: NOW,
    macos: { file: MAC, signature: "bWFj\n" },
    windows: { file: WIN, signature: "d2lu" },
  };

  test("lists the universal macOS package for both architectures and the NSIS installer for Windows", () => {
    const base = "https://github.com/GuillaumeOj/BIMCraft/releases/download/v0.4.0-beta.1";
    expect(buildManifest(input)).toEqual({
      version: "0.4.0-beta.1",
      pub_date: "2026-11-15T10:00:00.000Z",
      platforms: {
        "darwin-aarch64": { signature: "bWFj", url: `${base}/${MAC}` },
        "darwin-x86_64": { signature: "bWFj", url: `${base}/${MAC}` },
        "windows-x86_64": { signature: "d2lu", url: `${base}/${WIN}` },
      },
    });
  });

  test("refuses an empty signature", () => {
    expect(() => buildManifest({ ...input, windows: { file: WIN, signature: " \n" } })).toThrow(
      `the signature of ${WIN} is empty`,
    );
  });

  test("encodes the file names in the addresses", () => {
    expect(assetUrl("o/r", "v1.0.0", "BIMCraft 1.app.tar.gz")).toBe(
      "https://github.com/o/r/releases/download/v1.0.0/BIMCraft%201.app.tar.gz",
    );
  });
});

describe("findPackage", () => {
  const dir = mkdtempSync(join(tmpdir(), "update-manifest-"));
  writeFileSync(join(dir, `${MAC}.sig`), "sig");

  test("finds the single package and reads its signature", () => {
    expect(findPackage(dir, [MAC, `${MAC}.sig`, "BIMCraft.dmg"], ".app.tar.gz")).toEqual({ file: MAC, signature: "sig" });
  });

  test("fails when the package is missing, repeated or unsigned", () => {
    expect(() => findPackage(dir, [], ".app.tar.gz")).toThrow("expected one *.app.tar.gz");
    expect(() => findPackage(dir, [MAC, `x${MAC}`], ".app.tar.gz")).toThrow("found 2");
    expect(() => findPackage(dir, [MAC], ".app.tar.gz")).toThrow(`${MAC} has no ${MAC}.sig`);
  });
});

describe("main", () => {
  const env = { VERSION: "0.4.0-beta.1", TAG: "v0.4.0-beta.1", REPO: "o/r" };

  test("writes latest.json from a folder of packages", () => {
    const log = spyOn(console, "log").mockImplementation(() => {});
    const dir = mkdtempSync(join(tmpdir(), "update-manifest-"));
    for (const [file, content] of [[MAC, "m"], [`${MAC}.sig`, "c2ln"], [WIN, "w"], [`${WIN}.sig`, "c2lnMg=="]]) {
      writeFileSync(join(dir, file as string), content as string);
    }
    const out = join(dir, "latest.json");
    expect(main([dir, out], env, NOW)).toBe(0);
    const manifest = JSON.parse(readFileSync(out, "utf8"));
    expect(manifest.version).toBe("0.4.0-beta.1");
    expect(manifest.platforms["windows-x86_64"]).toEqual({ signature: "c2lnMg==", url: `https://github.com/o/r/releases/download/v0.4.0-beta.1/${WIN}` });
    expect(log).toHaveBeenCalledWith(`Wrote ${out} for 0.4.0-beta.1: darwin-aarch64, darwin-x86_64, windows-x86_64`);
    log.mockRestore();
  });

  test("fails on missing arguments or packages", () => {
    const error = spyOn(console, "error").mockImplementation(() => {});
    expect(main([], env, NOW)).toBe(2);
    expect(main(["dir", "out"], { ...env, REPO: "" }, NOW)).toBe(2);
    const empty = mkdtempSync(join(tmpdir(), "update-manifest-"));
    expect(main([empty, join(empty, "latest.json")], env, NOW)).toBe(1);
    expect(error).toHaveBeenCalledWith(`update-manifest: expected one *.app.tar.gz in ${empty}, found 0`);
    error.mockRestore();
  });
});
