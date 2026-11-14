import { act } from "@testing-library/react";
import { vi } from "vitest";
import type {
  Tool,
  ViewCommand,
  ViewportEvent,
  ViewportHandle,
  ViewportModule,
  ViewportTheme,
} from "../viewport/types";

/**
 * A stand-in for the viewport's WebAssembly module (events in, commands out): it records what the
 * UI sends and lets a test raise the events the real viewport would.
 */
export function fakeViewport(options: { webgpu?: boolean } = {}) {
  let listener: ((event: ViewportEvent) => void) | null = null;
  let revision: number | null = null;
  let rejectScenes = false;
  const commands: ViewCommand[] = [];
  const tools: Tool[] = [];
  const themes: ViewportTheme[] = [];
  const sizes: [number, number, number][] = [];
  /** The first byte of each delta applied. */
  const applied: number[] = [];
  const free = vi.fn();

  const handle: ViewportHandle = {
    free,
    applyScene(bytes) {
      if (rejectScenes) throw new Error("scene rejected");
      revision = bytes[0] ?? 0;
      applied.push(revision);
      return revision;
    },
    revision: () => revision,
    resize: (width, height, ratio) => sizes.push([width, height, ratio]),
    setTheme: (theme) => themes.push(theme),
    command: (command) => commands.push(command),
    setTool: (tool) => tools.push(tool),
    onEvent: (next) => {
      listener = next;
    },
  };
  const create = vi.fn(async (_canvas: HTMLCanvasElement, opts: { theme: ViewportTheme }) => {
    themes.push(opts.theme);
    return handle;
  });
  const module: ViewportModule = {
    webgpuAvailable: async () => options.webgpu ?? true,
    create,
  };

  return {
    module,
    handle,
    create,
    free,
    commands,
    tools,
    themes,
    sizes,
    applied,
    /** Raises `event` as the viewport would. */
    emit: (event: ViewportEvent) => act(() => listener?.(event)),
    /** Makes every later delta fail to apply. */
    rejectScenes: () => {
      rejectScenes = true;
    },
  };
}
