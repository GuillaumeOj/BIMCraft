// Stands in for the generated viewport bindings (src/viewport/pkg/, built from Rust by
// scripts/build-viewport.sh) in unit tests, so Vitest runs without a Rust toolchain. Tests that
// reach the bindings mock this module (src/viewport/module.test.ts).
const missing = () => {
  throw new Error("the viewport bindings are not available in unit tests");
};

export default missing;
export const webgpuAvailable = missing;
export const Viewport = { create: missing };
