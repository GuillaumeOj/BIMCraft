#!/usr/bin/env bun
// Writes the update channels' manifests from the published releases (ENG-05): stable.json is the
// latest.json of the newest stable release, beta.json that of the newest release of all, stable or
// beta, so the beta channel is never behind stable. Each gets the release's notes. A channel with
// no release yet gets a manifest that offers nothing. Run on every release event, so unpublishing
// or deleting a release rolls its channel back.
//
// Usage: GH_REPO=owner/name bun scripts/release/channel-manifests.ts <releases> <out-dir>
//   <releases>: one JSON release per line, as printed by
//   `gh api --paginate repos/OWNER/NAME/releases --jq '.[] | {tag_name, draft, prerelease, body}'`.
//   Each chosen release's latest.json is downloaded with `gh release download`.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseTag, type Release as Tag } from "./check-version";

/** A release as the GitHub API lists it. */
export type GitHubRelease = { tag_name: string; draft: boolean; prerelease: boolean; body?: string | null };

export type Channels = { stable: GitHubRelease | null; beta: GitHubRelease | null };

/** A manifest that offers no update: its version is lower than any program's. */
export const EMPTY_MANIFEST = { version: "0.0.0", platforms: {} };

const numbers = (version: string): number[] => version.split(/[.-]/).flatMap((part) => (/^\d+$/.test(part) ? [Number(part)] : []));

/** Semver order of two release tags: a beta comes before its stable version. */
export function compareTags(a: Tag, b: Tag): number {
  const [x, y] = [numbers(a.version), numbers(b.version)];
  for (let i = 0; i < 3; i += 1) {
    const difference = (x[i] ?? 0) - (y[i] ?? 0);
    if (difference !== 0) return difference;
  }
  // A stable version has no beta number: it sorts after every beta of itself.
  const [betaA, betaB] = [x[3] ?? Number.POSITIVE_INFINITY, y[3] ?? Number.POSITIVE_INFINITY];
  return betaA === betaB ? 0 : betaA < betaB ? -1 : 1;
}

/**
 * The newest published stable release and the newest published release of all. Drafts, tags that
 * are not release tags and releases whose pre-release flag disagrees with their tag are left out.
 */
export function selectChannels(releases: GitHubRelease[]): Channels {
  const published = releases.flatMap((release) => {
    const tag = parseTag(release.tag_name);
    return !release.draft && tag !== null && tag.prerelease === release.prerelease ? [{ release, tag }] : [];
  });
  published.sort((a, b) => compareTags(b.tag, a.tag));
  return {
    stable: published.find(({ tag }) => !tag.prerelease)?.release ?? null,
    beta: published[0]?.release ?? null,
  };
}

/** The channel manifest of `release`: its latest.json with the release notes. */
export function channelManifest(release: GitHubRelease, latest: string): Record<string, unknown> {
  const manifest = JSON.parse(latest) as Record<string, unknown>;
  const version = parseTag(release.tag_name)?.version;
  if (manifest.version !== version) {
    throw new Error(`latest.json of ${release.tag_name} is for version ${String(manifest.version)}`);
  }
  const notes = release.body?.trim();
  return notes ? { ...manifest, notes } : manifest;
}

export const parseReleases = (lines: string): GitHubRelease[] =>
  lines
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => JSON.parse(line) as GitHubRelease);

/** Downloads a release's latest.json with the GitHub CLI. */
export function ghDownload(tag: string, env: Record<string, string | undefined> = process.env): string {
  const result = Bun.spawnSync(["gh", "release", "download", tag, "--pattern", "latest.json", "--output", "-"], { env });
  if (result.exitCode !== 0) throw new Error(`could not download latest.json of ${tag}: ${result.stderr.toString().trim()}`);
  return result.stdout.toString();
}

export function main(args: string[], download: (tag: string) => string = ghDownload): number {
  const [releases, out] = args;
  if (!releases || !out) {
    console.error("usage: bun scripts/release/channel-manifests.ts <releases> <out-dir>");
    return 2;
  }
  try {
    const channels = selectChannels(parseReleases(readFileSync(releases, "utf8")));
    mkdirSync(out, { recursive: true });
    for (const [channel, release] of Object.entries(channels)) {
      const manifest = release === null ? EMPTY_MANIFEST : channelManifest(release, download(release.tag_name));
      writeFileSync(join(out, `${channel}.json`), `${JSON.stringify(manifest, null, 2)}\n`);
      console.log(`${channel}: ${release?.tag_name ?? "no release yet"}`);
    }
    return 0;
  } catch (error) {
    console.error(`channel-manifests: ${(error as Error).message}`);
    return 1;
  }
}

if (import.meta.main) process.exit(main(process.argv.slice(2)));
