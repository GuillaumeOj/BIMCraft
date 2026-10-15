import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, mock, spyOn, test } from "bun:test";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gitEnv, git as gitIn } from "./check-pinned-deps";
import {
  closedIssues,
  createArgs,
  editArgs,
  effective,
  emptyPr,
  linkArgs,
  main,
  makeRun,
  type PrOptions,
  parseArgs,
  type Result,
  type Run,
  stackBlock,
  unique,
  withIssues,
  withStackBlock,
} from "./push-pr";

const ok = (stdout = ""): Result => ({ code: 0, stdout, stderr: "" });
const pr = (overrides: Partial<PrOptions> = {}): PrOptions => ({ ...emptyPr(), ...overrides });

describe("parseArgs", () => {
  test("defaults", () => {
    const args = parseArgs([]);
    expect(args).toMatchObject({ stack: false, base: "main", remote: "origin", noPr: false, dryRun: false });
    expect(args.global).toEqual(pr());
  });

  test("options before --branch apply to all, after it to that branch", () => {
    const args = parseArgs([
      "--stack",
      "--label",
      "a,b",
      "--label=c",
      "--assignee",
      "@me",
      "--draft",
      "--branch",
      "one",
      "--title",
      "One",
      "--body",
      "Body",
      "--reviewer",
      "x, y",
      "--milestone",
      "M1",
      "--project",
      "P",
      "--branch=two",
      "--label",
      "d",
      "--base",
      "dev",
      "--remote",
      "up",
      "--force",
      "--no-pr",
      "--dry-run",
    ]);
    expect(args).toMatchObject({ stack: true, base: "dev", remote: "up", force: true, noPr: true, dryRun: true });
    expect(args.global).toEqual(pr({ labels: ["a", "b", "c"], assignees: ["@me"], draft: true }));
    expect(args.branches.get("one")).toEqual(
      pr({ title: "One", body: "Body", reviewers: ["x", "y"], milestone: "M1", projects: ["P"] }),
    );
    expect(args.branches.get("two")).toEqual(pr({ labels: ["d"] }));
  });

  test("--only lists the stack and implies --stack", () => {
    expect(parseArgs(["--only", "a,b"])).toMatchObject({ stack: true, only: ["a", "b"] });
  });

  test("help", () => {
    expect(parseArgs(["-h"]).help).toBe(true);
    expect(parseArgs(["--help"]).help).toBe(true);
  });

  test("--body-file must exist", () => {
    const dir = mkdtempSync(join(tmpdir(), "push-pr-"));
    const file = join(dir, "pr.md");
    writeFileSync(file, "# PR\n");
    expect(parseArgs(["--body-file", file]).global.bodyFile).toBe(file);
    expect(() => parseArgs(["--body-file", join(dir, "missing.md")])).toThrow("does not exist");
    rmSync(dir, { recursive: true });
  });

  test.each([
    [["--title"], "--title needs a value"],
    [["--stack=yes"], "--stack takes no value"],
    [["--frobnicate"], "unknown option --frobnicate"],
    [["extra"], "unknown option extra"],
    [["--branch", "a", "--branch", "a"], "--branch a is given twice"],
    [["--branch", "a", "--body", "x", "--body-file", "y"], "either --body or --body-file"],
  ])("rejects %p", (argv, message) => {
    expect(() => parseArgs(argv)).toThrow(message);
  });
});

describe("effective", () => {
  const global = pr({ title: "G", body: "global", labels: ["a"], assignees: ["@me"], milestone: "M", draft: true });

  test("without branch options, the global ones", () => {
    expect(effective(global, undefined)).toBe(global);
  });

  test("branch options override scalars and extend lists", () => {
    expect(effective(global, pr({ title: "B", bodyFile: "b.md", labels: ["a", "b"], reviewers: ["r"], projects: ["P"] }))).toEqual(
      pr({
        title: "B",
        body: undefined,
        bodyFile: "b.md",
        labels: ["a", "b"],
        assignees: ["@me"],
        reviewers: ["r"],
        milestone: "M",
        projects: ["P"],
        draft: true,
      }),
    );
  });

  test("keeps the global body when the branch gives none", () => {
    expect(effective(global, pr()).body).toBe("global");
  });
});

