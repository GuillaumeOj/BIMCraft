#!/bin/sh
# Blocks shell commands that would push or bypass checks, wherever they appear in a command line
# (permission rules only match the start of a command). Exit code 2 blocks the tool call.
# Write files with the Write/Edit tools rather than heredocs: text mentioning these commands at
# the start of a line or after a shell operator is treated as a command.
command=$(jq -r '.tool_input.command // empty')

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
matches 'push|gh|verify|commit|hookspath|review-stamp' || exit 0

matches "${git_cmd}push([[:space:]]|$)" && block "pushing is reserved to the user"
matches "${at_cmd}gh[[:space:]]+(pr[[:space:]]+(create|merge)|release|repo[[:space:]]+create)" &&
  block "remote publishing is reserved to the user"
matches '(^|[[:space:]])--no-verify([[:space:]]|$)' && block "bypassing git hooks is not allowed"
# `-n` is the short form of --no-verify for git commit (alone or bundled, e.g. -an).
matches "${git_cmd}commit([[:space:]][^;&|]*)?[[:space:]]-[a-z]*n" && block "bypassing git hooks is not allowed"
# Reading the value (`git config core.hooksPath`, `--get`) is fine; setting, unsetting or
# overriding it for one command (`git -c core.hooksPath=…`) is not.
matches "${git_cmd}config[^;&|]*(--unset[^;&|]*core\.hooksPath|core\.hooksPath[[:space:]]+[^-;&|[:space:]])" &&
  block "changing the git hooks path is not allowed"
matches 'core\.hooksPath=' && block "changing the git hooks path is not allowed"
# Run directly or through an interpreter, with any flags (bun --bun …, bash -c "bun …").
matches "${at_cmd}((bun|bunx|node|deno|sh|bash|zsh)([[:space:]]+[^[:space:];&|]+)*[[:space:]]+)?([^[:space:]]*/)?push-pr\.ts" &&
  block "scripts/push-pr.ts stamps and pushes, which is reserved to the user"
matches 'bimcraft-review-stamp' && block "only scripts/mark-reviewed.sh may touch the review stamp"
exit 0
