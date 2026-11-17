import { closeSync, fstatSync, openSync, readSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

/**
 * Where Chromium writes its own log (`--log-file`). Playwright keeps the browser's stderr in its
 * debug log only, so a GPU process that hangs or dies would leave no trace in the report. Each
 * worker launches one browser, and the config and the specs are loaded in that same process, so
 * the process id gives the worker its own file.
 */
export const browserLogFile = join(tmpdir(), `bimcraft-e2e-chromium-${process.pid}.log`);

/** The lines that tell why a WebGPU device was lost; the rest of Chromium's log is noise here. */
const GPU_LINE = /gpu|webgpu|dawn|swiftshader|vulkan|context lost|watchdog|hung|crash/i;
/** What one test may add to the log before the rest is not read: a bound on the attachment. */
const MAX_BYTES = 8 * 1024 * 1024;

/** The current length of the log, to read only what a test adds to it. */
export function browserLogSize(): number {
  try {
    return statSync(browserLogFile).size;
  } catch {
    return 0;
  }
}

/**
 * The GPU-related lines Chromium logged since `from`, at most `limit` of them, and how many more
 * were left out. `null` when the browser wrote no log.
 */
export function gpuLogLines(
  from: number,
  limit: number,
): { kept: string[]; dropped: number } | null {
  let text: string;
  try {
    const file = openSync(browserLogFile, "r");
    try {
      const length = Math.min(MAX_BYTES, Math.max(0, fstatSync(file).size - from));
      const buffer = Buffer.alloc(length);
      readSync(file, buffer, 0, length, from);
      text = buffer.toString("utf8");
    } finally {
      closeSync(file);
    }
  } catch {
    return null;
  }
  const lines = text.split("\n").filter((line) => GPU_LINE.test(line));
  return { kept: lines.slice(0, limit), dropped: Math.max(0, lines.length - limit) };
}
