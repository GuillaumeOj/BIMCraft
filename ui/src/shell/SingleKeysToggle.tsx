import { useI18n } from "../i18n/I18nProvider";
import { useShellState } from "./ShellState";

/**
 * Turns the single-key shortcuts (V, M, B…) on or off outside the focused 3D view (WCAG 2.1.4:
 * character key shortcuts must be switchable off). Next to the language choice in the title bar.
 */
export function SingleKeysToggle() {
  const { t } = useI18n();
  const { singleKeys, setSingleKeys } = useShellState();
  return (
    <label className="single-keys-toggle">
      <input
        type="checkbox"
        checked={singleKeys}
        onChange={(event) => setSingleKeys(event.target.checked)}
      />
      {t("shell.singleKeys")}
    </label>
  );
}
