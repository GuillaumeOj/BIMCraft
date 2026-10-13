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

# Start of a command: line start, or after ; & | ( ` or $(
at_cmd='(^|[;&|(`]|\$\()[[:space:]]*'
git_cmd="${at_cmd}git([[:space:]]+-[Cc][[:space:]]+[^[:space:]]+)*[[:space:]]+"

matches() { printf '%s\n' "$command" | grep -Eq "$1"; }

matches "${git_cmd}push([[:space:]]|$)" && block "pushing is reserved to the user"
matches "${at_cmd}gh[[:space:]]+(pr[[:space:]]+(create|merge)|release|repo[[:space:]]+create)" &&
  block "remote publishing is reserved to the user"
matches '(^|[[:space:]])--no-verify([[:space:]]|$)' && block "bypassing git hooks is not allowed"
# Reading the value (`git config core.hooksPath`, `--get`) is fine; setting or unsetting it is not.
matches "${git_cmd}config[^;&|]*(--unset[^;&|]*core\.hooksPath|core\.hooksPath[[:space:]]+[^-;&|[:space:]])" &&
  block "changing the git hooks path is not allowed"
matches '>[[:space:]]*[^[:space:]]*bimcraft-review-stamp' &&
  block "only scripts/mark-reviewed.sh may write the review stamp"
exit 0
