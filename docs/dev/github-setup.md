# GitHub repository setup

Settings to apply once the GitHub repository exists. They make CI the final gate described in [quality.md](quality.md).

## Branch protection (ruleset on `main`)

- Require a pull request before merging, with at least one approval and dismissal of stale approvals.
- Require status checks to pass: **Repository checks**, **Commits and user docs**, **Rust (ubuntu-24.04)**, **Rust (macos-15)**, **Rust (windows-2025)**, **Rust coverage**, **Fuzzing**, **UI**, **End-to-end** and **Analyze (actions)** (add the Rust and TypeScript CodeQL checks once that code exists). Require branches to be up to date.
- Require conversation resolution and linear history.
- Block force pushes and deletions.
- Restrict who can bypass the rules to repository administrators, and use it only in emergencies.

## Security

- Enable private vulnerability reporting (Settings → Security → Private vulnerability reporting).
- Enable Dependabot alerts and security updates, and secret scanning with push protection.
- Actions: allow only actions pinned to a full commit SHA (Settings → Actions → General → “Require actions to be pinned to a full-length commit SHA”); set the default workflow permissions to read-only; do not allow Actions to create or approve pull requests.

## Labels

Create the labels, milestones and project board described in [project-management.md](project-management.md#one-time-setup).

## DCO

Install the [DCO GitHub App](https://github.com/apps/dco) or rely on the `Commits and user docs` CI job, which already rejects unsigned commits.
