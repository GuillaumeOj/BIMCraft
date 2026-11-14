/** The global single-letter shortcuts of the 3D view (api-contract.md §2, design/ux-notes.md). */
export type ViewportShortcut =
  | "select"
  | "move"
  | "place_block"
  | "projection"
  | "snapping"
  | "hide"
  | "isolate"
  | "reset_visibility"
  | "section_box";

const LETTERS: Readonly<Record<string, ViewportShortcut>> = {
  v: "select",
  m: "move",
  b: "place_block",
  p: "projection",
  s: "snapping",
  h: "hide",
  i: "isolate",
};

/**
 * Alt chords, matched on the physical key: on macOS Option+letter types a symbol, so `key` is not
 * the letter (design/ux-notes.md).
 */
const CHORDS: Readonly<Record<string, ViewportShortcut>> = {
  KeyH: "reset_visibility",
  KeyB: "section_box",
};

/** Whether `target` takes typed text: letters typed there are text, not shortcuts. */
export function isTextEntry(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (!(target instanceof HTMLInputElement)) return false;
  return !["button", "checkbox", "radio", "range", "reset", "submit"].includes(target.type);
}

/**
 * The viewport shortcut `event` presses, or null. Letters with Ctrl or Cmd belong to the browser
 * and to the shell's chords, a held key does not repeat a toggle, text fields keep their
 * letters, and with `singleKeys` off only the focused `canvas` takes the single letters. Alt
 * chords are not single keys, so they work wherever the focus is.
 */
export function viewportShortcut(
  event: KeyboardEvent,
  { singleKeys, canvas }: { singleKeys: boolean; canvas: HTMLCanvasElement | null },
): ViewportShortcut | null {
  if (event.ctrlKey || event.metaKey || event.repeat) return null;
  if (isTextEntry(event.target)) return null;
  if (event.altKey) return event.shiftKey ? null : (CHORDS[event.code] ?? null);
  // Turned off (WCAG 2.1.4), they work only while the 3D view has the focus.
  if (!singleKeys && event.target !== canvas) return null;
  return LETTERS[event.key.toLowerCase()] ?? null;
}
