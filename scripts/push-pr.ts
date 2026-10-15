#!/usr/bin/env bun
// Stamps HEAD as reviewed, pushes the branch (or the whole stack of branches ending at HEAD) and
// opens or updates one pull request per branch, each based on the branch below it (AGENTS.md §1).
// Reserved to the user: agents are blocked from running it. Run with --help for the options.

import { existsSync } from "node:fs";
import { join } from "node:path";

const USAGE = `Usage: bun scripts/push-pr.ts [options] [--branch <name> [pr options]]...

Stamps HEAD as reviewed (scripts/mark-reviewed.sh), pushes, then opens or updates the pull request.
Run it only after /code-review and /simplify (or equivalent reviews) on these changes.

Stack options (anywhere):
  --stack               ship every local branch between <remote>/<base> and HEAD, bottom first;
                        each PR is based on the branch below it
  --only a,b,c          ship exactly these branches, bottom to top (the last one must be checked out)
  --base <branch>       base of the bottom pull request (default: main)
  --remote <name>       remote to push to (default: origin)
  --force               push with --force-with-lease (after a rebase or a restack)
  --no-pr               stamp and push only
  --yes                 do not ask for the review confirmation
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
  --project <title>     project
  --draft               open as a draft

In a stack of several branches, give --title, --body and --body-file per branch.
An existing open pull request is updated: base fixed, labels/assignees/reviewers added, title and
description replaced only when given.

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
  project?: string;
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
  yes: boolean;
  dryRun: boolean;
  /** Options given before any --branch. */
  global: PrOptions;
  /** Options given after --branch <name>, by branch. */
  branches: Map<string, PrOptions>;
};

/** A branch to push, the branch its pull request merges into, and that request's options. */
export type Planned = { branch: string; base: string; pr: PrOptions };

export type Pr = { number: number; url: string; baseRefName: string };

export type Result = { code: number; stdout: string; stderr: string };
export type RunOptions = { input?: string; env?: Record<string, string>; inherit?: boolean };
export type Run = (cmd: string[], options?: RunOptions) => Result;
export type Deps = { run: Run; ask: (question: string) => string | null };

const emptyPr = (): PrOptions => ({ labels: [], assignees: [], reviewers: [], draft: false });
const list = (value: string): string[] =>
  value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
const unique = (items: string[]): string[] => [...new Set(items)];

export function parseArgs(argv: string[]): Args {
  const args: Args = {
    help: false,
    stack: false,
    only: [],
    base: "main",
    remote: "origin",
    force: false,
    noPr: false,
    yes: false,
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
    switch (name) {
      case "-h":
      case "--help":
        args.help = flag();
        break;
      case "--stack":
        args.stack = flag();
        break;
      case "--only":
        args.only = list(value());
        args.stack = true;
        break;
      case "--base":
        args.base = value();
        break;
      case "--remote":
        args.remote = value();
        break;
      case "--force":
        args.force = flag();
        break;
      case "--no-pr":
        args.noPr = flag();
        break;
      case "--yes":
        args.yes = flag();
        break;
      case "--dry-run":
        args.dryRun = flag();
        break;
      case "--branch": {
        const branch = value();
        if (args.branches.has(branch)) throw new Error(`--branch ${branch} is given twice`);
        scope = emptyPr();
        args.branches.set(branch, scope);
        break;
      }
      case "--title":
        scope.title = value();
        break;
      case "--body":
        scope.body = value();
        break;
      case "--body-file":
        scope.bodyFile = value();
        break;
      case "--label":
        scope.labels.push(...list(value()));
        break;
      case "--assignee":
        scope.assignees.push(...list(value()));
        break;
      case "--reviewer":
        scope.reviewers.push(...list(value()));
        break;
      case "--milestone":
        scope.milestone = value();
        break;
      case "--project":
        scope.project = value();
        break;
      case "--draft":
        scope.draft = flag();
        break;
      default:
        throw new Error(`unknown option ${arg} (see --help)`);
    }
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
    project: own.project ?? global.project,
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
    ...optional("--project", pr.project),
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
    ...optional("--add-project", pr.project),
  ];
  return changes.length > 0 ? ["gh", "pr", "edit", String(existing.number), ...changes] : undefined;
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

const isAncestor = (run: Run, ancestor: string, of: string): boolean =>
  run(["git", "merge-base", "--is-ancestor", ancestor, of]).code === 0;
const ahead = (run: Run, base: string, branch: string): number =>
  Number(must(run, ["git", "rev-list", "--count", `${base}..${branch}`]));

/** Local branches between `baseRef` and HEAD, bottom first. */
export function detectStack(run: Run, baseRef: string): string[] {
  const merged = must(run, ["git", "for-each-ref", "--merged", "HEAD", "--format=%(refname:short)", "refs/heads"]);
  const byDepth = new Map<number, string>();
  for (const branch of merged.split("\n").filter(Boolean)) {
    if (isAncestor(run, branch, baseRef)) continue;
    const depth = ahead(run, baseRef, branch);
    const other = byDepth.get(depth);
    if (other !== undefined) throw new Error(`cannot order ${other} and ${branch} in the stack; list it with --only`);
    byDepth.set(depth, branch);
  }
  return [...byDepth].sort(([a], [b]) => a - b).map(([, branch]) => branch);
}

/** Checks that `branches` form a chain from `baseRef` to the checked-out branch, each adding commits. */
export function checkChain(run: Run, branches: string[], current: string, baseRef: string): void {
  if (branches.at(-1) !== current) throw new Error(`the top of the stack must be the checked-out branch, ${current}`);
  branches.forEach((branch, i) => {
    const below = i === 0 ? baseRef : (branches[i - 1] as string);
    if (!isAncestor(run, below, branch)) throw new Error(`${branch} is not built on ${below}`);
    if (ahead(run, below, branch) === 0) throw new Error(`${branch} has no commits of its own on top of ${below}`);
  });
}

/** Subject and body of the first commit `branch` adds on top of `below`. */
function firstCommit(run: Run, below: string, branch: string): { title: string; body: string } {
  const sha = must(run, ["git", "rev-list", "--reverse", `${below}..${branch}`]).split("\n")[0] as string;
  return {
    title: must(run, ["git", "log", "-1", "--format=%s", sha]),
    body: must(run, ["git", "log", "-1", "--format=%b", sha]),
  };
}

function openPr(run: Run, branch: string): Pr | undefined {
  const json = must(run, ["gh", "pr", "list", "--head", branch, "--state", "open", "--json", "number,url,baseRefName", "--limit", "1"]);
  return (JSON.parse(json) as Pr[])[0];
}

function ship(args: Args, { run, ask }: Deps): number {
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
  const branches = args.only.length > 0 ? args.only : args.stack ? detectStack(run, baseRef) : [current];
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
    base: i === 0 ? args.base : (branches[i - 1] as string),
    pr: effective(args.global, args.branches.get(branch)),
  }));
  const below = (i: number): string => (i === 0 ? baseRef : (plan[i]?.base as string));

  plan.forEach((p, i) => {
    const env = { NO_USER_DOCS: p.pr.labels.includes("no-user-docs") ? "true" : "false" };
    if (run([join(SCRIPTS, "check-pr.sh"), below(i), p.branch], { env, inherit: true }).code !== 0) {
      throw new Error(`commit checks failed for ${p.branch}`);
    }
  });

  console.log(`push-pr: shipping to ${args.remote}:`);
  for (const p of plan) {
    const sha = must(run, ["git", "rev-parse", "--short", p.branch]);
    console.log(`  ${p.branch} (${sha})${args.noPr ? "" : ` → pull request into ${p.base}`}`);
  }
  if (args.dryRun) console.log("dry run: nothing is stamped, pushed or published.");
  else {
    const answer = args.yes ? "y" : ask("Have /code-review and /simplify been run on these changes? [y/N]");
    if (!/^y(es)?$/i.test(answer?.trim() ?? "")) {
      console.error("push-pr: aborted; review the changes first.");
      return 1;
    }
    must(run, [join(SCRIPTS, "mark-reviewed.sh")], { inherit: true });
  }

  exec(["git", "push", "--atomic", ...(args.force ? ["--force-with-lease"] : []), "-u", args.remote, ...branches], {
    inherit: true,
  });
  if (args.noPr) return 0;

  const prs = plan.map((p, i): Pr | undefined => {
    const existing = openPr(run, p.branch);
    if (existing !== undefined) {
      const edit = editArgs(p, existing);
      if (edit !== undefined) exec(edit);
      return existing;
    }
    const url = exec(createArgs(p, firstCommit(run, below(i), p.branch))).split("\n").at(-1) as string;
    const number = Number(/\/pull\/(\d+)/.exec(url)?.[1]);
    return Number.isNaN(number) ? undefined : { number, url, baseRefName: p.base };
  });

  const known = prs.filter((pr): pr is Pr => pr !== undefined);
  if (known.length > 1 && known.length === prs.length) {
    const numbers = known.map((pr) => pr.number);
    for (const pr of known) {
      const body = JSON.parse(must(run, ["gh", "pr", "view", String(pr.number), "--json", "body"])).body as string;
      const next = withStackBlock(body, stackBlock(numbers, args.base, pr.number));
      if (next !== body) exec(["gh", "pr", "edit", String(pr.number), "--body-file", "-"], { input: next });
    }
  }
  for (const pr of known) console.log(`  ${pr.url}`);
  return 0;
}

export function main(argv: string[], deps: Deps): number {
  try {
    const args = parseArgs(argv);
    if (args.help) {
      console.log(USAGE);
      return 0;
    }
    return ship(args, deps);
  } catch (error) {
    console.error(`push-pr: ${(error as Error).message}`);
    return 1;
  }
}

if (import.meta.main) process.exit(main(process.argv.slice(2), { run: makeRun(process.cwd()), ask: prompt }));
