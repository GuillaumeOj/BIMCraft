#!/usr/bin/env bun
// Chooses the scripts/check-all.sh sections and crates a branch's changes can break, so the
// pre-push hook runs only those (rationale: docs/dev/quality.md). CI always runs everything.
// Usage: bun scripts/affected.ts [repo-root] [base-ref]
// Prints shell assignments for `eval`: SECTIONS (check-all sections) and CRATES (empty = all).

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { loadCrates, localDependencies, workspaceDependencies } from "./check-architecture";
import { git, type Table } from "./check-pinned-deps";

/** check-all.sh sections, in the order it runs them by default. */
const SECTIONS = ["repo", "rust", "coverage", "wasm", "fuzz", "ui", "e2e", "docs"] as const;
type Section = (typeof SECTIONS)[number];

/** Sections to run, and the crates the Rust ones cover (none: the whole workspace). */
type Scope = { sections: Set<Section>; crates: Set<string> };

// Files every Rust check depends on, including the lists that decide what check-all.sh covers.
const RUST_WIDE = new Set([
  "Cargo.toml",
  "Cargo.lock",
  "rust-toolchain",
  "rust-toolchain.toml",
  "clippy.toml",
  ".clippy.toml",
  "deny.toml",
  "rustfmt.toml",
  ".rustfmt.toml",
  "scripts/critical-crates.txt",
  "scripts/sbom.sh",
  "scripts/wasm-crates.txt",
]);

// Root files read only by the `repo` section or by nobody; any other root file runs everything.
const ROOT_REPO_ONLY = new Set([".editorconfig", ".gitattributes", ".gitignore", "LICENSE", "taplo.toml", "typos.toml"]);

/** Files read only by the `repo` section (spelling, TOML style, secrets, tooling tests) or by nobody. */
const repoOnly = (file: string): boolean =>
  /^(scripts|\.githooks|\.github|\.claude|docs)\//.test(file) || /^[^/]+\.md$/.test(file) || ROOT_REPO_ONLY.has(file);

/** The given crates plus every crate that depends on them, directly or not. */
export function withDependents(seeds: Iterable<string>, deps: Map<string, string[]>): Set<string> {
  const result = new Set(seeds);
  let grew = true;
  while (grew) {
    grew = false;
    for (const [name, crateDeps] of deps) {
      if (!result.has(name) && crateDeps.some((dep) => result.has(dep))) {
        result.add(name);
        grew = true;
      }
    }
  }
  return result;
}

/**
 * Maps changed files to checks. `crateDirs` maps `crates/<dir>` to crate names, `deps` gives each
 * crate's local dependencies and `fuzzed` the crates the fuzz targets link. Anything unrecognised
 * runs everything: a missed check costs a red CI run, a skipped one costs nothing locally.
 */
export function scopeOf(
  files: string[],
  crateDirs: Map<string, string>,
  deps: Map<string, string[]>,
  fuzzed: Set<string>,
): Scope {
  const sections = new Set<Section>(["repo"]);
  const changed = new Set<string>();
  let allCrates = false;
  const runAll = () => {
    for (const section of SECTIONS) sections.add(section);
    allCrates = true;
  };
  for (const file of files) {
    const crateDir = /^crates\/[^/]+/.exec(file)?.[0];
    // The checks themselves changed: nothing is known to still pass.
    if (file === "scripts/check-all.sh") runAll();
    else if (RUST_WIDE.has(file) || file.startsWith(".cargo/")) allCrates = true;
    else if (crateDir !== undefined) {
      const name = crateDirs.get(crateDir);
      if (name === undefined) runAll(); // a removed or unparsable crate: let the full run report it
      else changed.add(name);
    } else if (file.startsWith("fuzz/")) sections.add("fuzz");
    else if (file.startsWith("ui/")) {
      sections.add("ui");
      sections.add("e2e");
    } else if (file.startsWith("docs/user/")) sections.add("docs");
    else if (!repoOnly(file)) runAll();
  }
  const crates = withDependents(changed, deps);
  if (allCrates || crates.size > 0) {
    for (const section of ["rust", "coverage", "wasm"] as const) sections.add(section);
    if (allCrates || [...crates].some((name) => fuzzed.has(name))) sections.add("fuzz");
  }
  return { sections, crates: allCrates ? new Set() : crates };
}

export function format(scope: Scope): string {
  const sections = SECTIONS.filter((section) => scope.sections.has(section));
  return `SECTIONS='${sections.join(" ")}'\nCRATES='${[...scope.crates].sort().join(" ")}'`;
}

/**
 * Files changed on this branch since it left `base`, so a rebase or a force-push stays covered.
 * Without renames, a file moved out of a crate still marks that crate as changed.
 */
export function changedFiles(root: string, base: string): string[] {
  return git(root, "diff", "--name-only", "--no-renames", `${base}...HEAD`).split("\n").filter(Boolean);
}

function workspaceGraph(root: string): { crateDirs: Map<string, string>; deps: Map<string, string[]> } {
  const { crates, issues } = loadCrates(root);
  if (issues.length > 0) throw new Error(issues.map((i) => `${i.file}: ${i.message}`).join("\n"));
  const workspaceDeps = workspaceDependencies(root);
  return {
    crateDirs: new Map(crates.map((c) => [c.dir, c.name])),
    deps: new Map(crates.map((c) => [c.name, localDependencies(c.manifest, workspaceDeps)])),
  };
}

function fuzzedCrates(root: string): Set<string> {
  const file = join(root, "fuzz/Cargo.toml");
  if (!existsSync(file)) return new Set();
  return new Set(localDependencies(Bun.TOML.parse(readFileSync(file, "utf8")) as Table, {}));
}

export function main(root: string, base: string): number {
  try {
    const { crateDirs, deps } = workspaceGraph(root);
    console.log(format(scopeOf(changedFiles(root, base), crateDirs, deps, fuzzedCrates(root))));
    return 0;
  } catch (error) {
    console.error(`affected: ${(error as Error).message}`);
    return 1;
  }
}

if (import.meta.main) process.exit(main(process.argv[2] ?? process.cwd(), process.argv[3] ?? "origin/main"));
