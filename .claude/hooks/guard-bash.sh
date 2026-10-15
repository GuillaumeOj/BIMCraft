#!/bin/sh
# Blocks shell commands that would rewrite or publish beyond a feature branch, or bypass checks,
# wherever they appear in a command line (permission rules only match the start of a command).
# Exit code 2 blocks the tool call. Plain and --force-with-lease pushes of a feature branch and
# draft pull requests pass here; .claude/settings.json asks the user for each one, and pre-push
# requires the review stamp.
# Write files with the Write/Edit tools rather than heredocs: text mentioning these commands at
# the start of a line or after a shell operator is treated as a command.
# Line continuations are joined, so the options of one command stay on one line for grep.
command=$(jq -r '(.tool_input.command // empty) | gsub("\\\\\n"; " ")')

block() {
  echo "Blocked by .claude/hooks/guard-bash.sh: $1 (AGENTS.md §1)." >&2
  exit 2
}

# Start of a command: line start, or after ; & | ( ` or $(, then optional variable assignments,
# wrappers (env, command, xargs…) and a directory prefix (/usr/bin/git).
at_cmd='(^|[;&|(`]|\$\()[[:space:]]*(([a-z_][a-z0-9_]*=[^[:space:]]*|command|env|exec|nohup|time|xargs|sudo)[[:space:]]+)*([^[:space:];&|()]*/)?'
git_cmd="${at_cmd}git([[:space:]]+(-c[[:space:]]+[^[:space:]]+|--[^[:space:]]+))*[[:space:]]+"

# Case-insensitive: git config keys are (core.hookspath works too).
matches() { printf '%s\n' "$command" | grep -Eiq "$1"; }

# Fast path: this runs on every Bash call, and most mention none of the trigger words.
matches 'push|(^|[^a-z0-9_.-])gh[[:space:]]|verify|commit|hookspath|review-stamp' || exit 0

# Arguments up to the next shell operator, then one whole, possibly quoted, argument.
args='([[:space:]][^;&|]*)?[[:space:]]'
q="[\"']?"
end="${q}([[:space:]]|$)"

if matches "${git_cmd}push([[:space:]]|$)"; then
  push="${git_cmd}push${args}${q}"
  # --force-with-lease (AGENTS.md §3b) refuses to overwrite remote commits not seen locally;
  # --force, -f and + refspecs overwrite blindly.
  matches "${push}(-[a-z]*f[a-z]*|--force|\+[^[:space:]]*)${end}" &&
    block "force pushes other than --force-with-lease are reserved to the user"
  matches "${push}(-[a-z]*d[a-z]*|--delete|--mirror|--all|--tags|--prune|:[^[:space:]]*)${end}" &&
    block "deleting, mirroring or pushing tags is reserved to the user"
  matches "${push}([^[:space:]]*:)?(refs/heads/)?main${end}" && block "pushing main is reserved to the user"
  # From a checkout of main, any push is refused: without a refspec, or with HEAD, it updates main.
  [ "$(git symbolic-ref --quiet --short HEAD 2>/dev/null)" = main ] && block "pushing from main is reserved to the user"
fi

if matches "${at_cmd}gh[[:space:]]"; then
  matches "${at_cmd}gh[[:space:]]+(pr[[:space:]]+(merge|ready)|release|repo[[:space:]]+create)" &&
    block "merging, marking pull requests ready and publishing are reserved to the user"
  pr_create="${at_cmd}gh[[:space:]]+pr[[:space:]]+create"
  if matches "${pr_create}" && ! matches "${pr_create}${args}(--draft|-d)${end}"; then
    block "agents open pull requests as drafts (gh pr create --draft)"
  fi
fi

matches '(^|[[:space:]])--no-verify([[:space:]]|$)' && block "bypassing git hooks is not allowed"
# `-n` is the short form of --no-verify for git commit (alone or bundled, e.g. -an).
matches "${git_cmd}commit${args}-[a-z]*n" && block "bypassing git hooks is not allowed"
# Reading the value (`git config core.hooksPath`, `--get`) is fine; setting, unsetting or
# overriding it for one command (`git -c core.hooksPath=…`) is not.
matches "${git_cmd}config[^;&|]*(--unset[^;&|]*core\.hooksPath|core\.hooksPath[[:space:]]+[^-;&|[:space:]])" &&
  block "changing the git hooks path is not allowed"
matches 'core\.hooksPath=' && block "changing the git hooks path is not allowed"
matches 'bimcraft-review-stamp' && block "only scripts/mark-reviewed.sh may touch the review stamp"
exit 0
