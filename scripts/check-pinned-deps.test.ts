import { describe, expect, spyOn, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  checkCargoToml,
  checkFile,
  checkPackageJson,
  checkRepo,
  checkRustToolchain,
  checkWorkflow,
  main,
} from "./check-pinned-deps";

const SHA = "3d3c42e5aac5ba805825da76410c181273ba90b1";

function gitRepo(prefix: string): string {
  const root = mkdtempSync(join(tmpdir(), prefix));
  Bun.spawnSync(["git", "init", "-q"], { cwd: root });
  return root;
}

const cargoMessages = (toml: string): string[] =>
  checkCargoToml("Cargo.toml", toml).map((i) => i.message);

describe("checkCargoToml", () => {
  test("accepts exact, workspace and path dependencies", () => {
    const toml = `
[dependencies]
serde = "=1.0.228"
anyhow = { version = "=1.0.98", default-features = false }
core-model = { workspace = true }
local = { path = "../local" }
[workspace.dependencies]
thiserror = "=2.0.12"
[target.'cfg(windows)'.dependencies]
winapi = "=0.3.9"
`;
    expect(checkCargoToml("Cargo.toml", toml)).toEqual([]);
  });

  test("rejects ranges, git and missing versions in every section", () => {
    const toml = `
[dependencies]
caret = "^1.0"
bare = "1.0.0"
[dev-dependencies]
tilde = { version = "~1.2" }
[build-dependencies]
from-git = { git = "https://example.com/x" }
[workspace.dependencies]
star = "*"
[target.'cfg(unix)'.dependencies]
nover = { features = ["x"] }
odd = 3
`;
    const messages = cargoMessages(toml);
    const text = messages.join("\n");
    expect(messages).toHaveLength(7);
    expect(text).toContain('"caret"');
    expect(text).toContain("git dependency");
    expect(text).toContain('"nover" has no version');
    expect(text).toContain('"odd" has an unsupported');
  });

  test("checks [patch] and [replace] overrides", () => {
    const toml = `
[patch.crates-io]
serde = { git = "https://example.com/serde" }
pinned = "=1.0.0"
[replace]
"foo:0.1.0" = { version = "^0.1" }
`;
    const messages = cargoMessages(toml);
    const text = messages.join("\n");
    expect(messages).toHaveLength(2);
    expect(text).toContain('"serde" is a git dependency');
    expect(text).toContain('"foo:0.1.0" must use an exact version');
  });

  test("accepts a manifest without dependencies", () => {
    expect(checkCargoToml("Cargo.toml", "[workspace]\nmembers = []\n")).toEqual([]);
  });
});

describe("checkPackageJson", () => {
  test("accepts exact versions, workspace links and a pinned bun", () => {
    const pkg = {
      packageManager: "bun@1.4.2",
      dependencies: { react: "19.2.0", shared: "workspace:*" },
      devDependencies: { "@biomejs/biome": "2.5.14", beta: "1.0.0-beta.3" },
      peerDependencies: { react: ">=18" },
    };
    expect(checkPackageJson("package.json", JSON.stringify(pkg))).toEqual([]);
  });

  test("rejects ranges, tags, non-strings and an unpinned package manager", () => {
    const pkg = {
      packageManager: "bun@latest",
      dependencies: { a: "^1.0.0", b: "~1.0.0", c: "latest", d: ">=1" },
      optionalDependencies: { e: "1.x" },
      overrides: { f: { nested: "1.0.0" } },
    };
    const issues = checkPackageJson("package.json", JSON.stringify(pkg));
    expect(issues).toHaveLength(7);
    expect(issues.at(-1)?.message).toContain("packageManager");
  });
});

