# Add Renovate and fix dependencies — API contract

Not applicable. No command, query or IPC message is added or changed, and the `front` node gets no task. See [spec.md](spec.md).

The only contract this feature touches is for tooling:

- `scripts/check-all.sh renovate`: a new section. It exits non-zero when `renovate-config-validator --strict` reports an error or a warning on `.github/renovate.json5`.
- `scripts/tool-versions.env`: each `NAME_VERSION=x.y.z` line must have an exact version and a `# renovate: datasource=… depName=…` comment on the line above it. `bun scripts/check-pinned-deps.ts` enforces this.
