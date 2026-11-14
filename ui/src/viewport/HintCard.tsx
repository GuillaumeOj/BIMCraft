import { useState } from "react";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { readStored, writeStored } from "../ui/storage";

/**
 * Where the card's dismissal is remembered: "Got it" in session storage (until the application
 * restarts), "Don't show again" in local storage.
 */
export const HINT_STORAGE_KEY = "bimcraft.viewport.navigationHint";

const ROWS = ["orbit", "pan", "zoom", "fit", "fitSelection", "trackpad"] as const;

const dismissed = () =>
  readStored(HINT_STORAGE_KEY, "session") !== null || readStored(HINT_STORAGE_KEY) !== null;

/** The first-time card listing the mouse, trackpad and keyboard bindings (board 3.21). */
export function NavigationHint() {
  const { t } = useI18n();
  const [shown, setShown] = useState(() => !dismissed());
  if (!shown) return null;

  const gotIt = () => {
    writeStored(HINT_STORAGE_KEY, "hidden", "session");
    setShown(false);
  };
  const never = () => {
    writeStored(HINT_STORAGE_KEY, "hidden");
    gotIt();
  };

  return (
    <section className="vp-card vp-hint-card" aria-labelledby="vp-hint-title">
      <h2 className="vp-card-title" id="vp-hint-title">
        {t("viewport.hintCard.title")}
      </h2>
      <dl>
        {ROWS.map((row) => (
          <div key={row}>
            <dt>{t(`viewport.hintCard.${row}` as MessageKey)}</dt>
            <dd>{t(`viewport.hintCard.${row}Keys` as MessageKey)}</dd>
          </div>
        ))}
      </dl>
      <div className="vp-card-actions">
        <button type="button" className="button button-primary" onClick={gotIt}>
          {t("viewport.hintCard.gotIt")}
        </button>
        <button type="button" className="ghost-button" onClick={never}>
          {t("viewport.hintCard.never")}
        </button>
      </div>
    </section>
  );
}
