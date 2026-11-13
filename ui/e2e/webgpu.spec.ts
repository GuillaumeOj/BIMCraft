import { expect, test } from "@playwright/test";

// Risk check for the 3D viewport e2e (VP-01): the bundled Chromium must give WebGPU an adapter
// without a graphics card, and be able to render and read back a pixel. No wasm module involved.
test("WebGPU renders a triangle to an offscreen texture", async ({ page }) => {
  await page.goto("/");

  const pixel = await page.evaluate(async () => {
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("no WebGPU adapter");
    const device = await adapter.requestDevice();

    // Numeric WebGPU flags: the GPU*Usage globals are not in the e2e type environment.
    // Texture usage RENDER_ATTACHMENT | COPY_SRC, buffer usage COPY_DST | MAP_READ, map mode READ.
    const size = 64;
    const texture = device.createTexture({
      size: [size, size],
      format: "rgba8unorm",
      usage: 0x10 | 0x01,
    });
    // One triangle covering the whole texture, so the centre pixel is always inside it.
    const module = device.createShaderModule({
      code: `
        @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
          var p = array<vec2f, 3>(vec2f(-1, -1), vec2f(3, -1), vec2f(-1, 3));
          return vec4f(p[i], 0, 1);
        }
        @fragment fn fs() -> @location(0) vec4f { return vec4f(1, 0, 0, 1); }`,
    });
    const pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs" },
      fragment: { module, entryPoint: "fs", targets: [{ format: "rgba8unorm" }] },
    });
    // Rows of a texture-to-buffer copy are padded to 256 bytes.
    const readback = device.createBuffer({
      size: 256 * size,
      usage: 0x08 | 0x01,
    });

    const encoder = device.createCommandEncoder();
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: texture.createView(),
          clearValue: [0, 0, 0, 1],
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.draw(3);
    pass.end();
    encoder.copyTextureToBuffer({ texture }, { buffer: readback, bytesPerRow: 256 }, [size, size]);
    device.queue.submit([encoder.finish()]);

    await readback.mapAsync(1);
    const row = size / 2;
    const offset = 256 * row + 4 * (size / 2);
    const rgba = Array.from(new Uint8Array(readback.getMappedRange()).slice(offset, offset + 4));
    readback.unmap();
    device.destroy();
    return rgba;
  });

  expect(pixel).toEqual([255, 0, 0, 255]);
});
