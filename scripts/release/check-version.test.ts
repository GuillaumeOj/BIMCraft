import { describe, expect, spyOn, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { cargoVersion, checkPublished, checkTag, MANIFEST, main, parseTag } from "./check-version";

describe("parseTag", () => {
  test("reads stable and beta tags", () => {
    expect(parseTag("v1.2.3")).toEqual({ tag: "v1.2.3", version: "1.2.3", prerelease: false });
    expect(parseTag("v0.10.0-beta.2")).toEqual({ tag: "v0.10.0-beta.2", version: "0.10.0-beta.2", prerelease: true });
  });

  test("refuses every other shape", () => {
    for (const tag of ["1.2.3", "v1.2", "v1.2.3.4", "v01.2.3", "v1.2.3-rc.1", "v1.2.3-beta", "v1.2.3-beta.01", "v1.2.3 ", "vx.y.z"]) {
      expect(parseTag(tag)).toBeNull();
    }
  });
});

describe("cargoVersion", () => {
  test("reads the package version", () => {
    expect(cargoVersion('[package]\nname = "desktop-app"\nversion = "0.3.0"\n')).toBe("0.3.0");
  });

  test("fails without one", () => {
    expect(() => cargoVersion('[package]\nname = "desktop-app"\n')).toThrow("no [package] version");
    expect(() => cargoVersion("[workspace]\n")).toThrow("no [package] version");
  });
});

describe("checkTag", () => {
  test("accepts a tag that names the version", () => {
    expect(checkTag("v0.4.0-beta.1", "0.4.0-beta.1").prerelease).toBe(true);
  });

  test("refuses a malformed tag or another version", () => {
    expect(() => checkTag("v0.4", "0.4.0")).toThrow("not a release tag");
    expect(() => checkTag("v0.4.1", "0.4.0")).toThrow('does not match version "0.4.0"');
    expect(() => checkTag("v0.4.0-beta.1", "0.4.0")).toThrow("does not match");
  });
});

describe("checkPublished", () => {
  test("a beta must be a pre-release and a stable release must not", () => {
    expect(checkPublished("v1.0.0-beta.3", true).version).toBe("1.0.0-beta.3");
    expect(checkPublished("v1.0.0", false).version).toBe("1.0.0");
    expect(() => checkPublished("v1.0.0-beta.3", false)).toThrow("must be a pre-release");
    expect(() => checkPublished("v1.0.0", true)).toThrow("must not be a pre-release");
    expect(() => checkPublished("nightly", false)).toThrow("not a release tag");
  });
});

describe("main", () => {
  const repo = (version: string) => {
    const root = mkdtempSync(join(tmpdir(), "check-version-"));
    mkdirSync(join(root, "crates/desktop-app"), { recursive: true });
    writeFileSync(join(root, MANIFEST), `[package]\nname = "desktop-app"\nversion = "${version}"\n`);
    return root;
  };
  const quiet = () => [spyOn(console, "log").mockImplementation(() => {}), spyOn(console, "error").mockImplementation(() => {})];

  test("writes the release to the step outputs", () => {
    const [log, error] = quiet();
    const root = repo("0.4.0-beta.1");
    const output = join(root, "out");
    expect(main([], { TAG: "v0.4.0-beta.1", GITHUB_OUTPUT: output }, root)).toBe(0);
    expect(readFileSync(output, "utf8")).toBe("tag=v0.4.0-beta.1\nversion=0.4.0-beta.1\nprerelease=true\n");
    expect(log).toHaveBeenCalledWith("Tag v0.4.0-beta.1 matches version 0.4.0-beta.1 (beta).");
    log?.mockRestore();
    error?.mockRestore();
  });

  test("a dry run takes the tag the version would have", () => {
    const [log, error] = quiet();
    expect(main([], { TAG: "" }, repo("1.0.0"))).toBe(0);
    expect(log).toHaveBeenCalledWith("Tag v1.0.0 matches version 1.0.0 (stable).");
    log?.mockRestore();
    error?.mockRestore();
  });

  test("fails on a tag that does not match", () => {
    const [log, error] = quiet();
    expect(main([], { TAG: "v1.0.1" }, repo("1.0.0"))).toBe(1);
    expect(error).toHaveBeenCalledWith(expect.stringContaining('tag "v1.0.1" does not match'));
    log?.mockRestore();
    error?.mockRestore();
  });

  test("checks a published release's flag", () => {
    const [log, error] = quiet();
    expect(main(["--published"], { TAG: "v1.0.0", PRERELEASE: "false" }, "/nowhere")).toBe(0);
    expect(log).toHaveBeenCalledWith("Release v1.0.0 is correctly not a pre-release.");
    expect(main(["--published"], { TAG: "v1.0.0-beta.1", PRERELEASE: "true" }, "/nowhere")).toBe(0);
    expect(log).toHaveBeenCalledWith("Release v1.0.0-beta.1 is correctly a pre-release.");
    expect(main(["--published"], { TAG: "v1.0.0", PRERELEASE: "true" }, "/nowhere")).toBe(1);
    expect(main(["--published"], { TAG: "v1.0.0" }, "/nowhere")).toBe(1);
    expect(error).toHaveBeenCalledWith("check-version: PRERELEASE must be true or false");
    expect(main(["--published"], { PRERELEASE: "false" }, "/nowhere")).toBe(1);
    log?.mockRestore();
    error?.mockRestore();
  });
});
