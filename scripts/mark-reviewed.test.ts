import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gitEnv, git as gitIn } from "./check-pinned-deps";

const SCRIPT = join(import.meta.dir, "mark-reviewed.sh");

let dir: string;

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "mark-reviewed-"));
  gitIn(dir, "init", "-q");
  gitIn(dir, "-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-q", "--allow-empty", "-m", "x");
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

test("refuses a confirmation that is not typed at a terminal", () => {
  const result = Bun.spawnSync(["sh", SCRIPT], { cwd: dir, env: gitEnv(), stdin: Buffer.from("y\n"), stderr: "pipe" });
  expect(result.exitCode).toBe(1);
  expect(result.stderr.toString()).toContain("run this in a terminal");
  expect(existsSync(join(dir, ".git", "bimcraft-review-stamp"))).toBe(false);
});
