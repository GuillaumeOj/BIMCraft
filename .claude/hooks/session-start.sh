#!/bin/sh
# Reminds the agent of the golden rules and checks that the shared git hooks are enabled.
cd "$CLAUDE_PROJECT_DIR" || exit 0
echo "BIMCraft: follow AGENTS.md. Never push; the user pushes after /code-review and /simplify."
if [ "$(git config core.hooksPath 2>/dev/null)" != ".githooks" ]; then
  echo "WARNING: git hooks are not enabled. Ask the user to run scripts/setup.sh."
fi
exit 0
