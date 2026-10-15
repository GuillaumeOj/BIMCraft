import { describe, expect, test } from "bun:test";
import { join } from "node:path";

const GUARD = join(import.meta.dir, "..", ".claude", "hooks", "guard-bash.sh");

function exitCode(command: string): number {
  const input = JSON.stringify({ tool_input: { command } });
  return Bun.spawnSync(["sh", GUARD], { stdin: Buffer.from(input) }).exitCode;
}

describe("guard-bash.sh", () => {
  test.each([
    "git push",
    "cd x && git push origin main",
    "env git push origin main",
    "command git push",
    "/usr/bin/git push",
    "FOO=1 git push",
    "git -C . push",
    "git --no-pager push",
    "env gh pr create",
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
    "bun scripts/push-pr.ts --yes",
    "bun run ./scripts/push-pr.ts --stack",
    "cd .. && scripts/push-pr.ts",
  ])("blocks %s", (command) => {
    expect(exitCode(command)).toBe(2);
  });

  test.each([
    "git status",
    'git commit -s -m "fix(scripts): thing"',
    "git commit --amend --no-edit -s",
    "git config core.hooksPath",
    "gh pr view 1",
    "scripts/mark-reviewed.sh",
    "grep -rn push scripts",
    "git add scripts/push-pr.ts",
    "cd scripts && bun test push-pr",
  ])("allows %s", (command) => {
    expect(exitCode(command)).toBe(0);
  });
});
