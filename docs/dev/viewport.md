# The 3D viewport: build and run

The viewport is the `viewport` crate compiled to WebAssembly and driven from the React UI ([ADR 0026](adr/0026-viewport-in-webassembly-with-a-binary-scene-channel.md)). Its interaction logic is plain Rust, tested natively; only `crates/viewport/src/web/` knows the browser.

## One-time setup

`scripts/setup.sh` checks the tools. The viewport needs, besides Rust and Bun:

- the `wasm32-unknown-unknown` target, which `rust-toolchain.toml` installs;
- `wasm-bindgen-cli` at exactly the version of the `wasm-bindgen` crate in `Cargo.lock`: `cargo install --locked wasm-bindgen-cli@<WASM_BINDGEN_VERSION>` (the version is in `scripts/tool-versions.env`). The build script refuses another version, as `wasm-bindgen` does.

## Build the module

```sh
scripts/build-viewport.sh
```

It builds the crate in release mode for `wasm32-unknown-unknown`, runs `wasm-bindgen --target web` and writes `viewport.js`, `viewport_bg.wasm` and their TypeScript declarations to `ui/src/viewport/pkg/`, then prints the size of the module (about 0.8 MB, 250 KB gzipped). The folder is generated and git-ignored, and Biome and typos skip it. The UI imports it only from `ui/src/viewport/`.

The `ui/` scripts call the build for you: `bun run dev`, `bun run build`, and the Playwright web server. `scripts/check-all.sh ui` and `desktop` build it before the typecheck and the bundle. Vitest does not: unit tests mock the module, so `bunx vitest run` needs no Rust build.

## Run it

1. `cd ui && bun run dev` builds the module and starts Vite on port 5173.
2. Open it in a browser with WebGPU (a recent Chrome or Edge), or run `cargo run -p desktop-app` for the window. On the desktop the webview's own WebGPU applies; the content security policy allows `'wasm-unsafe-eval'` for the module.

Rebuilding the module after a change in `crates/viewport` or `crates/render` is `bun run build:viewport` (or the script) and a page reload.

## Debugging

- A panic prints `viewport panicked at <file>:<line>` in the console and nothing else: messages could carry model content (SEC-03). Reproduce it natively in a test of the crate when you can; the interaction code runs there.
- GPU validation errors other than out-of-memory are printed as warnings in the console.
- `webgpuAvailable()` is false when the browser has no `navigator.gpu` or no adapter; the UI then shows its no-WebGPU state.
- End-to-end tests run Chromium with WebGPU flags (`ui/playwright.config.ts`): Metal on macOS, SwiftShader elsewhere.

## Native checks

```sh
CHECK_CRATES="viewport" scripts/check-all.sh rust   # format, lints, tests and coverage of the crate
scripts/check-all.sh wasm                           # the core crates build for the browser
```

The GPU tests of the crate need an adapter (Metal, lavapipe on Linux, WARP on Windows); they fail, never skip, without one.
