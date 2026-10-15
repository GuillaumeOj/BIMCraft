#!/usr/bin/env bun
// Stamps HEAD as reviewed, pushes the branch (or the whole stack of branches ending at HEAD) and
// opens or updates one pull request per branch, each based on the branch below it (AGENTS.md §1).
// Only the user can ship: the stamp asks them to confirm the review at a terminal. Run with --help for
// the options.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const USAGE = `Usage: bun scripts/push-pr.ts [options] [--branch <name> [pr options]]...

Stamps HEAD as reviewed (scripts/mark-reviewed.sh), pushes, then opens or updates the pull request.
Run it only after /code-review and /simplify (or equivalent reviews) on these changes; the stamp
asks you to confirm that at the terminal.

Stack options (anywhere):
  --stack               ship every local branch between <remote>/<base> and HEAD, bottom first;
                        each PR is based on the branch below it
  --only a,b,c          ship exactly these branches, bottom to top (the last one must be checked out)
  --base <branch>       base of the bottom pull request (default: main)
  --remote <name>       remote to push to (default: origin)
  --force               push with --force-with-lease --force-if-includes (after a rebase or a restack)
  --no-pr               stamp and push only
  --dry-run             print the commands that would change anything, run none, stamp nothing
  -h, --help            show this help

Pull request options (before any --branch: every PR; after --branch <name>: that PR only):
  --title <text>        title (default: subject of the branch's first commit)
  --body <text>         description (default: body of the branch's first commit)
  --body-file <file>    description read from a Markdown file
  --label a,b           labels (repeatable)
  --assignee a,b        assignees, @me for yourself (repeatable)
  --reviewer a,b        reviewers (repeatable)
  --milestone <name>    milestone
  --project a,b         projects, by title (repeatable)
  --draft               open as a draft

In a stack of several branches, give --title, --body and --body-file per branch.
An existing open pull request is updated: base fixed, labels/assignees/reviewers added, title and
description replaced only when given.
Each pull request also takes the milestone (unless it has one) and the projects of the issues its
description closes ("Closes #123"); --milestone and --project replace them. It is linked to those
issues too: GitHub does that by itself only for pull requests into the default branch.

Examples:
  bun scripts/push-pr.ts --title "feat(geom): add predicates" --body-file pr.md --label geometry --assignee @me
  bun scripts/push-pr.ts --stack --assignee @me \\
    --branch geom-predicates --title "feat(geom): add predicates" --body-file geom.md \\
    --branch geom-offsets --title "feat(geom): add offsets" --body "Builds on the predicates."`;

const SCRIPTS = import.meta.dir;
const STACK_START = "<!-- push-pr:stack -->";
const STACK_END = "<!-- /push-pr:stack -->";

/** Options of one pull request. */
export type PrOptions = {
  title?: string;
  body?: string;
  bodyFile?: string;
  labels: string[];
  assignees: string[];
  reviewers: string[];
  milestone?: string;
  projects: string[];
  draft: boolean;
};

export type Args = {
  help: boolean;
  stack: boolean;
  only: string[];
  base: string;
  remote: string;
  force: boolean;
  noPr: boolean;
  dryRun: boolean;
  /** Options given before any --branch. */
  global: PrOptions;
  /** Options given after --branch <name>, by branch. */
  branches: Map<string, PrOptions>;
};

/**
 * A branch to push, the branch its pull request merges into, the ref its own commits start from
 * (`base`, or its remote copy for the bottom branch) and that request's options.
 */
export type Planned = { branch: string; base: string; below: string; pr: PrOptions };

export type Pr = { number: number; url: string; baseRefName: string };
type Titled = { title: string };
/** Milestone and projects of an issue or pull request, as `gh` gives them. */
type Planning = { milestone: Titled | null; projectItems: Titled[] };
/** An open pull request. */
export type OpenPr = Pr & Planning & { body: string };
type FoundPr = OpenPr & { state: "OPEN" | "CLOSED" | "MERGED" };
/** An issue a pull request closes. */
export type Issue = Planning & { number: number; id: string };
type PrView = { id: string; body: string; closingIssuesReferences: { number: number }[] };

export type Result = { code: number; stdout: string; stderr: string };
export type RunOptions = { input?: string; env?: Record<string, string>; inherit?: boolean };
export type Run = (cmd: string[], options?: RunOptions) => Result;

export const emptyPr = (): PrOptions => ({ labels: [], assignees: [], reviewers: [], projects: [], draft: false });

// Options that only set a field, by kind.
const FLAGS = {
  "-h": "help",
  "--help": "help",
  "--stack": "stack",
  "--force": "force",
  "--no-pr": "noPr",
  "--dry-run": "dryRun",
} as const;
const LISTS = { "--label": "labels", "--assignee": "assignees", "--reviewer": "reviewers", "--project": "projects" } as const;
const TEXTS = {
  "--title": "title",
  "--body": "body",
  "--body-file": "bodyFile",
  "--milestone": "milestone",
} as const;
const lookup = <T extends object>(table: T, key: string): T[keyof T] | undefined =>
  Object.hasOwn(table, key) ? table[key as keyof T] : undefined;
const list = (value: string): string[] =>
  value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
export const unique = <T>(items: T[]): T[] => [...new Set(items)];

export function parseArgs(argv: string[]): Args {
  const args: Args = {
    help: false,
    stack: false,
    only: [],
    base: "main",
    remote: "origin",
    force: false,
    noPr: false,
    dryRun: false,
    global: emptyPr(),
    branches: new Map(),
  };
  let scope = args.global;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i] as string;
    const eq = arg.startsWith("--") ? arg.indexOf("=") : -1;
    const name = eq === -1 ? arg : arg.slice(0, eq);
    const inline = eq === -1 ? undefined : arg.slice(eq + 1);
    const value = (): string => {
      const next = inline ?? argv[++i];
      if (next === undefined) throw new Error(`${name} needs a value`);
      return next;
    };
    const flag = (): true => {
      if (inline !== undefined) throw new Error(`${name} takes no value`);
      return true;
    };
    const flagName = lookup(FLAGS, name);
    const listName = lookup(LISTS, name);
    const textName = lookup(TEXTS, name);
    if (flagName !== undefined) args[flagName] = flag();
    else if (listName !== undefined) scope[listName].push(...list(value()));
    else if (textName !== undefined) scope[textName] = value();
    else if (name === "--draft") scope.draft = flag();
    else if (name === "--base") args.base = value();
    else if (name === "--remote") args.remote = value();
    else if (name === "--only") {
      args.only = list(value());
      args.stack = true;
    } else if (name === "--branch") {
      const branch = value();
      if (args.branches.has(branch)) throw new Error(`--branch ${branch} is given twice`);
      scope = emptyPr();
      args.branches.set(branch, scope);
    } else throw new Error(`unknown option ${arg} (see --help)`);
  }
  for (const options of [args.global, ...args.branches.values()]) {
    if (options.body !== undefined && options.bodyFile !== undefined) {
      throw new Error("give either --body or --body-file, not both");
    }
    if (options.bodyFile !== undefined && !existsSync(options.bodyFile)) {
      throw new Error(`--body-file ${options.bodyFile} does not exist`);
    }
  }
  return args;
}

/** The options of one branch's pull request: its own on top of the global ones. */
export function effective(global: PrOptions, own: PrOptions | undefined): PrOptions {
  if (own === undefined) return global;
  const ownBody = own.body !== undefined || own.bodyFile !== undefined;
  return {
    title: own.title ?? global.title,
    body: ownBody ? own.body : global.body,
    bodyFile: ownBody ? own.bodyFile : global.bodyFile,
    labels: unique([...global.labels, ...own.labels]),
    assignees: unique([...global.assignees, ...own.assignees]),
    reviewers: unique([...global.reviewers, ...own.reviewers]),
    milestone: own.milestone ?? global.milestone,
    projects: unique([...global.projects, ...own.projects]),
    draft: global.draft || own.draft,
  };
}

const joined = (option: string, items: string[]): string[] => (items.length > 0 ? [option, items.join(",")] : []);
const optional = (option: string, value: string | undefined): string[] => (value === undefined ? [] : [option, value]);

/** `gh pr create` for a branch, with `fallback` as title and description when none was given. */
export function createArgs(planned: Planned, fallback: { title: string; body: string }): string[] {
  const pr = planned.pr;
  return [
    "gh",
    "pr",
    "create",
    "--head",
    planned.branch,
    "--base",
    planned.base,
    "--title",
    pr.title ?? fallback.title,
    ...(pr.bodyFile === undefined ? ["--body", pr.body ?? fallback.body] : ["--body-file", pr.bodyFile]),
    ...joined("--label", pr.labels),
    ...joined("--assignee", pr.assignees),
    ...joined("--reviewer", pr.reviewers),
    ...optional("--milestone", pr.milestone),
    ...joined("--project", pr.projects),
    ...(pr.draft ? ["--draft"] : []),
  ];
}

/** `gh pr edit` bringing an existing pull request in line, or undefined when nothing changes. */
export function editArgs(planned: Planned, existing: Pr): string[] | undefined {
  const pr = planned.pr;
  const changes = [
    ...(existing.baseRefName === planned.base ? [] : ["--base", planned.base]),
    ...optional("--title", pr.title),
    ...optional("--body", pr.body),
    ...optional("--body-file", pr.bodyFile),
    ...joined("--add-label", pr.labels),
    ...joined("--add-assignee", pr.assignees),
    ...joined("--add-reviewer", pr.reviewers),
    ...optional("--milestone", pr.milestone),
    ...joined("--add-project", pr.projects),
  ];
  return changes.length > 0 ? ["gh", "pr", "edit", String(existing.number), ...changes] : undefined;
}

/** The issues a pull request description closes with a GitHub closing keyword, in order. */
export function closedIssues(body: string): number[] {
  const matches = body.matchAll(/\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?):?\s+#(\d+)\b/gi);
  return unique([...matches].map((match) => Number(match[1])));
}

/**
 * `pr` with the milestone and projects of the issues it closes as defaults. The command line wins, and
 * an open pull request keeps the milestone and projects it already has.
 */
export function withIssues(pr: PrOptions, issues: Issue[], open?: OpenPr): PrOptions {
  const fromIssue = issues.map((issue) => issue.milestone?.title).find((title) => title !== undefined);
  const has = new Set(open?.projectItems.map((item) => item.title));
  const fromIssues = unique(issues.flatMap((issue) => issue.projectItems.map((item) => item.title)));
  return {
    ...pr,
    milestone: pr.milestone ?? (open?.milestone ? undefined : fromIssue),
    projects: pr.projects.length > 0 ? pr.projects : fromIssues.filter((title) => !has.has(title)),
  };
}

/** `gh api graphql` linking a pull request to an issue it closes, as the sidebar's Development field does. */
export function linkArgs(issueId: string, prId: string): string[] {
  const query =
    "mutation($issue: ID!, $prs: [ID!]!) { addCloseIssueReferences(input: {issueId: $issue, pullRequestIds: $prs}) { clientMutationId } }";
  return ["gh", "api", "graphql", "-f", `query=${query}`, "-f", `issue=${issueId}`, "-f", `prs[]=${prId}`];
}

/** The stack navigation shown in each pull request of a stack, top first. */
export function stackBlock(numbers: number[], base: string, current: number): string {
  const lines = [...numbers].reverse().map((n) => (n === current ? `- **#${n} ← this pull request**` : `- #${n}`));
  return [STACK_START, "**Stacked pull requests** (merge from the bottom up):", "", ...lines, `- \`${base}\``, STACK_END].join(
    "\n",
  );
}

/** `body` with its stack navigation replaced by `block`, or with `block` appended. */
export function withStackBlock(body: string, block: string): string {
  const start = body.indexOf(STACK_START);
  const end = body.indexOf(STACK_END);
  if (start !== -1 && end > start) return body.slice(0, start) + block + body.slice(end + STACK_END.length);
  const text = body.trimEnd();
  return text === "" ? block : `${text}\n\n${block}`;
}

const quote = (arg: string): string => (/^[\w@%+=:,./-]+$/.test(arg) ? arg : `'${arg.replaceAll("'", `'\\''`)}'`);

/** Runs commands in `cwd`; `inherit` lets the user see the output (and answer prompts). */
export function makeRun(cwd: string, env: Record<string, string | undefined> = process.env): Run {
  return (cmd, options = {}) => {
    const io = options.inherit ? "inherit" : "pipe";
    const stdin = options.input === undefined ? (options.inherit ? "inherit" : "ignore") : Buffer.from(options.input);
    const result = Bun.spawnSync(cmd, { cwd, env: { ...env, ...options.env }, stdin, stdout: io, stderr: io });
    return { code: result.exitCode ?? 1, stdout: result.stdout?.toString() ?? "", stderr: result.stderr?.toString() ?? "" };
  };
}

/** Runs `cmd` and returns its output; throws when it fails. */
function must(run: Run, cmd: string[], options?: RunOptions): string {
  const result = run(cmd, options);
  if (result.code !== 0) throw new Error(`${cmd.map(quote).join(" ")} failed${result.stderr ? `: ${result.stderr.trim()}` : ""}`);
  return result.stdout.trim();
}

/** Runs a `gh … --json` command and parses its output. */
const ghJson = <T>(run: Run, cmd: string[]): T => JSON.parse(must(run, cmd)) as T;

const isAncestor = (run: Run, ancestor: string, of: string): boolean =>
  run(["git", "merge-base", "--is-ancestor", ancestor, of]).code === 0;
const ahead = (run: Run, base: string, branch: string): number =>
  Number(must(run, ["git", "rev-list", "--count", `${base}..${branch}`]));

/**
 * Local branches between `baseRef` and HEAD, bottom first. Other branches pointing at HEAD (a backup
 * copy, say) are left out.
 */
export function detectStack(run: Run, baseRef: string, current: string): string[] {
  const head = must(run, ["git", "rev-parse", "HEAD"]);
  const refs = must(run, [
    "git",
    "for-each-ref",
    "--merged",
    "HEAD",
    "--no-merged",
    baseRef,
    "--format=%(objectname) %(refname:short)",
    "refs/heads",
  ]);
  const byDepth = new Map<number, string>();
  for (const [sha, branch] of refs.split("\n").filter(Boolean).map((line) => line.split(" ", 2) as [string, string])) {
    if (sha === head && branch !== current) continue;
    const depth = ahead(run, baseRef, branch);
    const other = byDepth.get(depth);
    if (other !== undefined) throw new Error(`cannot order ${other} and ${branch} in the stack; list it with --only`);
    byDepth.set(depth, branch);
  }
  return [...byDepth].sort(([a], [b]) => a - b).map(([, branch]) => branch);
}

/** Checks that `branches` form a chain from `baseRef` to the checked-out branch, each adding commits. */
export function checkChain(run: Run, branches: string[], current: string, baseRef: string): void {
  if (branches.length === 0) throw new Error(`${current} has no commits of its own on top of ${baseRef}`);
  if (branches.at(-1) !== current) throw new Error(`the top of the stack must be the checked-out branch, ${current}`);
  branches.forEach((branch, i) => {
    if (run(["git", "rev-parse", "--verify", "--quiet", `refs/heads/${branch}`]).code !== 0) {
      throw new Error(`there is no local branch ${branch}`);
    }
    const below = branches[i - 1] ?? baseRef;
    if (!isAncestor(run, below, branch)) throw new Error(`${branch} is not built on ${below}`);
    if (ahead(run, below, branch) === 0) throw new Error(`${branch} has no commits of its own on top of ${below}`);
  });
}

/** The description the pull request of `p` gets, before its stack navigation. */
function description(run: Run, p: Planned, open: OpenPr | undefined): string {
  if (p.pr.bodyFile !== undefined) return readFileSync(p.pr.bodyFile, "utf8");
  return p.pr.body ?? open?.body ?? firstCommit(run, p.below, p.branch).body;
}

/** Subject and body of the first commit `branch` adds on top of `below`. */
function firstCommit(run: Run, below: string, branch: string): { title: string; body: string } {
  const log = must(run, ["git", "log", "--reverse", "--format=%s%x00%b%x1e", `${below}..${branch}`]);
  const [title = "", body = ""] = (log.split("\x1e")[0] as string).split("\0");
  // Sign-off and co-author trailers belong to the commit, not to the pull request description.
  return { title, body: body.replace(/^(signed-off-by|co-authored-by): .*$/gim, "").trim() };
}

/**
 * The open pull request of `branch`, if any. Refuses a branch whose latest pull request was merged:
 * a squash merge leaves it outside the base, and shipping it would open a duplicate.
 */
function openPr(run: Run, branch: string, base: string): OpenPr | undefined {
  const fields = "number,url,baseRefName,state,body,milestone,projectItems";
  const latest = ghJson<FoundPr[]>(run, ["gh", "pr", "list", "--head", branch, "--state", "all", "--json", fields, "--limit", "1"])[0];
  if (latest?.state === "MERGED") {
    throw new Error(`${branch} was already merged in #${latest.number}; rebase the branches above it onto ${base} and delete it`);
  }
  return latest?.state === "OPEN" ? latest : undefined;
}

