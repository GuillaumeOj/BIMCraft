#!/usr/bin/env bun
// Checks a release tag against the program's version (ENG-05, docs/dev/release.md).
// A stable release is tagged vX.Y.Z, a beta vX.Y.Z-beta.N; no other tag starts a release.
//
// Usage, from the repository root:
//   TAG=v1.2.0 bun scripts/release/check-version.ts
//     The tag must match the `version` of crates/desktop-app/Cargo.toml. Without TAG (a dry run),
//     the tag is the one the version would have. Writes `tag`, `version` and `prerelease` to
//     $GITHUB_OUTPUT when it is set.
//   TAG=v1.2.0 PRERELEASE=false bun scripts/release/check-version.ts --published
//     A published release must be a pre-release exactly when its tag is a beta.

import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** What a release tag says. */
export type Release = { tag: string; version: string; prerelease: boolean };

/** Where Tauri reads the program's version from. */
export const MANIFEST = "crates/desktop-app/Cargo.toml";

const NUMBER = "(0|[1-9]\\d*)";
const TAG = new RegExp(`^v(${NUMBER}\\.${NUMBER}\\.${NUMBER}(?:-beta\\.${NUMBER})?)$`);

/** The release a tag names, or `null` when the tag does not start a release. */
export function parseTag(tag: string): Release | null {
  const version = TAG.exec(tag)?.[1];
  return version === undefined ? null : { tag, version, prerelease: version.includes("-") };
}

/** The `[package] version` of a Cargo manifest. */
export function cargoVersion(manifest: string): string {
  const parsed = Bun.TOML.parse(manifest) as { package?: { version?: unknown } };
  const version = parsed.package?.version;
  if (typeof version !== "string") throw new Error(`${MANIFEST} has no [package] version`);
  return version;
}

const shapes = "vX.Y.Z (stable) or vX.Y.Z-beta.N (beta)";

/** The release `tag` starts, when it is well formed and names `version`. */
export function checkTag(tag: string, version: string): Release {
  const release = parseTag(tag);
  if (release === null) throw new Error(`tag "${tag}" is not a release tag: use ${shapes}`);
  if (release.version !== version) {
    throw new Error(`tag "${tag}" does not match version "${version}" of ${MANIFEST}: bump it first`);
  }
  return release;
}

/** The published release `tag`, when its pre-release flag matches its tag shape. */
export function checkPublished(tag: string, prerelease: boolean): Release {
  const release = parseTag(tag);
  if (release === null) throw new Error(`tag "${tag}" is not a release tag: use ${shapes}`);
  if (release.prerelease !== prerelease) {
    const expected = release.prerelease ? "be a pre-release" : "not be a pre-release";
    throw new Error(`release "${tag}" must ${expected}: fix the flag on GitHub`);
  }
  return release;
}

type Env = Record<string, string | undefined>;

function run(args: string[], env: Env, root: string): string {
  if (args.includes("--published")) {
    const flag = env.PRERELEASE;
    if (flag !== "true" && flag !== "false") throw new Error("PRERELEASE must be true or false");
    const release = checkPublished(env.TAG ?? "", flag === "true");
    return `Release ${release.tag} is correctly ${release.prerelease ? "" : "not "}a pre-release.`;
  }
  const version = cargoVersion(readFileSync(join(root, MANIFEST), "utf8"));
  const release = checkTag(env.TAG || `v${version}`, version);
  if (env.GITHUB_OUTPUT) {
    const lines = [`tag=${release.tag}`, `version=${release.version}`, `prerelease=${release.prerelease}`];
    appendFileSync(env.GITHUB_OUTPUT, `${lines.join("\n")}\n`);
  }
  return `Tag ${release.tag} matches version ${release.version} (${release.prerelease ? "beta" : "stable"}).`;
}

export function main(args: string[], env: Env, root: string): number {
  try {
    console.log(run(args, env, root));
    return 0;
  } catch (error) {
    console.error(`check-version: ${(error as Error).message}`);
    return 1;
  }
}

if (import.meta.main) process.exit(main(process.argv.slice(2), process.env, process.cwd()));