describe("gh arguments", () => {
  const fallback = { title: "feat: first", body: "first body" };

  test("create with defaults", () => {
    expect(createArgs({ branch: "b", base: "a", below: "a", pr: pr() }, fallback)).toEqual([
      "gh",
      "pr",
      "create",
      "--head",
      "b",
      "--base",
      "a",
      "--title",
      "feat: first",
      "--body",
      "first body",
    ]);
  });

  test("create with every option", () => {
    const options = pr({
      title: "T",
      bodyFile: "pr.md",
      labels: ["x", "y"],
      assignees: ["@me"],
      reviewers: ["r"],
      milestone: "M",
      projects: ["P"],
      draft: true,
    });
    expect(createArgs({ branch: "b", base: "main", below: "origin/main", pr: options }, fallback).slice(7)).toEqual([
      "--title",
      "T",
      "--body-file",
      "pr.md",
      "--label",
      "x,y",
      "--assignee",
      "@me",
      "--reviewer",
      "r",
      "--milestone",
      "M",
      "--project",
      "P",
      "--draft",
    ]);
  });

  test("edit only what changes", () => {
    const existing = { number: 7, url: "u", baseRefName: "main" };
    expect(editArgs({ branch: "b", base: "main", below: "origin/main", pr: pr() }, existing)).toBeUndefined();
    expect(editArgs({ branch: "b", base: "a", below: "a", pr: pr({ body: "B", labels: ["x"], projects: ["P"] }) }, existing)).toEqual([
      "gh",
      "pr",
      "edit",
      "7",
      "--base",
      "a",
      "--body",
      "B",
      "--add-label",
      "x",
      "--add-project",
      "P",
    ]);
  });
});

describe("stack navigation", () => {
  const block = stackBlock([12, 13, 14], "main", 13);

  test("lists the stack top first and marks the current pull request", () => {
    expect(block).toContain("- #14\n- **#13 ← this pull request**\n- #12\n- `main`");
  });

  test("is appended once, then replaced in place", () => {
    expect(withStackBlock("", block)).toBe(block);
    const body = withStackBlock("Description\n\n", block);
    expect(body).toBe(`Description\n\n${block}`);
    const updated = withStackBlock(`${body}\nAfter`, stackBlock([12, 13], "main", 12));
    expect(updated).toBe(`Description\n\n${stackBlock([12, 13], "main", 12)}\nAfter`);
  });
});

describe("issues", () => {
  test("finds the issues a description closes", () => {
    expect(closedIssues("Closes #12. fixes #3, Resolved: #40\nRESOLVES #12 and fixed #5")).toEqual([12, 3, 40, 5]);
  });

  test("ignores mentions and the template placeholder", () => {
    expect(closedIssues("Closes #\nSee #7, closes#8, encloses #9, #10")).toEqual([]);
  });

  const issue = (number: number, milestone: string | null, ...projects: string[]) => ({
    number,
    id: `I_${number}`,
    milestone: milestone === null ? null : { title: milestone },
    projectItems: projects.map((title) => ({ title })),
  });
  const open = (milestone: string | null, ...projects: string[]) => ({
    number: 7,
    url: "u",
    baseRefName: "main",
    body: "",
    ...issue(7, milestone, ...projects),
  });

  test("give the pull request the first milestone and every project of its issues", () => {
    expect(withIssues(pr(), [issue(1, null, "A"), issue(2, "M2", "A", "B"), issue(3, "M3")])).toEqual(
      pr({ milestone: "M2", projects: ["A", "B"] }),
    );
    expect(withIssues(pr(), [])).toEqual(pr());
  });

  test("never replace what the pull request already has", () => {
    expect(withIssues(pr(), [issue(1, "M", "A", "B")], open("Own", "A"))).toEqual(pr({ projects: ["B"] }));
  });

  test("never replace the milestone and projects given on the command line", () => {
    const given = pr({ milestone: "Own", projects: ["P"] });
    expect(withIssues(given, [issue(1, "M", "A")], open("Old"))).toEqual(given);
  });

  test("link a pull request to an issue through GraphQL", () => {
    expect(linkArgs("I_1", "PR_2").slice(-4)).toEqual(["-f", "issue=I_1", "-f", "prs[]=PR_2"]);
  });
});

