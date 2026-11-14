import { useState } from "react";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";

/** Where "Don't show again" is remembered. */
export const HINT_STORAGE_KEY = "bimcraft.viewport.navigationHint";

// "Got it" hides the card for the rest of the session: until the application restarts.
let dismissedThisSession = false;

/** Forgets "Got it", for tests. */
export function resetNavigationHint() {
  dismissedThisSession = false;
}

const ROWS = ["orbit", "pan", "zoom", "fit", "fitSelection", "trackpad"] as const;

// Storage can be missing or throw (private windows, blocked site data).
function hiddenForGood(): boolean {
  try {
    return localStorage.getItem(HINT_STORAGE_KEY) === "hidden";
  } catch {
    return false;
  }
}

/** The first-time card listing the mouse, trackpad and keyboard bindings (board 3.21). */
export function NavigationHint() {
  const { t } = useI18n();
  const [shown, setShown] = useState(() => !dismissedThisSession && !hiddenForGood());
  if (!shown) return null;

  const gotIt = () => {
    dismissedThisSession = true;
    setShown(false);
  };
  const never = () => {
    try {
      localStorage.setItem(HINT_STORAGE_KEY, "hidden");
    } catch {
      // Not remembered beyond this session.
    }
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
        <button type="button" className="button vp-quiet" onClick={never}>
          {t("viewport.hintCard.never")}
        </button>
      </div>
    </section>
  );
}
