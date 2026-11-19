import { describe, expect, spyOn, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { findHits, findStaleEntries, isBinary, main, parseAllowlist } from "./check-old-name";

const OLD = "BIM" + "Craft";
const ADR = "docs/dev/adr/0028-rename-to-fabrilo.md";

const file = (path: string, content = "") => ({ path, content });

describe("parseAllowlist", () => {
  test("reads the three forms and ignores comments and blank lines", () => {
    const entries = parseAllowlist(
      ["# header", "", "a/b.md # why", "dir/", `c.md | has ${OLD} here`, `* | org/${OLD}`].join("\n"),
    );
    expect(entries.map(({ path, phrase }) => [path, phrase])).toEqual([
      ["a/b.md", undefined],
      ["dir/", undefined],
      ["c.md", `has ${OLD} here`],
      ["*", `org/${OLD}`],
    ]);
  });
});

test("parseAllowlist rejects an entry with an empty phrase", () => {
  expect(() => parseAllowlist("a.md |")).toThrow("empty phrase");
});

describe("findHits", () => {
  test.each([OLD, OLD.toLowerCase(), OLD.toUpperCase()])("flags %s in a file", (name) => {
    const hits = findHits([file("README.md", `ok\nthe ${name} tool\n`)], []);
    expect(hits).toEqual([{ path: "README.md", line: 2, text: `the ${name} tool` }]);
  });

  test("flags the name in a path", () => {
    expect(findHits([file(`crates/foo/${OLD}.txt`)], [])).toEqual([{ path: `crates/foo/${OLD}.txt` }]);
  });

  test("passes a clean tree", () => {
    expect(findHits([file("README.md", "Fabrilo\n")], [])).toEqual([]);
  });

  test("ignores binary content but still checks the path", () => {
    const binary = { path: "logo.png", content: "", binary: true };
    expect(findHits([binary, { ...binary, path: `${OLD}.png` }], [])).toEqual([{ path: `${OLD}.png` }]);
  });

  test("the guard's own files are exempt", () => {
    expect(findHits([file("scripts/old-name-allowlist.txt", OLD)], [])).toEqual([]);
  });

  test("a file entry allows every line of that file only", () => {
    const entries = parseAllowlist("a.md");
    const files = [file("a.md", OLD), file("b.md", OLD)];
    expect(findHits(files, entries).map((h) => h.path)).toEqual(["b.md"]);
  });

  test("a directory entry allows the files below it only", () => {
    const entries = parseAllowlist("dir/");
    const files = [file("dir/a.md", OLD), file("dir/sub/b.md", OLD), file("dirx/c.md", OLD), file(`dir/${OLD}.md`)];
    expect(findHits(files, entries).map((h) => h.path)).toEqual(["dirx/c.md"]);
  });

  test("a phrase entry allows the matching lines of its file, not other lines or files", () => {
    const entries = parseAllowlist(`a.md | was called ${OLD}`);
    const files = [
      file("a.md", `it was called ${OLD} once\nand ${OLD} again\n`),
      file("b.md", `was called ${OLD}`),
    ];
    expect(findHits(files, entries)).toEqual([
      { path: "a.md", line: 2, text: `and ${OLD} again` },
      { path: "b.md", line: 1, text: `was called ${OLD}` },
    ]);
  });

  test("a phrase entry is case-sensitive and does not allow a path hit", () => {
    const entries = parseAllowlist(`${OLD}.md | x ${OLD}`);
    expect(findHits([file(`${OLD}.md`, `X ${OLD}`)], entries)).toHaveLength(2);
  });

  test("a * phrase entry allows the line in any file", () => {
    const entries = parseAllowlist(`* | org/${OLD}`);
    const files = [file("a.md", `https://x/org/${OLD}/issues`), file("b.md", `plain ${OLD}`)];
    expect(findHits(files, entries).map((h) => h.path)).toEqual(["b.md"]);
  });
});

describe("findStaleEntries", () => {
  test("reports entries that allow nothing", () => {
    const entries = parseAllowlist(`used.md\ngone.md\nused.md | nope\n* | org/${OLD}`);
    const files = [file("used.md", OLD)];
    expect(findStaleEntries(files, entries).map((e) => e.raw)).toEqual(["gone.md", "used.md | nope", `* | org/${OLD}`]);
  });

  test("keeps an entry that allows only a path hit", () => {
    expect(findStaleEntries([file(`d/${OLD}.md`)], parseAllowlist("d/"))).toEqual([]);
  });
});

test("isBinary looks for a NUL in the first 8 KiB only", () => {
  expect(isBinary(new Uint8Array([65, 0, 66]))).toBe(true);
  const late = new Uint8Array(9000).fill(65);
  late[8500] = 0;
  expect(isBinary(late)).toBe(false);
});

describe("main", () => {
  const repo = (files: Record<string, string | Uint8Array>, allowlist = "") => {
    const root = mkdtempSync(join(tmpdir(), "check-old-name-"));
    const all = { "scripts/old-name-allowlist.txt": allowlist, ...files };
    for (const [path, content] of Object.entries(all)) {
      mkdirSync(dirname(join(root, path)), { recursive: true });
      writeFileSync(join(root, path), content);
    }
    Bun.spawnSync(["git", "init", "-q"], { cwd: root });
    Bun.spawnSync(["git", "add", "-A"], { cwd: root });
    return root;
  };
  const run = (root: string, argv: string[] = []) => {
    const err = spyOn(console, "error").mockImplementation(() => {});
    const out = spyOn(console, "log").mockImplementation(() => {});
    const code = main(argv, root);
    const stderr = err.mock.calls.map((c) => c.join(" ")).join("\n");
    err.mockRestore();
    out.mockRestore();
    return { code, stderr };
  };

  test("passes on a clean tree, binary files included", () => {
    const root = repo({ "a.md": "Fabrilo", "b.bin": new Uint8Array([0, ...Buffer.from(OLD)]) });
    expect(run(root).code).toBe(0);
  });

  test("fails with the hit and the pointers to ADR 0028 and the allowlist", () => {
    const { code, stderr } = run(repo({ "a.md": `x\n${OLD} here` }));
    expect(code).toBe(1);
    expect(stderr).toContain(`a.md:2: ${OLD} here`);
    expect(stderr).toContain(ADR);
    expect(stderr).toContain("scripts/old-name-allowlist.txt");
  });

  test("names a file whose path holds the name", () => {
    expect(run(repo({ [`${OLD}.txt`]: "" })).stderr).toContain(`${OLD}.txt: (file name)`);
  });

  test("fails on a stale entry", () => {
    const { code, stderr } = run(repo({ "a.md": "Fabrilo" }, "gone.md"));
    expect(code).toBe(1);
    expect(stderr).toContain("stale allowlist entries");
    expect(stderr).toContain("gone.md");
  });

  test("with files, checks only those and skips the stale rule", () => {
    const root = repo({ "a.md": "Fabrilo", "b.md": OLD }, "gone.md");
    expect(run(root, ["a.md"]).code).toBe(0);
    expect(run(root, ["b.md"]).code).toBe(1);
    expect(run(root, ["deleted.md"]).code).toBe(0);
  });

  test("allowed hits pass", () => {
    expect(run(repo({ "a.md": `see org/${OLD}` }, `* | org/${OLD}`)).code).toBe(0);
  });
});