type FakePr = {
  number: number;
  head: string;
  base: string;
  title: string;
  body: string;
  state: string;
  args: string[];
  milestone?: string;
  projects: string[];
  /** Issues linked by hand (the Development field). */
  links: number[];
};

/** In-memory GitHub answering the `gh` calls push-pr makes. */
class FakeGitHub {
  prs: FakePr[] = [];
  issues = new Map<number, { milestone?: string; projects: string[] }>();
  signedIn = true;
  /** False while GitHub has not yet read the closing keywords of a new pull request. */
  keywordLinks = true;

  handle(cmd: string[], input: string | undefined): Result {
    const flag = (name: string): string | undefined => {
      const i = cmd.indexOf(name);
      return i === -1 ? undefined : cmd[i + 1];
    };
    const field = (name: string): string | undefined =>
      cmd.find((arg) => arg.startsWith(`${name}=`))?.slice(name.length + 1);
    const action = cmd.slice(1, 3).join(" ");
    if (action === "auth status") return { code: this.signedIn ? 0 : 1, stdout: "", stderr: "" };
    if (action === "pr list") {
      const latest = this.prs.filter((p) => p.head === flag("--head")).slice(-1);
      return ok(
        JSON.stringify(
          latest.map((p) => ({
            number: p.number,
            url: this.url(p.number),
            baseRefName: p.base,
            state: p.state,
            body: p.body,
            ...planning(p.milestone, p.projects),
          })),
        ),
      );
    }
    if (action === "pr create") {
      const number = 100 + this.prs.length;
      const body = flag("--body-file") === undefined ? (flag("--body") as string) : `file:${flag("--body-file")}`;
      const [head, base, title] = [flag("--head"), flag("--base"), flag("--title")] as string[];
      const projects = flag("--project")?.split(",") ?? [];
      this.prs.push({ number, head, base, title, body, state: "OPEN", args: cmd, milestone: flag("--milestone"), projects, links: [] });
      return ok(`Creating pull request\n${this.url(number)}\n`);
    }
    if (action === "issue view") {
      const issue = this.issues.get(Number(cmd[3]));
      if (issue === undefined) return { code: 1, stdout: "", stderr: `no issue #${cmd[3]}` };
      return ok(JSON.stringify({ number: Number(cmd[3]), id: `I_${cmd[3]}`, ...planning(issue.milestone, issue.projects) }));
    }
    if (action === "repo view") return ok("main\n");
    if (action === "api graphql") {
      const linked = this.prs.find((p) => `PR_${p.number}` === field("prs[]"));
      linked?.links.push(Number(field("issue")?.slice(2)));
      return linked === undefined ? { code: 1, stdout: "", stderr: "no such node" } : ok();
    }
    const found = this.prs.find((p) => p.number === Number(cmd[3]));
    if (found === undefined) return { code: 1, stdout: "", stderr: "no such pull request" };
    if (action === "pr view") {
      // GitHub links the issues a pull request closes by itself only when it targets the default branch.
      const closing = unique([...(found.base === "main" && this.keywordLinks ? closedIssues(found.body) : []), ...found.links]);
      const closingIssuesReferences = closing.map((number) => ({ number }));
      return ok(JSON.stringify({ id: `PR_${found.number}`, body: found.body, closingIssuesReferences }));
    }
    found.base = flag("--base") ?? found.base;
    found.title = flag("--title") ?? found.title;
    found.body = flag("--body-file") === "-" ? (input as string) : (flag("--body") ?? found.body);
    found.milestone = flag("--milestone") ?? found.milestone;
    found.projects = unique([...found.projects, ...(flag("--add-project")?.split(",") ?? [])]);
    found.args.push(...cmd);
    return ok();
  }