describe("checkWorkflow", () => {
  test("accepts SHA-pinned, local and digest-pinned actions", () => {
    const yml = `
steps:
  - uses: actions/checkout@${SHA} # v7.0.1
  - uses: ./.github/actions/local
  - uses: "docker://alpine@sha256:${"a".repeat(64)}"
`;
    expect(checkWorkflow(".github/workflows/ci.yml", yml)).toEqual([]);
  });

  test("rejects tags, branches and unpinned docker images", () => {
    const yml = `
steps:
  - uses: actions/checkout@v4
  - uses: owner/repo@main
  - uses: owner/repo
  - uses: docker://alpine:3
`;
    expect(checkWorkflow(".github/workflows/ci.yml", yml)).toHaveLength(4);
  });
});

describe("checkRustToolchain", () => {
  test("accepts an exact channel", () => {
    expect(checkRustToolchain("rust-toolchain.toml", '[toolchain]\nchannel = "1.98.1"\n')).toEqual(
      [],
    );
  });

  test("rejects floating or missing channels", () => {
    expect(checkRustToolchain("t", '[toolchain]\nchannel = "stable"\n')).toHaveLength(1);
    expect(checkRustToolchain("t", '[toolchain]\nchannel = "1.98"\n')).toHaveLength(1);
    expect(checkRustToolchain("t", "")).toHaveLength(1);
  });
});

describe("checkFile", () => {
  test("dispatches by file name and ignores other files", () => {
    expect(checkFile("crates/x/Cargo.toml", '[dependencies]\na = "1"\n')).toHaveLength(1);
    expect(checkFile("ui/package.json", '{"dependencies":{"a":"^1.0.0"}}')).toHaveLength(1);
    expect(checkFile("rust-toolchain.toml", "")).toHaveLength(1);
    expect(checkFile(".github/workflows/ci.yaml", "- uses: a/b@v1")).toHaveLength(1);
    expect(checkFile(".github/dependabot.yml", "- uses: a/b@v1")).toEqual([]);
    expect(checkFile("README.md", "anything")).toEqual([]);
  });
});

describe("checkRepo", () => {
  test("scans tracked and untracked files and reports parse errors", () => {
    const root = gitRepo("pins-");
    mkdirSync(join(root, ".github/workflows"), { recursive: true });
    writeFileSync(join(root, "Cargo.toml"), '[dependencies]\nok = "=1.0.0"\n');
    writeFileSync(join(root, ".github/workflows/ci.yml"), "- uses: a/b@v1\n");
    writeFileSync(join(root, "package.json"), "{ not json");
    const issues = checkRepo(root);
    expect(issues.map((i) => i.file).sort()).toEqual([".github/workflows/ci.yml", "package.json"]);
    expect(issues.find((i) => i.file === "package.json")?.message).toContain("cannot parse");
  });

  test("skips files deleted from the working tree", () => {
    const root = gitRepo("deleted-");
    writeFileSync(join(root, "Cargo.toml"), '[dependencies]\na = "1"\n');
    Bun.spawnSync(["git", "add", "Cargo.toml"], { cwd: root });
    rmSync(join(root, "Cargo.toml"));
    expect(checkRepo(root)).toEqual([]);
  });

  test("fails outside a git repository", () => {
    const root = mkdtempSync(join(tmpdir(), "nogit-"));
    expect(() => checkRepo(root)).toThrow();
  });
});

describe("main", () => {
  test("returns 0 and reports success on a pinned repository", () => {
    const root = gitRepo("main-ok-");
    const log = spyOn(console, "log").mockImplementation(() => undefined);
    expect(main(root)).toBe(0);
    expect(log).toHaveBeenCalledTimes(1);
    log.mockRestore();
  });

  test("returns 1 and lists every issue", () => {
    const root = gitRepo("main-bad-");
    writeFileSync(join(root, "Cargo.toml"), '[dependencies]\na = "1"\nb = "2"\n');
    const error = spyOn(console, "error").mockImplementation(() => undefined);
    expect(main(root)).toBe(1);
    expect(error).toHaveBeenCalledTimes(3);
    error.mockRestore();
  });
});
