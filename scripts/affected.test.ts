import { describe, expect, spyOn, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { changedFiles, format, main, scopeOf, withDependents } from "./affected";
import { git } from "./check-pinned-deps";

// schema <- schema-gen <- (fuzz); geom stands alone.
const crateDirs = new Map([
  ["crates/schema", "schema"],
  ["crates/schema-gen", "schema-gen"],
  ["crates/geom", "geom"],
]);
const deps = new Map([
  ["schema", []],
  ["schema-gen", ["schema"]],
  ["geom", []],
]);
const fuzzed = new Set(["schema-gen"]);

const scope = (...files: string[]): string => format(scopeOf(files, crateDirs, deps, fuzzed));
const ALL = "SECTIONS='repo rust coverage wasm fuzz ui e2e docs'\nCRATES=''";

describe("withDependents", () => {
  test("adds direct and indirect dependents, not dependencies", () => {
    const chain = new Map([
      ["a", []],
      ["b", ["a"]],
      ["c", ["b"]],
    ]);
    expect([...withDependents(["b"], chain)].sort()).toEqual(["b", "c"]);
    expect([...withDependents(["a"], chain)].sort()).toEqual(["a", "b", "c"]);
  });
});

describe("scopeOf", () => {
  test("no change runs the repository checks only", () => {
    expect(scope()).toBe("SECTIONS='repo'\nCRATES=''");
  });

  test("a crate change checks it and its dependents, and fuzzes them when linked", () => {
    expect(scope("crates/schema/src/lib.rs")).toBe(
      "SECTIONS='repo rust coverage wasm fuzz'\nCRATES='schema schema-gen'",
    );
  });

  test("a crate the fuzz targets do not link is not fuzzed", () => {
    expect(scope("crates/geom/src/lib.rs")).toBe("SECTIONS='repo rust coverage wasm'\nCRATES='geom'");
  });

  test.each(["Cargo.lock", "clippy.toml", ".rustfmt.toml", ".cargo/config.toml", "scripts/critical-crates.txt", "scripts/sbom.sh"])(
    "%s checks every crate",
    (file) => {
      expect(scope(file, "crates/geom/src/lib.rs")).toBe("SECTIONS='repo rust coverage wasm fuzz'\nCRATES=''");
    },
  );

  test.each(["scripts/check-all.sh", "crates/removed/src/lib.rs", "assets/model.ifc", "build.rs"])(
    "%s runs everything",
    (file) => {
      expect(scope(file)).toBe(ALL);
    },
  );

  test("fuzz targets, UI and user guide map to their sections", () => {
    expect(scope("fuzz/fuzz_targets/x.rs")).toBe("SECTIONS='repo fuzz'\nCRATES=''");
    expect(scope("ui/src/App.tsx")).toBe("SECTIONS='repo ui e2e'\nCRATES=''");
    expect(scope("docs/user/SUMMARY.md")).toBe("SECTIONS='repo docs'\nCRATES=''");
  });

  test.each(["README.md", "typos.toml", "scripts/affected.ts", ".githooks/pre-push", "docs/dev/quality.md"])(
    "%s runs the repository checks only",
    (file) => {
      expect(scope(file)).toBe("SECTIONS='repo'\nCRATES=''");
    },
  );
});

function write(root: string, file: string, content: string) {
  mkdirSync(join(root, file, ".."), { recursive: true });
  writeFileSync(join(root, file), content);
}

/** A repository whose `main` holds two crates, with a branch changing `low`. */
function repo(): string {
  const root = mkdtempSync(join(tmpdir(), "affected-"));
  git(root, "init", "-q", "-b", "main");
  git(root, "config", "user.email", "test@example.com");
  git(root, "config", "user.name", "Test");
  write(root, "Cargo.toml", '[workspace]\n[workspace.dependencies]\nlow = { path = "crates/low" }\n');
  write(root, "crates/low/Cargo.toml", '[package]\nname = "low"\n');
  write(root, "crates/high/Cargo.toml", '[package]\nname = "high"\n[dependencies]\nlow = { workspace = true }\n');
  write(root, "fuzz/Cargo.toml", '[dependencies]\nhigh = { path = "../crates/high" }\n');
  git(root, "add", ".");
  git(root, "commit", "-q", "-m", "base");
  git(root, "checkout", "-q", "-b", "topic");
  write(root, "crates/low/src/lib.rs", "");
  git(root, "add", ".");
  git(root, "commit", "-q", "-m", "change");
  return root;
}

describe("changedFiles", () => {
  test("a file moved out of a crate still marks the crate", () => {
    const root = repo();
    git(root, "mv", "crates/high/Cargo.toml", "notes.toml");
    git(root, "commit", "-q", "-m", "move");
    expect(changedFiles(root, "main")).toEqual(["crates/high/Cargo.toml", "crates/low/src/lib.rs", "notes.toml"]);
  });
});

describe("main", () => {
  test("prints the scope of the branch since it left the base", () => {
    const root = repo();
    const log = spyOn(console, "log").mockImplementation(() => undefined);
    expect(main(root, "main")).toBe(0);
    expect(log).toHaveBeenCalledWith("SECTIONS='repo rust coverage wasm fuzz'\nCRATES='high low'");
    log.mockRestore();
  });

  test("fails so the hook can fall back to the full run", () => {
    const root = repo();
    const error = spyOn(console, "error").mockImplementation(() => undefined);
    expect(main(root, "missing/main")).toBe(1);
    expect(error.mock.calls[0]?.[0]).toStartWith("affected: git diff");
    error.mockRestore();
  });

  test("refuses a workspace it cannot read", () => {
    const root = repo();
    write(root, "crates/broken/Cargo.toml", "[lib]\n");
    const error = spyOn(console, "error").mockImplementation(() => undefined);
    expect(main(root, "main")).toBe(1);
    expect(error.mock.calls[0]?.[0]).toContain("no [package] name");
    error.mockRestore();
  });
});
