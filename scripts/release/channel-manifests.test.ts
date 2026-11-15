import { describe, expect, spyOn, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseTag } from "./check-version";
import {
  channelManifest,
  compareTags,
  EMPTY_MANIFEST,
  type GitHubRelease,
  ghDownload,
  main,
  parseReleases,
  selectChannels,
} from "./channel-manifests";

const release = (tag_name: string, extra: Partial<GitHubRelease> = {}): GitHubRelease => ({
  tag_name,
  draft: false,
  prerelease: tag_name.includes("-beta."),
  body: `Notes of ${tag_name}`,
  ...extra,
});
const tag = (name: string) => parseTag(name) ?? (() => { throw new Error(name); })();
const latest = (version: string) => JSON.stringify({ version, pub_date: "2026-11-15T10:00:00.000Z", platforms: { "windows-x86_64": { signature: "s", url: "u" } } });

describe("compareTags", () => {
  test("orders by semver, a beta before its stable version", () => {
    const ordered = ["v0.9.0", "v0.10.0-beta.1", "v0.10.0-beta.2", "v0.10.0-beta.10", "v0.10.0", "v0.10.1-beta.1", "v1.0.0"];
    const shuffled = [...ordered].reverse().map(tag);
    expect(shuffled.sort(compareTags).map((t) => t.tag)).toEqual(ordered);
    expect(compareTags(tag("v1.0.0"), tag("v1.0.0"))).toBe(0);
  });
});

describe("selectChannels", () => {
  test("stable follows the newest stable release, beta the newest of all", () => {
    const channels = selectChannels([release("v0.9.0"), release("v1.0.0-beta.2"), release("v0.10.0"), release("v1.0.0-beta.1")]);
    expect(channels.stable?.tag_name).toBe("v0.10.0");
    expect(channels.beta?.tag_name).toBe("v1.0.0-beta.2");
  });

  test("beta is never behind stable", () => {
    const channels = selectChannels([release("v1.0.0-beta.2"), release("v1.0.0")]);
    expect(channels.beta?.tag_name).toBe("v1.0.0");
  });

  test("ignores drafts, other tags and releases flagged against their tag", () => {
    const channels = selectChannels([
      release("v0.1.0"),
      release("v2.0.0", { draft: true }),
      release("v3.0.0", { prerelease: true }),
      release("v4.0.0-beta.1", { prerelease: false }),
      release("nightly"),
    ]);
    expect(channels).toEqual({ stable: release("v0.1.0"), beta: release("v0.1.0") });
  });

  test("a channel with no release yet has none", () => {
    expect(selectChannels([])).toEqual({ stable: null, beta: null });
    expect(selectChannels([release("v0.1.0-beta.1")]).stable).toBeNull();
  });
});

describe("channelManifest", () => {
  test("is the release's latest.json with its notes", () => {
    expect(channelManifest(release("v1.0.0"), latest("1.0.0"))).toEqual({ ...JSON.parse(latest("1.0.0")), notes: "Notes of v1.0.0" });
  });

  test("leaves out empty notes", () => {
    for (const body of [null, "", "  \n"]) {
      expect(channelManifest(release("v1.0.0", { body }), latest("1.0.0"))).toEqual(JSON.parse(latest("1.0.0")));
    }
  });

  test("refuses a latest.json for another version", () => {
    expect(() => channelManifest(release("v1.0.0"), latest("0.9.0"))).toThrow("latest.json of v1.0.0 is for version 0.9.0");
  });
});

test("parseReleases reads one release per line", () => {
  expect(parseReleases(`${JSON.stringify(release("v1.0.0"))}\n\n${JSON.stringify(release("v0.9.0"))}\n`).map((r) => r.tag_name)).toEqual(["v1.0.0", "v0.9.0"]);
});

describe("ghDownload", () => {
  // A stand-in `gh` that prints its arguments, or fails for the tag "missing".
  const bin = mkdtempSync(join(tmpdir(), "fake-gh-"));
  writeFileSync(join(bin, "gh"), '#!/bin/sh\nif [ "$3" = missing ]; then echo "release not found" >&2; exit 1; fi\necho "$@"\n');
  chmodSync(join(bin, "gh"), 0o755);
  const env = { PATH: `${bin}:${process.env.PATH}` };

  test("prints the release's latest.json", () => {
    expect(ghDownload("v1.0.0", env)).toBe("release download v1.0.0 --pattern latest.json --output -\n");
  });

  test("fails with gh's message", () => {
    expect(() => ghDownload("missing", env)).toThrow("could not download latest.json of missing: release not found");
  });
});

describe("main", () => {
  const releasesFile = (releases: GitHubRelease[]) => {
    const dir = mkdtempSync(join(tmpdir(), "channels-"));
    const file = join(dir, "releases.jsonl");
    writeFileSync(file, releases.map((r) => JSON.stringify(r)).join("\n"));
    return { dir, file };
  };
  const read = (dir: string, channel: string) => JSON.parse(readFileSync(join(dir, "updates", `${channel}.json`), "utf8"));

  test("writes both channels", () => {
    const log = spyOn(console, "log").mockImplementation(() => {});
    const { dir, file } = releasesFile([release("v1.0.0"), release("v1.1.0-beta.1")]);
    expect(main([file, join(dir, "updates")], (t) => latest(t.slice(1)))).toBe(0);
    expect(read(dir, "stable").version).toBe("1.0.0");
    expect(read(dir, "beta")).toMatchObject({ version: "1.1.0-beta.1", notes: "Notes of v1.1.0-beta.1" });
    expect(log).toHaveBeenCalledWith("beta: v1.1.0-beta.1");
    log.mockRestore();
  });

  test("downloads a release once when both channels name it", () => {
    const log = spyOn(console, "log").mockImplementation(() => {});
    const { dir, file } = releasesFile([release("v1.0.0")]);
    const downloads: string[] = [];
    const download = (t: string) => {
      downloads.push(t);
      return latest(t.slice(1));
    };
    expect(main([file, join(dir, "updates")], download)).toBe(0);
    expect(downloads).toEqual(["v1.0.0"]);
    expect(read(dir, "beta").version).toBe("1.0.0");
    log.mockRestore();
  });

  test("writes a manifest that offers nothing when a channel has no release", () => {
    const log = spyOn(console, "log").mockImplementation(() => {});
    const { dir, file } = releasesFile([release("v0.1.0-beta.1")]);
    expect(main([file, join(dir, "updates")], (t) => latest(t.slice(1)))).toBe(0);
    expect(read(dir, "stable")).toEqual(EMPTY_MANIFEST);
    expect(log).toHaveBeenCalledWith("stable: no release yet");
    log.mockRestore();
  });

  test("fails on missing arguments or a failed download", () => {
    const error = spyOn(console, "error").mockImplementation(() => {});
    expect(main([])).toBe(2);
    const { dir, file } = releasesFile([release("v1.0.0")]);
    expect(main([file, join(dir, "updates")], () => { throw new Error("offline"); })).toBe(1);
    expect(error).toHaveBeenCalledWith("channel-manifests: offline");
    error.mockRestore();
  });
});
