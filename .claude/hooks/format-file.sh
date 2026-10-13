#!/bin/sh
# Formats the file an agent just edited, so its output always matches project style.
file=$(jq -r '.tool_input.file_path // empty')
[ -f "$file" ] || exit 0
cd "$CLAUDE_PROJECT_DIR" || exit 0

case "$file" in
  *.rs) command -v rustfmt >/dev/null && rustfmt "$file" ;;
  *.toml) command -v taplo >/dev/null && taplo fmt "$file" ;;
  */ui/*.[jt]s|*/ui/*.[jt]sx|*/ui/*.json|*/ui/*.jsonc|*/ui/*.css)
    [ -x ui/node_modules/.bin/biome ] && ui/node_modules/.bin/biome check --write --no-errors-on-unmatched "$file" ;;
esac >/dev/null 2>&1
exit 0
