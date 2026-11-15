#!/usr/bin/env bash
# Dry run only (ENG-05): a throw-away updater key pair, without password, so that the dry run signs
# and checks its update packages like a release does. Its installers trust this key, which nobody
# keeps, so they can never update. Exports DRY_RUN_UPDATER_KEY and DRY_RUN_UPDATER_PUBLIC_KEY to
# the following steps of the job.
set -eu
key="$RUNNER_TEMP/dry-run-updater.key"
cargo tauri signer generate --ci --password "" --write-keys "$key" >/dev/null
{
  echo "DRY_RUN_UPDATER_KEY=$(cat "$key")"
  echo "DRY_RUN_UPDATER_PUBLIC_KEY=$(cat "$key.pub")"
} >> "$GITHUB_ENV"
