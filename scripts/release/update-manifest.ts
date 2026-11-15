#!/usr/bin/env bun
// Writes latest.json, the Tauri static update manifest of one release (ENG-05): for each platform,
// the address of its update package on the release page and the package's signature.
//
// Usage: VERSION=1.2.0 TAG=v1.2.0 REPO=owner/name bun scripts/release/update-manifest.ts <dir> <out>
//   <dir> holds the update packages (the macOS *.app.tar.gz and the Windows *-setup.exe) and their
//   .sig files, as they are attached to the release.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** An update package as attached to the release: its file name and its signature. */
export type Package = { file: string; signature: string };

/** One platform's entry in the manifest. */
export type Platform = { signature: string; url: string };

/** The manifest the updater reads (`notes` are added per channel at publication). */
export type Manifest = { version: string; pub_date: string; platforms: Record<string, Platform> };

export type ManifestInput = {
  version: string;
  tag: string;
  repo: string;
  pubDate: Date;
  macos: Package;
  windows: Package;
};

/**
 * The updater looks a platform up by `{os}-{arch}`: the universal macOS package serves both
 * architectures. Windows has only the generic key, which the program reads because the release
 * cannot write its bundle type into the signed program; it names the per-user NSIS installer, so
 * MSI installations do not update themselves (docs/dev/release.md).
 */
const PLATFORMS = { "darwin-aarch64": "macos", "darwin-x86_64": "macos", "windows-x86_64": "windows" } as const;

export const assetUrl = (repo: string, tag: string, file: string): string =>
  `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(file)}`;

export function buildManifest(input: ManifestInput): Manifest {
  const platforms = Object.fromEntries(
    Object.entries(PLATFORMS).map(([platform, os]) => {
      const { file, signature } = input[os];
      if (signature.trim() === "") throw new Error(`the signature of ${file} is empty`);
      return [platform, { signature: signature.trim(), url: assetUrl(input.repo, input.tag, file) }];
    }),
  );
  return { version: input.version, pub_date: input.pubDate.toISOString(), platforms };
}

/** The one package of `files` whose name ends with `suffix`, with its signature from `dir`. */
export function findPackage(dir: string, files: string[], suffix: string): Package {
  const found = files.filter((file) => file.endsWith(suffix));
  if (found.length !== 1) throw new Error(`expected one *${suffix} in ${dir}, found ${found.length}`);
  const [file] = found as [string];
  if (!files.includes(`${file}.sig`)) throw new Error(`${file} has no ${file}.sig`);
  return { file, signature: readFileSync(join(dir, `${file}.sig`), "utf8") };
}

type Env = Record<string, string | undefined>;

export function main(args: string[], env: Env, now: Date): number {
  const [dir, out] = args;
  const { VERSION: version, TAG: tag, REPO: repo } = env;
  if (!dir || !out || !version || !tag || !repo) {
    console.error("usage: VERSION=… TAG=… REPO=… bun scripts/release/update-manifest.ts <dir> <out>");
    return 2;
  }
  try {
    const files = readdirSync(dir);
    const manifest = buildManifest({
      version,
      tag,
      repo,
      pubDate: now,
      macos: findPackage(dir, files, ".app.tar.gz"),
      windows: findPackage(dir, files, "-setup.exe"),
    });
    writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
    console.log(`Wrote ${out} for ${version}: ${Object.keys(manifest.platforms).join(", ")}`);
    return 0;
  } catch (error) {
    console.error(`update-manifest: ${(error as Error).message}`);
    return 1;
  }
}

if (import.meta.main) process.exit(main(process.argv.slice(2), process.env, new Date()));
