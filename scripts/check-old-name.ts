#!/usr/bin/env bun
// Fails when the old product name (see docs/dev/adr/0028-rename-to-fabrilo.md) appears, in any case,
// in the path or content of a tracked file outside scripts/old-name-allowlist.txt.
// Usage: bun scripts/check-old-name.ts [file...]
// With files (the pre-commit hook passes the staged ones) only those are checked and stale allowlist
// entries are not reported, as the rest of the repository is out of sight.

import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";

export type SourceFile = { path: string; content: string; binary?: boolean };
/** `phrase` set: only lines of the matching files that contain it are allowed. `*` matches any path. */
export type AllowEntry = { raw: string; path: string; phrase?: string };
/** `line` is 1-based; absent for a hit on the path itself. */
export type Hit = { path: string; line?: number; text?: string };

const ALLOWLIST = "scripts/old-name-allowlist.txt";
const ADR = "docs/dev/adr/0028-rename-to-fabrilo.md";
const OLD_NAME = /bimcraft/i;
const BINARY_PROBE = 8192;
// These files have to spell the name out to do their job, so they cannot be allowlisted by phrase.
const SELF = new Set([ALLOWLIST, "scripts/check-old-name.ts", "scripts/check-old-name.test.ts"]);

/** One entry per line: `<path>` (a directory when it ends with `/`) or `<path> | <phrase>`; `#` starts a comment. */
export function parseAllowlist(content: string): AllowEntry[] {
  return content
    .split("\n")
    .map((line) => line.replace(/^\s*#.*|\s#.*/, "").trim())
    .filter(Boolean)
    .map((raw) => {
      const [path = "", ...rest] = raw.split("|");
      const phrase = rest.length > 0 ? rest.join("|").trim() : undefined;
      return { raw, path: path.trim(), ...(phrase ? { phrase } : {}) };
    });
}

const pathMatches = (entry: AllowEntry, path: string): boolean =>
  entry.path === "*" ||
  entry.path === path ||
  (entry.path.endsWith("/") && path.startsWith(entry.path));

/** Every hit of the files, each with the entries that allow it (none: the hit is a violation). */
function classify(files: SourceFile[], entries: AllowEntry[]): { hit: Hit; allowedBy: AllowEntry[] }[] {
  return files
    .filter((file) => !SELF.has(file.path))
    .flatMap((file) => {
      const hits: Hit[] = [];
      if (OLD_NAME.test(file.path)) hits.push({ path: file.path });
      if (!file.binary) {
        file.content.split("\n").forEach((text, index) => {
          if (OLD_NAME.test(text)) hits.push({ path: file.path, line: index + 1, text: text.trim() });
        });
      }
      return hits.map((hit) => ({
        hit,
        allowedBy: entries.filter(
          (entry) =>
            pathMatches(entry, file.path) &&
            (entry.phrase === undefined || (hit.text ?? "").includes(entry.phrase)),
        ),
      }));
    });
}

/** The hits no allowlist entry covers. */
export const findHits = (files: SourceFile[], entries: AllowEntry[]): Hit[] =>
  classify(files, entries)
    .filter(({ allowedBy }) => allowedBy.length === 0)
    .map(({ hit }) => hit);

/** The entries that allow nothing: dead weight that would hide a future hit. */
export function findStaleEntries(files: SourceFile[], entries: AllowEntry[]): AllowEntry[] {
  const used = new Set(classify(files, entries).flatMap(({ allowedBy }) => allowedBy));
  return entries.filter((entry) => !used.has(entry));
}

export function isBinary(buffer: Uint8Array): boolean {
  return buffer.subarray(0, BINARY_PROBE).includes(0);
}

function trackedFiles(root: string): string[] {
  const run = Bun.spawnSync(["git", "ls-files", "-z"], { cwd: root });
  if (run.exitCode !== 0) throw new Error(`git ls-files failed: ${run.stderr.toString().trim()}`);
  return run.stdout.toString().split("\0").filter(Boolean);
}

const isFile = (path: string): boolean => {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
};

/** Paths that are not files on disk (deleted but still in the index, submodules) are skipped. */
function readSources(root: string, paths: string[]): SourceFile[] {
  return paths
    .filter((path) => isFile(join(root, path)))
    .map((path) => {
      const buffer = readFileSync(join(root, path));
      const binary = isBinary(buffer);
      return { path, binary, content: binary ? "" : buffer.toString("utf8") };
    });
}

const format = (hit: Hit): string =>
  hit.line === undefined ? `  ${hit.path}: (file name)` : `  ${hit.path}:${hit.line}: ${hit.text}`;

export function main(argv: string[], root: string): number {
  const entries = parseAllowlist(readFileSync(join(root, ALLOWLIST), "utf8"));
  const partial = argv.length > 0;
  const files = readSources(root, partial ? argv : trackedFiles(root));
  const hits = findHits(files, entries);
  const stale = partial ? [] : findStaleEntries(files, entries);

  if (hits.length > 0) {
    console.error("check-old-name: the old name BIMCraft appears outside the allowlist:");
    for (const hit of hits) console.error(format(hit));
    console.error(`Use Fabrilo and the identifiers in the table of ${ADR}.`);
    console.error(`A justified exception goes in ${ALLOWLIST}.`);
  }
  if (stale.length > 0) {
    console.error(`check-old-name: stale allowlist entries in ${ALLOWLIST} (they allow nothing; remove them):`);
    for (const entry of stale) console.error(`  ${entry.raw}`);
  }
  if (hits.length > 0 || stale.length > 0) return 1;
  console.log("The old name BIMCraft appears only where the allowlist permits.");
  return 0;
}

if (import.meta.main) process.exit(main(process.argv.slice(2), process.cwd()));
