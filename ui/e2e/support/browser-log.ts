import { closeSync, fstatSync, mkdirSync, openSync, readSync, statSync } from "node:fs";
import { join } from "node:path";

/**
 * Where Chromium writes its own log (`--log-file`). Playwright keeps the browser's stderr in its
 * debug log only, so a GPU process that hangs or dies would leave no trace in the report. Each
 * worker launches one browser, and the config and the specs are loaded in that same process, so
 * the process id gives the worker its own file. A browser launched elsewhere (UI mode, a reused
 * browser) writes no such file, and the attachment says so. The folder is Playwright's output
 * folder (the config sets it from here), which it empties at the start of each run, so the logs
 * do not pile up.
 */
export const outputDir = join(import.meta.dirname, "..", "..", "test-results");
mkdirSync(outputDir, { recursive: true });
export const browserLogFile = join(outputDir, `e2e-chromium-${process.pid}.log`);

/** The first or last lines of a log, and how many others were left out. */
export type KeptLines = { kept: string[]; dropped: number };

/**
 * Chromium prefixes each line with its level and source file
 * (`[pid:tid:time:LEVEL:gpu/…/file.cc(12)] message`). The source path names `gpu` on every line of
 * the GPU process, so the words are looked for in the message only.
 */
const LINE = /^\[[^\]]*:(INFO|WARNING|ERROR|FATAL|VERBOSE\d*):[^\]]*\]\s?(.*)$/;
const ABOUT_GPU = /dawn|swiftshader|vulkan|context|lost|watchdog|hung|crash|exited|gpu/i;
/** How much of the log one test may add before only its end is read: a bound on the read. */
const MAX_BYTES = 2 * 1024 * 1024;

/** The current length of the log, to read only what a test adds to it. */
export function browserLogSize(): number {
  try {
    return statSync(browserLogFile).size;
  } catch {
    return 0;
  }
}

/** The log from `from` to its end, or its last `MAX_BYTES` when the test logged more. */
function readFrom(from: number): string | null {
  try {
    const file = openSync(browserLogFile, "r");
    try {
      const size = fstatSync(file).size;
      const start = Math.max(from, size - MAX_BYTES);
      const buffer = Buffer.allocUnsafe(Math.max(0, size - start));
      readSync(file, buffer, 0, buffer.length, start);
      return buffer.toString("utf8");
    } finally {
      closeSync(file);
    }
  } catch {
    return null;
  }
}

/** Whether a line is a warning or worse, or speaks of the GPU in its message. */
function aboutGpu(line: string): boolean {
  const parsed = LINE.exec(line);
  if (parsed === null) return ABOUT_GPU.test(line);
  const [, level = "", message = ""] = parsed;
  return /^(WARNING|ERROR|FATAL)$/.test(level) || ABOUT_GPU.test(message);
}

/**
 * The GPU-related lines Chromium logged since `from`: the last `limit` of them, where a loss
 * shows up, and how many earlier ones were left out. `null` when the browser wrote no log.
 */
export function gpuLogLines(from: number, limit: number): KeptLines | null {
  const text = readFrom(from);
  if (text === null) return null;
  const lines = text.split("\n").filter(aboutGpu);
  return { kept: lines.slice(-limit), dropped: Math.max(0, lines.length - limit) };
}
