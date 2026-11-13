#!/usr/bin/env bun
// Enforces the architecture rules of docs/specifications.md §4:
// - ARCH-04: no crate depends on a crate in a higher layer (layers: scripts/crate-layers.txt);
// - ARCH-02: core crates (scripts/wasm-crates.txt) forbid the forbidden-API lints of clippy.toml;
// - ARCH-03: discipline crates (disc-*) depend only on the public extension interfaces listed in
//   scripts/extension-surface.txt, and core-ext re-exports items by name, never a glob or a crate.
// Usage: bun scripts/check-architecture.ts [repo-root]

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { type Issue, isTable, packageDepTables, type Table } from "./check-pinned-deps";

export type Crate = { name: string; dir: string; manifest: Table };

const LAYERS_FILE = "scripts/crate-layers.txt";
const CORE_FILE = "scripts/wasm-crates.txt";
const SURFACE_FILE = "scripts/extension-surface.txt";
const FACADE = "core-ext";
const FORBIDDEN_LINTS = ["clippy::disallowed_methods", "clippy::disallowed_types"];
const FORBID = `#![forbid(${FORBIDDEN_LINTS.join(", ")})]`;

const meaningfulLines = (content: string): string[] =>
  content
    .split("\n")
    .map((line) => line.replace(/#.*/, "").trim())
    .filter(Boolean);

/** One array of crate names per layer, lowest first. */
export const parseLayers = (content: string): string[][] =>
  meaningfulLines(content).map((line) => line.split(/\s+/));

// Read like `existing_crates` in check-all.sh does: one name per line, no comments.
export const parseCrateList = (content: string): Set<string> =>
  new Set(content.split("\n").map((line) => line.trim()).filter(Boolean));

/** Every layer that declares the crate; a well-formed layers file yields exactly one. */
export const layersOf = (layers: string[][], name: string): number[] =>
  layers.flatMap((names, index) => (names.includes(name) ? [index] : []));

/**
 * Names of the local (path) crates a manifest depends on, in any section or target. A
 * `{ workspace = true }` dependency is resolved through the root `[workspace.dependencies]`, so
 * renames declared there are followed and registry crates are never mistaken for members.
 */
export function localDependencies(manifest: Table, workspaceDeps: Table): string[] {
  return packageDepTables(manifest).flatMap((deps) =>
    Object.entries(deps).flatMap(([key, spec]) => {
      const resolved = isTable(spec) && spec.workspace === true ? workspaceDeps[key] : spec;
      if (!isTable(resolved) || !("path" in resolved)) return [];
      return [typeof resolved.package === "string" ? resolved.package : key];
    }),
  );
}

export function checkLayering(crates: Crate[], layers: string[][], workspaceDeps: Table): Issue[] {
  return crates.flatMap((crate) => {
    const file = join(crate.dir, "Cargo.toml");
    const [layer, ...extra] = layersOf(layers, crate.name);
    if (layer === undefined || extra.length > 0) {
      const problem = layer === undefined ? "has no layer" : `is in several layers (${[layer, ...extra].join(", ")})`;
      return [{ file, message: `crate "${crate.name}" ${problem}; fix ${LAYERS_FILE}` }];
    }
    return localDependencies(crate.manifest, workspaceDeps).flatMap((dep) => {
      // A dependency with no or several layers is reported on its own manifest.
      const [depLayer] = layersOf(layers, dep);
      if (depLayer === undefined || depLayer <= layer) return [];
      return [
        {
          file,
          message: `${crate.name} (layer ${layer}) depends on ${dep} (layer ${depLayer}), a higher layer; see ARCH-04`,
        },
      ];
    });
  });
}

const isDiscipline = (name: string): boolean => name.startsWith("disc-");

/** The crates of scripts/extension-surface.txt. */
export const parseSurface = (content: string): Set<string> => new Set(meaningfulLines(content));

/**
 * ARCH-03: a discipline may depend, in any section, only on the listed crates, and never on
 * another discipline, as a plugin could not either. Every listed crate must exist.
 */
export function checkExtensionSurface(crates: Crate[], surface: Set<string>, workspaceDeps: Table): Issue[] {
  const names = new Set(crates.map((c) => c.name));
  const missing = [...surface]
    .filter((name) => !names.has(name))
    .map((name) => ({ file: SURFACE_FILE, message: `"${name}" is not a crate in crates/; fix the list (ARCH-03)` }));
  const forbidden = crates
    .filter((crate) => isDiscipline(crate.name))
    .flatMap((crate) => {
      const file = join(crate.dir, "Cargo.toml");
      return [...new Set(localDependencies(crate.manifest, workspaceDeps))]
        .filter((dep) => isDiscipline(dep) || !surface.has(dep))
        .map((dep) => ({
          file,
          message: isDiscipline(dep)
            ? `${crate.name} depends on ${dep}, another discipline; disciplines share code only through ${FACADE} (ARCH-03)`
            : `${crate.name} depends on ${dep}, which is not a public extension interface; use ${FACADE} (ARCH-03)`,
        }));
    });
  return [...missing, ...forbidden];
}

const stripComments = (source: string): string => source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*/g, "");

/** Why a `pub use` statement would leak more than the items it names, if it does. */
const leak = (tree: string): string | undefined => {
  if (tree.includes("*")) return "a glob";
  const path = tree.replace(/^::/, "").trim();
  // `x`, `x as y`, `{x, y}` or `x::{self}` re-export a whole crate (or module).
  if (/^\{/.test(path) || /^\w+(\s+as\s+\w+)?$/.test(path) || /\bself\b(?!\s*::)/.test(path)) {
    return "a whole crate";
  }
  return undefined;
};

/** ARCH-03: the façade names each item it re-exports, so the surface is exactly what it lists. */
export function checkFacade(root: string, crate: Crate): Issue[] {
  const file = join(crate.dir, "src/lib.rs");
  if (!existsSync(join(root, file))) return [{ file: crate.dir, message: `${FACADE} has no src/lib.rs to check` }];
  const source = stripComments(readFileSync(join(root, file), "utf8"));
  const statements = [...source.matchAll(/\bpub\s+use\s+([^;]+);/g)].map((m) => m[1] ?? "");
  const leaks = [
    ...(/\bpub\s+extern\s+crate\b/.test(source) ? ["a whole crate"] : []),
    ...statements.flatMap((tree) => leak(tree) ?? []),
  ];
  return [...new Set(leaks)].map((what) => ({
    file,
    message: `${FACADE} re-exports ${what}; name each item of the extension surface instead (ARCH-03)`,
  }));
}

const forbidsLints = (source: string): boolean => {
  const forbidden = [...source.matchAll(/#!\[forbid\(([^)]*)\)\]/g)].flatMap((m) =>
    (m[1] ?? "").split(",").map((lint) => lint.trim()),
  );
  return FORBIDDEN_LINTS.every((lint) => forbidden.includes(lint));
};

/** Why a crate-root forbid is enough, and why tests/ is exempt: docs/dev/quality.md. */
export function checkForbid(root: string, crate: Crate): Issue[] {
  const issues: Issue[] = [];
  for (const config of ["clippy.toml", ".clippy.toml"]) {
    const file = join(crate.dir, config);
    if (existsSync(join(root, file))) {
      issues.push({ file, message: "core crates use the root clippy.toml (ARCH-02)" });
    }
  }
  const crateRoots = ["src/lib.rs", "src/main.rs"].map((path) => join(crate.dir, path));
  const existing = crateRoots.filter((file) => existsSync(join(root, file)));
  if (existing.length === 0) {
    issues.push({ file: crate.dir, message: "core crate has no src/lib.rs or src/main.rs to check" });
  }
  for (const file of existing) {
    if (!forbidsLints(readFileSync(join(root, file), "utf8"))) {
      issues.push({ file, message: `core crate roots must start with ${FORBID} (ARCH-02)` });
    }
  }
  return issues;
}

export function loadCrates(root: string): { crates: Crate[]; issues: Issue[] } {
  const crates: Crate[] = [];
  const issues: Issue[] = [];
  const cratesDir = join(root, "crates");
  const dirs = existsSync(cratesDir) ? readdirSync(cratesDir).sort().map((name) => join("crates", name)) : [];
  for (const dir of dirs) {
    const file = join(dir, "Cargo.toml");
    if (!existsSync(join(root, file))) continue;
    let manifest: Table;
    try {
      manifest = Bun.TOML.parse(readFileSync(join(root, file), "utf8")) as Table;
    } catch (error) {
      issues.push({ file, message: `cannot parse: ${(error as Error).message}` });
      continue;
    }
    const name = isTable(manifest.package) ? manifest.package.name : undefined;
    if (typeof name === "string") crates.push({ name, dir, manifest });
    else issues.push({ file, message: "manifest has no [package] name" });
  }
  return { crates, issues };
}

export function workspaceDependencies(root: string): Table {
  const manifest = Bun.TOML.parse(readFileSync(join(root, "Cargo.toml"), "utf8")) as Table;
  return isTable(manifest.workspace) && isTable(manifest.workspace.dependencies)
    ? manifest.workspace.dependencies
    : {};
}

export function checkRepo(root: string): Issue[] {
  const { crates, issues } = loadCrates(root);
  const layers = parseLayers(readFileSync(join(root, LAYERS_FILE), "utf8"));
  const core = parseCrateList(readFileSync(join(root, CORE_FILE), "utf8"));
  const surface = parseSurface(readFileSync(join(root, SURFACE_FILE), "utf8"));
  const workspaceDeps = workspaceDependencies(root);
  return [
    ...issues,
    ...checkLayering(crates, layers, workspaceDeps),
    ...crates.filter((c) => core.has(c.name)).flatMap((c) => checkForbid(root, c)),
    ...checkExtensionSurface(crates, surface, workspaceDeps),
    ...crates.filter((c) => c.name === FACADE).flatMap((c) => checkFacade(root, c)),
  ];
}

export function main(root: string): number {
  const issues = checkRepo(root);
  for (const issue of issues) console.error(`${issue.file}: ${issue.message}`);
  if (issues.length > 0) {
    console.error(`\n${issues.length} architecture violation(s). See docs/dev/quality.md.`);
    return 1;
  }
  console.log("Crate layering, core-crate lint scope and the discipline extension surface are respected.");
  return 0;
}

if (import.meta.main) process.exit(main(process.argv[2] ?? process.cwd()));