  url(number: number): string {
    return `https://github.com/owner/repo/pull/${number}`;
  }
}

const planning = (milestone: string | undefined, projects: string[]) => ({
  milestone: milestone === undefined ? null : { title: milestone },
  projectItems: projects.map((title) => ({ title })),
});

describe("main", () => {
  let dir: string;
  let remote: string;
  let gh: FakeGitHub;
  let calls: string[][];
  let checkPrFails: boolean;
  let output: string[];
  let errors: string[];
  let reviewed: boolean;

  let template: string;

  const git = (...args: string[]): string => gitIn(dir, ...args).trim();
  const stampFile = (): string => join(dir, ".git", "bimcraft-review-stamp");
  const commit = (message: string): void => {
    writeFileSync(join(dir, `${message.replace(/\W/g, "_")}.txt`), message);
    git("add", "-A");
    git("-c", "commit.gpgsign=false", "commit", "-q", "-m", message);
  };
  const run: Run = (cmd, options = {}) => {
    calls.push(cmd);
    if (cmd[0] === "gh") return gh.handle(cmd, options.input);
    if (cmd[0]?.endsWith("check-pr.sh")) return { code: checkPrFails ? 1 : 0, stdout: "", stderr: "" };
    // The real script needs a terminal; `reviewed` stands for the user's answer to its question.
    if (cmd[0]?.endsWith("mark-reviewed.sh")) {
      if (reviewed) writeFileSync(stampFile(), `${git("rev-parse", "HEAD")}\n`);
      return { code: reviewed ? 0 : 1, stdout: "", stderr: "" };
    }
    return makeRun(dir, gitEnv())(cmd, { ...options, inherit: false });
  };
  const ship = (...argv: string[]): number => main(argv, run);
  const ran = (command: string): string[][] => calls.filter((cmd) => cmd.slice(0, 3).join(" ") === command);
  const wouldRun = (command: string): string[] => output.filter((line) => line.startsWith(`would run: ${command}`));
  const stamp = (): string => {
    try {
      return readFileSync(stampFile(), "utf8").trim();
    } catch {
      return "";
    }
  };
  const remoteBranches = (): string[] =>
    gitIn(remote, "for-each-ref", "--format=%(refname:short)", "refs/heads").split("\n").filter(Boolean);

  // One repository with a pushed main, copied for each test (the remote URL is relative).
  beforeAll(() => {
    template = mkdtempSync(join(tmpdir(), "push-pr-"));
    gitIn(template, "init", "-q", "--bare", "remote.git");
    gitIn(template, "init", "-q", "-b", "main", "work");
    dir = join(template, "work");
    git("config", "user.name", "Test");
    git("config", "user.email", "test@example.com");
    git("remote", "add", "origin", "../remote.git");
    commit("chore: initial");
    git("push", "-q", "origin", "main");
  });

  afterAll(() => rmSync(template, { recursive: true, force: true }));

  beforeEach(() => {
    const root = mkdtempSync(join(tmpdir(), "push-pr-"));
    cpSync(template, root, { recursive: true });
    remote = join(root, "remote.git");
    dir = join(root, "work");
    gh = new FakeGitHub();
    calls = [];
    checkPrFails = false;
    reviewed = true;
    output = [];
    errors = [];
    spyOn(console, "log").mockImplementation((line: string) => output.push(line));
    spyOn(console, "error").mockImplementation((line: string) => errors.push(line));
  });

  afterEach(() => {
    rmSync(join(dir, ".."), { recursive: true, force: true });
    mock.restore();
  });

  const featureBranch = (...messages: string[]): void => {
    git("checkout", "-q", "-b", "feature");
    for (const message of messages) commit(message);
  };
  const stackOfThree = (): void => {
    for (const branch of ["one", "two", "three"]) {
      git("checkout", "-q", "-b", branch);
      commit(`feat: ${branch}`);
    }
  };

  test("prints the help", () => {
    expect(ship("--help")).toBe(0);
    expect(output.join("\n")).toContain("Usage: bun scripts/push-pr.ts");
  });

  test("ships a single branch: stamp, push, pull request", () => {
    featureBranch("feat: add thing");
    expect(ship("--title", "Add thing", "--body", "Why", "--label", "geometry", "--assignee", "@me")).toBe(0);
    expect(stamp()).toBe(git("rev-parse", "HEAD"));
    expect(remoteBranches()).toContain("feature");
    expect(gh.prs).toHaveLength(1);
    expect(gh.prs[0]).toMatchObject({ head: "feature", base: "main", title: "Add thing", body: "Why" });
    expect(gh.prs[0]?.args).toContain("geometry");
    expect(output).toContain(`  ${gh.url(100)}`);
  });

  test("defaults the title and description to the first commit", () => {
    featureBranch("feat: first\n\nWhy it matters.\n\nCo-Authored-By: A <a@example.com>\nSigned-off-by: T <t@example.com>", "fix: second");
    expect(ship()).toBe(0);
    expect(gh.prs[0]).toMatchObject({ title: "feat: first", body: "Why it matters." });
  });

  test("ships a whole stack, each pull request based on the branch below", () => {
    stackOfThree();
    expect(ship("--stack", "--assignee", "@me", "--branch", "one", "--title", "One", "--branch", "three", "--body", "Top")).toBe(0);
    expect(remoteBranches()).toEqual(["main", "one", "three", "two"]);
    expect(gh.prs.map((p) => [p.head, p.base, p.title])).toEqual([
      ["one", "main", "One"],
      ["two", "one", "feat: two"],
      ["three", "two", "feat: three"],
    ]);
    expect(gh.prs[2]?.body).toStartWith("Top\n\n<!-- push-pr:stack -->");
    expect(gh.prs[0]?.body).toContain("- #102\n- #101\n- **#100 ← this pull request**\n- `main`");
  });

  test("updates the existing pull requests of a restacked stack instead of opening new ones", () => {
    stackOfThree();
    expect(ship("--stack")).toBe(0);
    const bodies = gh.prs.map((p) => p.body);
    (gh.prs[1] as { base: string }).base = "main";
    git("commit", "-q", "--amend", "-m", "feat: three, amended");
    calls = [];
    expect(ship("--stack", "--force", "--label", "core")).toBe(0);
    expect(gh.prs).toHaveLength(3);
    expect(gh.prs[1]?.base).toBe("one");
    expect(gh.prs.map((p) => p.body)).toEqual(bodies);
    expect(calls).toContainEqual(["git", "push", "--atomic", "--force-with-lease", "--force-if-includes", "-u", "origin", "one", "two", "three"]);
  });

  test("--only ships the listed branches", () => {
    stackOfThree();
    git("push", "-q", "origin", "one");
    expect(ship("--only", "two,three", "--base", "one")).toBe(0);
    expect(gh.prs.map((p) => [p.head, p.base])).toEqual([
      ["two", "one"],
      ["three", "two"],
    ]);
  });

  test("opens a new pull request when the previous one was closed", () => {
    featureBranch("feat: add thing");
    expect(ship()).toBe(0);
    (gh.prs[0] as { state: string }).state = "CLOSED";
    expect(ship()).toBe(0);
    expect(gh.prs.map((p) => p.head)).toEqual(["feature", "feature"]);
  });

  test("--no-pr stamps and pushes only", () => {
    featureBranch("feat: add thing");
    expect(ship("--no-pr")).toBe(0);
    expect(remoteBranches()).toContain("feature");
    expect(calls.some((cmd) => cmd[0] === "gh")).toBe(false);
  });

  test("--dry-run changes nothing", () => {
    stackOfThree();
    expect(ship("--stack", "--dry-run")).toBe(0);
    expect(stamp()).toBe("");
    expect(remoteBranches()).toEqual(["main"]);
    expect(gh.prs).toHaveLength(0);
    expect(output).toContain("would run: git push --atomic -u origin one two three");
    expect(wouldRun("gh pr create --head two --base one")).toHaveLength(1);
  });

  test("--dry-run on an open stack shows the navigation edits", () => {
    stackOfThree();
    expect(ship("--stack")).toBe(0);
    for (const p of gh.prs) p.body = "";
    expect(ship("--stack", "--dry-run")).toBe(0);
    expect(wouldRun("gh pr edit")).toHaveLength(3);
  });

  describe("issues closed by the pull requests", () => {
    beforeEach(() => {
      gh.issues.set(1, { milestone: "Phase 0", projects: ["BIMCraft"] });
      gh.issues.set(2, { milestone: "Phase 0", projects: ["BIMCraft"] });
      gh.issues.set(3, { projects: [] });
    });

    test("give a stack their milestone and project, and link the upper pull requests", () => {
      stackOfThree();
      const argv = ["--stack", "--branch", "one", "--body", "Closes #1", "--branch", "two", "--body", "Fixes #2"];
      expect(ship(...argv, "--branch", "three", "--body", "Closes #3")).toBe(0);
      expect(gh.prs.map((p) => [p.milestone, p.projects, p.links])).toEqual([
        ["Phase 0", ["BIMCraft"], []],
        ["Phase 0", ["BIMCraft"], [2]],
        [undefined, [], [3]],
      ]);
      calls = [];
      expect(ship("--stack")).toBe(0);
      expect(ran("gh api graphql")).toEqual([]);
      expect(ran("gh pr edit")).toEqual([]);
    });

    test("give a single pull request into main its milestone and project; GitHub links it", () => {
      featureBranch("feat: add thing\n\nCloses #1");
      expect(ship()).toBe(0);
      expect(gh.prs[0]).toMatchObject({ milestone: "Phase 0", projects: ["BIMCraft"], links: [] });
      expect(ran("gh api graphql")).toEqual([]);
    });

    test("never replace the milestone given on the command line", () => {
      featureBranch("feat: add thing\n\nCloses #1");
      expect(ship("--milestone", "Phase 1", "--project", "Other")).toBe(0);
      expect(gh.prs[0]).toMatchObject({ milestone: "Phase 1", projects: ["Other"] });
    });

    test("never link by hand a pull request into the default branch", () => {
      featureBranch("feat: add thing\n\nCloses #1");
      gh.keywordLinks = false;
      expect(ship()).toBe(0);
      expect(gh.prs[0]).toMatchObject({ links: [] });
      expect(ran("gh api graphql")).toEqual([]);
    });

    test("show in --dry-run the milestone and project a new pull request gets", () => {
      featureBranch("feat: add thing\n\nCloses #1");
      expect(ship("--dry-run")).toBe(0);
      expect(wouldRun("gh pr create")[0]).toEndWith("--milestone 'Phase 0' --project BIMCraft");
    });

    test("stop before stamping when an issue cannot be read", () => {
      stackOfThree();
      expect(ship("--stack", "--branch", "two", "--body", "Closes #99")).toBe(1);
      expect(errors.join("\n")).toContain("gh issue view 99 --json number,id,milestone,projectItems failed: no issue #99");
      expect(stamp()).toBe("");
      expect(remoteBranches()).toEqual(["main"]);
    });

    test("change nothing when the description closes no issue", () => {
      featureBranch("feat: add thing\n\nCloses #");
      expect(ship()).toBe(0);
      expect(ran("gh issue view")).toEqual([]);
      expect(gh.prs[0]).toMatchObject({ milestone: undefined, projects: [] });
    });

    test("are only shown by --dry-run", () => {
      stackOfThree();
      expect(ship("--stack")).toBe(0);
      for (const p of gh.prs) p.body = `Closes #${p.number - 99}\n\n${p.body}`;
      calls = [];
      expect(ship("--stack", "--dry-run")).toBe(0);
      expect(gh.prs.map((p) => [p.milestone, p.links])).toEqual([
        [undefined, []],
        [undefined, []],
        [undefined, []],
      ]);
      expect(wouldRun("gh pr edit").filter((line) => line.includes("--milestone"))).toHaveLength(2);
      expect(wouldRun("gh api graphql")).toHaveLength(2);
    });
  });

  test("passes the no-user-docs label to the commit checks", () => {
    featureBranch("chore: tooling");
    let env: Record<string, string> | undefined;
    const spy: Run = (cmd, options = {}) => {
      if (cmd[0]?.endsWith("check-pr.sh")) env = options.env;
      return run(cmd, options);
    };
    expect(main(["--label", "no-user-docs"], spy)).toBe(0);
    expect(env).toEqual({ NO_USER_DOCS: "true" });
  });

  test("pushes nothing when the stamp is refused", () => {
    featureBranch("feat: add thing");
    reviewed = false;
    expect(ship()).toBe(1);
    expect(errors).toContain("push-pr: not stamped; nothing pushed");
    expect(stamp()).toBe("");
    expect(remoteBranches()).toEqual(["main"]);
    expect(gh.prs).toHaveLength(0);
  });

  describe("refuses", () => {
    const refuses = (message: string, ...argv: string[]): void => {
      expect(ship(...argv)).toBe(1);
      expect(errors.join("\n")).toContain(message);
      expect(stamp()).toBe("");
    };

    test("a dirty tree", () => {
      featureBranch();
      writeFileSync(join(dir, "dirty.txt"), "x");
      refuses("working tree is not clean");
    });

    test("the base branch", () => refuses("main is the base branch"));

    test("a detached HEAD", () => {
      git("checkout", "-q", "--detach");
      refuses("HEAD is detached");
    });

    test("when gh is signed out", () => {
      featureBranch("feat: add thing");
      gh.signedIn = false;
      refuses("gh is not signed in");
    });

    test("a branch without commits", () => {
      featureBranch();
      refuses("feature has no commits of its own on top of origin/main");
      refuses("feature has no commits of its own on top of origin/main", "--stack");
    });

    test("branches that are not a chain", () => {
      stackOfThree();
      refuses("the top of the stack must be the checked-out branch", "--only", "three,two");
      refuses("one is not built on two", "--only", "two,one,three");
    });

    test("a stack it cannot order", () => {
      stackOfThree();
      git("branch", "twin", "two");
      refuses("cannot order", "--stack");
    });

    test("options for a branch outside the stack", () => {
      stackOfThree();
      refuses("--branch nope is not in the stack: one, two, three", "--stack", "--branch", "nope");
    });

    test("a shared title in a stack", () => {
      stackOfThree();
      refuses("give --title, --body and --body-file after --branch", "--stack", "--title", "Same");
    });

    test("failing commit checks", () => {
      featureBranch("feat: add thing");
      checkPrFails = true;
      refuses("commit checks failed for feature");
    });

    test("a failed push", () => {
      featureBranch("feat: add thing");
      git("remote", "set-url", "--push", "origin", join(dir, "missing.git"));
      expect(ship()).toBe(1);
      expect(errors.join("\n")).toContain("git push --atomic -u origin feature failed");
    });

    test("bad arguments", () => refuses("unknown option --nope", "--nope"));

    test("a stacked branch shipped alone onto main", () => {
      stackOfThree();
      git("branch", "copy");
      refuses("three is built on one, two, not yet in origin/main; use --stack, or --base <branch>");
    });

    test("a missing branch in --only", () => {
      stackOfThree();
      refuses("there is no local branch tow", "--only", "one,tow,three");
    });

    test("a branch whose pull request was already merged, before stamping", () => {
      stackOfThree();
      expect(ship("--stack")).toBe(0);
      (gh.prs[0] as { state: string }).state = "MERGED";
      const stamped = stamp();
      commit("feat: more");
      expect(ship("--stack")).toBe(1);
      expect(errors.join("\n")).toContain("one was already merged in #100; rebase the branches above it onto main and delete it");
      expect(stamp()).toBe(stamped);
      expect(gh.prs).toHaveLength(3);
    });
  });
});
