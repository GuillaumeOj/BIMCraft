#!/usr/bin/env bun
// Enforces AGENTS.md §4: every dependency, toolchain and CI action is pinned to an exact version.
// Usage: bun scripts/check-pinned-deps.ts [repo-root]

import { readFileSync } from "node:fs";
import { basename, join } from "node:path";

export type Issue = { file: string; message: string };

const SEMVER = String.raw`\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?`;
const EXACT_CARGO = new RegExp(`^=${SEMVER}$`);
const EXACT_NPM = new RegExp(`^${SEMVER}$`);
const EXACT_TOOLCHAIN = /^\d+\.\d+\.\d+$/;
const COMMIT_SHA = /^[0-9a-f]{40}$/;
const CARGO_DEP_SECTIONS = ["dependencies", "dev-dependencies", "build-dependencies"];
const NPM_DEP_SECTIONS = ["dependencies", "devDependencies", "optionalDependencies", "overrides"];

type Table = Record<string, unknown>;

const isTable = (value: unknown): value is Table =>
  typeof value === "object" && value !== null && !Array.isArray(value);

function checkCargoDep(file: string, name: string, spec: unknown): Issue[] {
  if (typeof spec === "string") {
    return EXACT_CARGO.test(spec)
      ? []
      : [{ file, message: `crate "${name}" must use an exact version "=x.y.z", got "${spec}"` }];
  }
  if (!isTable(spec)) {
    return [{ file, message: `crate "${name}" has an unsupported specification` }];
  }
  if (spec.workspace === true) return [];
  if ("git" in spec) {
    return [{ file, message: `crate "${name}" is a git dependency; use a crates.io release` }];
  }
  if (typeof spec.version === "string") return checkCargoDep(file, name, spec.version);
  if ("path" in spec) return [];
  return [{ file, message: `crate "${name}" has no version` }];
}

function cargoDepTables(manifest: Table): Table[] {
  const tables: Table[] = [];
  const collect = (owner: Table) => {
    for (const section of CARGO_DEP_SECTIONS) {
      if (isTable(owner[section])) tables.push(owner[section]);
    }
  };
  collect(manifest);
  if (isTable(manifest.workspace) && isTable(manifest.workspace.dependencies)) {
    tables.push(manifest.workspace.dependencies);
  }
  if (isTable(manifest.target)) {
    for (const target of Object.values(manifest.target)) {
      if (isTable(target)) collect(target);
    }
  }
  return tables;
}

export function checkCargoToml(file: string, content: string): Issue[] {
  const manifest = Bun.TOML.parse(content) as Table;
  return cargoDepTables(manifest).flatMap((deps) =>
    Object.entries(deps).flatMap(([name, spec]) => checkCargoDep(file, name, spec)),
  );
}

export function checkPackageJson(file: string, content: string): Issue[] {
  const pkg = JSON.parse(content) as Table;
  const issues: Issue[] = [];
  for (const section of NPM_DEP_SECTIONS) {
    const deps = pkg[section];
    if (!isTable(deps)) continue;
    for (const [name, version] of Object.entries(deps)) {
      const ok =
        typeof version === "string" && (EXACT_NPM.test(version) || version === "workspace:*");
      if (!ok) {
        issues.push({
          file,
          message: `package "${name}" in ${section} must use an exact version, got ${JSON.stringify(version)}`,
        });
      }
    }
  }
  const manager = pkg.packageManager;
  if (manager !== undefined && !new RegExp(`^bun@${SEMVER}$`).test(String(manager))) {
    issues.push({ file, message: `packageManager must be "bun@x.y.z", got "${String(manager)}"` });
  }
  return issues;
}

export function checkWorkflow(file: string, content: string): Issue[] {
  const issues: Issue[] = [];
  for (const match of content.matchAll(/^\s*-?\s*uses:\s*["']?([^\s"'#]+)/gm)) {
    const ref = match[1] ?? "";
    if (ref.startsWith("./")) continue;
    if (ref.startsWith("docker://")) {
      if (!/@sha256:[0-9a-f]{64}$/.test(ref)) {
        issues.push({ file, message: `docker action "${ref}" must be pinned by sha256 digest` });
      }
      continue;
    }
    const version = ref.split("@")[1] ?? "";
    if (!COMMIT_SHA.test(version)) {
      issues.push({ file, message: `action "${ref}" must be pinned to a full commit SHA` });
    }
  }
  return issues;
}

export function checkRustToolchain(file: string, content: string): Issue[] {
  const toolchain = (Bun.TOML.parse(content) as Table).toolchain;
  const channel = isTable(toolchain) ? toolchain.channel : undefined;
  return typeof channel === "string" && EXACT_TOOLCHAIN.test(channel)
    ? []
    : [{ file, message: `toolchain channel must be an exact version "x.y.z", got "${String(channel)}"` }];
}

export function checkFile(file: string, content: string): Issue[] {
  const name = basename(file);
  if (name === "Cargo.toml") return checkCargoToml(file, content);
  if (name === "package.json") return checkPackageJson(file, content);
  if (name === "rust-toolchain.toml") return checkRustToolchain(file, content);
  if (/^\.github\/workflows\/[^/]+\.ya?ml$/.test(file)) return checkWorkflow(file, content);
  return [];
}

function listRepoFiles(root: string): string[] {
  const result = Bun.spawnSync(
    ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
    { cwd: root },
  );
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  return result.stdout.toString().split("\n").filter(Boolean);
}

export function checkRepo(root: string): Issue[] {
  return listRepoFiles(root).flatMap((file) => {
    let content: string;
    try {
      content = readFileSync(join(root, file), "utf8");
    } catch {
      return []; // deleted but still in the index
    }
    try {
      return checkFile(file, content);
    } catch (error) {
      return [{ file, message: `cannot parse: ${(error as Error).message}` }];
    }
  });
}

export function main(root: string): number {
  const issues = checkRepo(root);
  for (const issue of issues) console.error(`${issue.file}: ${issue.message}`);
  if (issues.length > 0) {
    console.error(`\n${issues.length} unpinned version(s). See AGENTS.md §4.`);
    return 1;
  }
  console.log("All dependencies, toolchains and actions are pinned.");
  return 0;
}

if (import.meta.main) process.exit(main(process.argv[2] ?? process.cwd()));