function ship(args: Args, run: Run): number {
  const exec = (cmd: string[], options?: RunOptions): string => {
    if (!args.dryRun) return must(run, cmd, options);
    console.log(`would run: ${cmd.map(quote).join(" ")}`);
    return "";
  };

  if (must(run, ["git", "status", "--porcelain"]) !== "") throw new Error("working tree is not clean; commit or stash first");
  const current = must(run, ["git", "branch", "--show-current"]);
  if (current === "") throw new Error("HEAD is detached; check out the branch to ship");
  if (current === args.base) throw new Error(`${current} is the base branch; check out the branch to ship`);
  if (!args.noPr && run(["gh", "auth", "status"]).code !== 0) throw new Error("gh is not signed in; run gh auth login");

  must(run, ["git", "fetch", "--quiet", args.remote, args.base]);
  const baseRef = `${args.remote}/${args.base}`;
  const branches = args.only.length > 0 ? args.only : detectStack(run, baseRef, current);
  if (!args.stack && branches.length > 1) {
    const parents = branches.slice(0, -1).join(", ");
    throw new Error(`${current} is built on ${parents}, not yet in ${baseRef}; use --stack, or --base <branch>`);
  }
  checkChain(run, branches, current, baseRef);
  for (const branch of args.branches.keys()) {
    if (!branches.includes(branch)) throw new Error(`--branch ${branch} is not in the stack: ${branches.join(", ")}`);
  }
  const g = args.global;
  if (branches.length > 1 && (g.title ?? g.body ?? g.bodyFile) !== undefined) {
    throw new Error("in a stack, give --title, --body and --body-file after --branch <name>");
  }
  const plan: Planned[] = branches.map((branch, i) => ({
    branch,
    base: branches[i - 1] ?? args.base,
    below: branches[i - 1] ?? baseRef,
    pr: effective(args.global, args.branches.get(branch)),
  }));
  const existing = args.noPr ? [] : plan.map((p) => openPr(run, p.branch, args.base));
  // Read before anything is pushed, so that a description closing an unknown issue stops the ship.
  const closes = plan.map((p, i) =>
    args.noPr
      ? []
      : closedIssues(description(run, p, existing[i])).map((number) =>
          ghJson<Issue>(run, ["gh", "issue", "view", String(number), "--json", "number,id,milestone,projectItems"]),
        ),
  );
  plan.forEach((p, i) => {
    p.pr = withIssues(p.pr, closes[i] ?? [], existing[i]);
  });
  const defaultBranch = closes.some((issues) => issues.length > 0)
    ? must(run, ["gh", "repo", "view", "--json", "defaultBranchRef", "--jq", ".defaultBranchRef.name"])
    : args.base;

  for (const p of plan) {
    const env = { NO_USER_DOCS: p.pr.labels.includes("no-user-docs") ? "true" : "false" };
    if (run([join(SCRIPTS, "check-pr.sh"), p.below, p.branch], { env, inherit: true }).code !== 0) {
      throw new Error(`commit checks failed for ${p.branch}`);
    }
  }

  console.log(`push-pr: shipping to ${args.remote}:`);
  for (const p of plan) {
    const sha = must(run, ["git", "rev-parse", "--short", p.branch]);
    console.log(`  ${p.branch} (${sha})${args.noPr ? "" : ` → pull request into ${p.base}`}`);
  }
  if (args.dryRun) console.log("dry run: nothing is stamped, pushed or published.");
  else if (run([join(SCRIPTS, "mark-reviewed.sh")], { inherit: true }).code !== 0) throw new Error("not stamped; nothing pushed");

  exec(["git", "push", "--atomic", ...(args.force ? ["--force-with-lease", "--force-if-includes"] : []), "-u", args.remote, ...branches], {
    inherit: true,
  });
  if (args.noPr) return 0;

  const prs = plan.map((p, i): Pr | undefined => {
    const open = existing[i];
    if (open !== undefined) {
      const edit = editArgs(p, open);
      if (edit !== undefined) exec(edit);
      return open;
    }
    const url = exec(createArgs(p, firstCommit(run, p.below, p.branch))).split("\n").at(-1) as string;
    const number = Number(/\/pull\/(\d+)/.exec(url)?.[1]);
    return Number.isNaN(number) ? undefined : { number, url, baseRefName: p.base };
  });

  const known = prs.filter((pr): pr is Pr => pr !== undefined);
  const stacked = known.length > 1 && known.length === prs.length;
  const numbers = known.map((pr) => pr.number);
  plan.forEach((p, i) => {
    const pr = prs[i];
    const issues = closes[i] ?? [];
    // GitHub links the issues itself for a pull request into the default branch.
    const link = p.base !== defaultBranch && issues.length > 0;
    if (pr === undefined || !(stacked || link)) return;
    const fields = "id,body,closingIssuesReferences";
    const view = ghJson<PrView>(run, ["gh", "pr", "view", String(pr.number), "--json", fields]);
    const body = stacked ? withStackBlock(view.body, stackBlock(numbers, args.base, pr.number)) : view.body;
    if (body !== view.body) exec(["gh", "pr", "edit", String(pr.number), "--body-file", "-"], { input: body });
    const linked = new Set(view.closingIssuesReferences.map((issue) => issue.number));
    for (const issue of issues) if (link && !linked.has(issue.number)) exec(linkArgs(issue.id, view.id));
  });
  for (const pr of known) console.log(`  ${pr.url}`);
  return 0;
}

export function main(argv: string[], run: Run): number {
  try {
    const args = parseArgs(argv);
    if (args.help) {
      console.log(USAGE);
      return 0;
    }
    return ship(args, run);
  } catch (error) {
    console.error(`push-pr: ${(error as Error).message}`);
    return 1;
  }
}

if (import.meta.main) process.exit(main(process.argv.slice(2), makeRun(process.cwd())));
