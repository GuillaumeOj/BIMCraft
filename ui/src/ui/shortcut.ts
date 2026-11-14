import type { MessageKey, MessageParams } from "../i18n/i18n";

type Translate = (key: MessageKey, params?: MessageParams) => string;

/**
 * What names a control's keyboard shortcut (UI-06): the keys for assistive technology and a
 * "Label (Key)" tooltip; without a shortcut, the label alone.
 */
export function shortcutProps(
  t: Translate,
  label: string,
  shortcut: string | undefined,
): { title: string; "aria-keyshortcuts"?: string } {
  if (shortcut === undefined) return { title: label };
  return { title: t("ui.withShortcut", { label, shortcut }), "aria-keyshortcuts": shortcut };
}
