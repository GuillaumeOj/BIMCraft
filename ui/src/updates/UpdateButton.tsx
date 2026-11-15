import { useI18n } from "../i18n/I18nProvider";
import { ErrorIcon } from "../ui/ErrorIcon";
import { useUpdates } from "./UpdatesContext";
import { percentDownloaded, pillKind } from "./updateState";
import "./updates.css";

/**
 * The title-bar entry of the updates: a plain button while nothing is pending, a pill when the
 * user is needed (boards 8.7). Same element, same shortcut: the notice and the way in are one.
 * It never takes focus and never opens anything by itself.
 */
export function UpdateButton() {
  const { t } = useI18n();
  const { updatable, state, openDialog } = useUpdates();
  if (!updatable) return null;

  const kind = pillKind(state);
  const percent = percentDownloaded(state.progress);
  const version = state.offer?.version ?? "";
  const labels = {
    idle: t("updates.open"),
    available: t("updates.pill.available"),
    downloading:
      percent === null
        ? t("updates.pill.downloadingUnknown")
        : t("updates.pill.downloading", { percent }),
    ready: t("updates.pill.ready"),
    failed: t("updates.pill.failed"),
  };

  return (
    <button
      type="button"
      className={kind === "idle" ? "ghost-button" : "update-pill"}
      data-kind={kind}
      aria-haspopup="dialog"
      aria-keyshortcuts="Control+Shift+U Meta+Shift+U"
      aria-label={kind === "available" ? t("updates.pill.availableLabel", { version }) : undefined}
      onClick={() => openDialog()}
    >
      {kind === "failed" && <ErrorIcon />}
      {(kind === "available" || kind === "ready") && (
        <span className="update-pill-dot" aria-hidden="true" />
      )}
      {labels[kind]}
    </button>
  );
}
