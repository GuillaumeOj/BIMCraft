import { useEffect, useId, useRef, useState } from "react";
import { useI18n } from "../i18n/I18nProvider";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { loadErrorMessage } from "./formModel";
import { useGeoref } from "./GeorefContext";
import { LocationForm } from "./LocationForm";

/** The Project location dialog (F1): where the project is on Earth, and how it is turned. */
export function LocationDialog() {
  const { t } = useI18n();
  const { state, closeDialog } = useGeoref();
  const titleId = useId();
  const closeButton = useRef<HTMLButtonElement>(null);
  // Nothing closes the dialog while a command runs: its refusal must reach the user.
  const [busy, setBusy] = useState(false);
  const showingForm = state.status === "ready";

  // Without the form, the Close button takes the focus once, not at every render.
  useEffect(() => {
    if (!showingForm) closeButton.current?.focus();
  }, [showingForm]);

  return (
    <Dialog titleId={titleId} onCancel={() => !busy && closeDialog()}>
      <h2 id={titleId} className="dialog-title">
        {t("georef.location.title")}
      </h2>
      {showingForm ? (
        <LocationForm info={state.info} reloadError={state.reloadError} onBusyChange={setBusy} />
      ) : (
        <>
          <div className="dialog-body">
            {state.status === "loading" ? (
              <p>{t("georef.loading")}</p>
            ) : (
              <Banner>{t(loadErrorMessage(state.reason))}</Banner>
            )}
          </div>
          <div className="dialog-footer">
            <button type="button" className="button" onClick={closeDialog} ref={closeButton}>
              {t("georef.close")}
            </button>
          </div>
        </>
      )}
    </Dialog>
  );
}
