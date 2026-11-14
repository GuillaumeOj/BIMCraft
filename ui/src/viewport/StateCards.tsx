import { useLayoutEffect, useRef } from "react";
import { useI18n } from "../i18n/I18nProvider";
import { useShellState } from "../shell/ShellState";
import { ErrorIcon } from "../ui/ErrorIcon";
import { shortcutProps } from "../ui/shortcut";
import { GUIDE_PLACE_BLOCK, GUIDE_TROUBLESHOOTING } from "./guide";
import { WarningIcon } from "./icons";
import type { FailureReason } from "./useViewportSession";

/** Loading the module and the first scene (board 3.27b); after 10 s it offers Cancel. */
export function LoadingCard({ slow, onCancel }: { slow: boolean; onCancel: () => void }) {
  const { t } = useI18n();
  return (
    <div className="vp-card vp-card-centre" role="status">
      <span className="vp-spinner" aria-hidden="true" />
      <p className="vp-card-title">{t("viewport.loading")}</p>
      {slow && (
        <>
          <p>{t("viewport.loadingSlow")}</p>
          <button type="button" className="button" onClick={onCancel}>
            {t("viewport.cancel")}
          </button>
        </>
      )}
    </div>
  );
}

/** An empty model (board 3.27a): what to do next. */
export function EmptyCard({ onPlaceBlock }: { onPlaceBlock: () => void }) {
  const { t } = useI18n();
  const { singleKeys } = useShellState();
  const label = t("viewport.empty.button");
  return (
    <div className="vp-card vp-card-centre">
      <h2 className="vp-card-title">{t("viewport.empty.title")}</h2>
      <p>{t("viewport.empty.body")}</p>
      <button
        type="button"
        className="button button-primary"
        {...shortcutProps(t, label, singleKeys ? "B" : undefined)}
        onClick={onPlaceBlock}
      >
        {label}
      </button>
      <a href={GUIDE_PLACE_BLOCK} target="_blank" rel="noreferrer">
        {t("viewport.empty.guide")}
      </a>
    </div>
  );
}

/** No WebGPU on this computer (board 3.27c): plain words and the troubleshooting guide. */
export function UnavailableCard() {
  const { t } = useI18n();
  return (
    <div className="vp-card vp-card-centre">
      <h2 className="vp-card-title">
        <WarningIcon />
        {t("viewport.unavailable.title")}
      </h2>
      <p>{t("viewport.unavailable.body")}</p>
      <a className="button" href={GUIDE_TROUBLESHOOTING} target="_blank" rel="noreferrer">
        {t("viewport.unavailable.guide")}
      </a>
    </div>
  );
}

type FailedProps = { reason: FailureReason; repeated: boolean; onReload: () => void };

/** The view stopped (board 3.27d): the cause, Reload (focused) and Copy details. */
export function FailedCard({ reason, repeated, onReload }: FailedProps) {
  const { t } = useI18n();
  const { notify } = useShellState();
  const reload = useRef<HTMLButtonElement>(null);

  // The card takes the focus the view had (board 3.27d), and leaves it anywhere else alone.
  // Before paint: the hidden canvas is about to lose it.
  useLayoutEffect(() => {
    const card = reload.current;
    if (card?.closest(".vp")?.contains(document.activeElement)) card.focus();
  }, []);

  // Only the cause and the browser: never model content (SEC-03).
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(
        `BIMCraft 3D view stopped: ${reason}\n${navigator.userAgent}`,
      );
      notify(t("viewport.error.copied"));
    } catch {
      // Clipboard access can be refused; nothing was copied, so nothing is announced.
    }
  };

  return (
    <div className="vp-card vp-card-centre vp-card-error" role="alert">
      <h2 className="vp-card-title">
        <ErrorIcon />
        {t("viewport.error.title")}
      </h2>
      <p>{t(`viewport.error.${reason}`)}</p>
      {repeated && <p>{t("viewport.error.repeated")}</p>}
      <div className="vp-card-actions">
        <button type="button" className="button button-primary" ref={reload} onClick={onReload}>
          {t("viewport.error.reload")}
        </button>
        <button type="button" className="button" onClick={copy}>
          {t("viewport.error.copy")}
        </button>
      </div>
    </div>
  );
}
