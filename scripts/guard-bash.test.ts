import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gitEnv, git } from "./check-pinned-deps";

const GUARD = join(import.meta.dir, "..", ".claude", "hooks", "guard-bash.sh");

// The guard reads the current branch, so every case runs in a repository of known branch, never
// in this checkout (CI tests main on main).
function repoOn(branch: string): string {
  const repo = mkdtempSync(join(tmpdir(), "guard-bash-"));
  git(repo, "init", "--quiet", "--initial-branch", branch);
  return repo;
}
const featureRepo = repoOn("feat");
const mainRepo = repoOn("main");
afterAll(() => {
  rmSync(featureRepo, { recursive: true });
  rmSync(mainRepo, { recursive: true });
});

function exitCode(command: string, cwd = featureRepo): number {
  const input = JSON.stringify({ tool_input: { command } });
  return Bun.spawnSync(["sh", GUARD], { cwd, env: gitEnv(), stdin: Buffer.from(input) }).exitCode;
}

describe("guard-bash.sh", () => {
  test.each([
    "cd x && git push origin main",
    "env git push origin main",
    'git push origin "main"',
    "git push origin 'HEAD:main'",
    "git push origin HEAD:main",
    "git push origin feat:refs/heads/main",
    "/usr/bin/git push -u origin main",
    "FOO=1 git push --force origin feat",
    "git -C . push -f origin feat",
    "git push -uf origin feat",
    "git --no-pager push --force-with-lease origin feat",
    "git push \\\n  --force origin feat",
    "git push origin +feat",
    'git push origin "+feat"',
    "git push origin :feat",
    "git push --delete origin feat",
    "git push -d origin feat",
    "git push --tags",
    "git push --mirror origin",
    "git push --all origin",
    "env gh pr create --title x",
    "gh pr create --title x && git branch -d old",
    "gh pr merge 1",
    "gh pr ready 1",
    "gh release create v1",
    "gh repo create a/b --public",
    "git commit -s --no-verify -m x",
    "cd ui && git commit -n -m x",
    "git commit -an -m x",
    "git -c core.hooksPath=/dev/null commit -m x",
    "git -c core.hookspath=/dev/null commit -m x",
    "git config core.hooksPath /dev/null",
    "git config --unset core.hooksPath",
    "git rev-parse HEAD | tee .git/bimcraft-review-stamp",
    "cp x .git/bimcraft-review-stamp",
  ])("blocks %s", (command) => {
    expect(exitCode(command)).toBe(2);
  });

  test.each([
    "git status",
    'git commit -s -m "fix(scripts): thing"',
    "git commit --amend --no-edit -s",
    "git config core.hooksPath",
    "gh pr view 1",
    "git push",
    "git push -u origin claude/feature",
    'git push -u origin "claude/feature"',
    "git push origin feat/main-fix",
    "git push --dry-run origin feat",
    "gh pr create --draft --title x --body-file pr.md",
    "gh pr checks 12 --watch",
    "gh run view 34 --log-failed",
    "scripts/mark-reviewed.sh",
    "grep -rn push scripts",
  ])("allows %s", (command) => {
    expect(exitCode(command)).toBe(0);
  });

  test.each(["git push", "git push origin feat:feat"])("blocks %s from a checkout of main", (command) => {
    expect(exitCode(command, mainRepo)).toBe(2);
  });
});
