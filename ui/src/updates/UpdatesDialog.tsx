import { type ReactNode, useEffect, useId, useRef } from "react";
import type { UpdateOffer, UpdateProgress } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import { Banner } from "../ui/Banner";
import { Dialog } from "../ui/Dialog";
import { SegmentedToggle } from "../ui/SegmentedToggle";
import { type Updates, useUpdates } from "./UpdatesContext";
import {
  channelLocked,
  type FailureKind,
  failureKind,
  formatBytes,
  percentDownloaded,
  retryOf,
} from "./updateState";
import "./updates.css";

/** The Updates dialog, or the restart confirmation that replaces it (one modal at a time). */
export function UpdatesDialog() {
  const updates = useUpdates();
  const titleId = useId();
  const { phase } = updates.state;
  const onCancel = () => {
    if (phase === "confirm") updates.cancelRestart();
    else if (phase !== "restarting") updates.closeDialog();
  };
  return (
    <Dialog titleId={titleId} onCancel={onCancel}>
      {phase === "confirm" ? (
        <RestartConfirmation titleId={titleId} updates={updates} />
      ) : (
        <UpdatesPanel titleId={titleId} updates={updates} />
      )}
    </Dialog>
  );
}

/** Mounts the dialog while it is open. */
export function UpdatesDialogHost() {
  const { dialogOpen } = useUpdates();
  return dialogOpen ? <UpdatesDialog /> : null;
}

/**
 * Moves focus to the view's first primary action when the dialog opens and when the state
 * changes under a control that went away; a control the user chose (`data-keep-focus`) keeps it.
 */
function useInitialFocus(viewKey: string) {
  const root = useRef<HTMLDivElement>(null);
  const opened = useRef(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs at each change of the view
  useEffect(() => {
    const active = document.activeElement;
    const kept = opened.current && active?.isConnected && active.closest("[data-keep-focus]");
    opened.current = true;
    if (kept) return;
    const target = root.current?.querySelector<HTMLElement>("[data-initial-focus]");
    (target ?? root.current?.closest<HTMLElement>('[role="dialog"]'))?.focus();
  }, [viewKey]);
  return root;
}

/** The Windows installer shows its own window during the restart (ux-flow.md section 11). */
const isWindows = () => /windows/i.test(navigator.userAgent);

function RestartConfirmation({ titleId, updates }: { titleId: string; updates: Updates }) {
  const { t } = useI18n();
  const root = useInitialFocus("confirm");
  const version = updates.state.offer?.version ?? "";
  return (
    <div ref={root} className="updates-confirm">
      <h2 id={titleId} className="dialog-title">
        {t("updates.confirm.title")}
      </h2>
      <div className="dialog-body">
        <p>{t("updates.confirm.body", { version })}</p>
        {isWindows() && <p>{t("updates.restart.windowsNote")}</p>}
      </div>
      <div className="dialog-footer">
        <div className="dialog-actions">
          <button
            type="button"
            className="button"
            data-initial-focus
            onClick={updates.cancelRestart}
          >
            {t("updates.confirm.cancel")}
          </button>
          <button type="button" className="button button-primary" onClick={updates.restart}>
            {t("updates.confirm.restart")}
          </button>
        </div>
      </div>
    </div>
  );
}

const FAILURE_KEYS = {
  network: ["updates.error.network", null],
  signature: ["updates.error.signature", "updates.error.signatureHint"],
  manifest: ["updates.error.manifest", "updates.error.manifestHint"],
  install: ["updates.error.install", "updates.error.installHint"],
  other: ["updates.error.other", null],
} as const satisfies Record<FailureKind, readonly [string, string | null]>;

function FailureBanner({ code }: { code: NonNullable<Updates["state"]["failure"]> }) {
  const { t } = useI18n();
  const [message, hint] = FAILURE_KEYS[failureKind(code)];
  return (
    <div className="updates-failure">
      <Banner>{t(message)}</Banner>
      {hint !== null && <p className="updates-hint">{t(hint)}</p>}
    </div>
  );
}

function UpdatesPanel({ titleId, updates }: { titleId: string; updates: Updates }) {
  const { t } = useI18n();
  const { state } = updates;
  const root = useInitialFocus(`${state.phase}:${state.failure ?? ""}`);
  const locked = channelLocked(state);
  const fromBeta = updates.version.includes("-");
  return (
    <div ref={root} className="updates-panel">
      <div className="updates-header">
        <h2 id={titleId} className="dialog-title">
          {t("updates.title")}
        </h2>
        <button
          type="button"
          className="ghost-button updates-close"
          aria-label={t("updates.close")}
          disabled={state.phase === "restarting"}
          onClick={updates.closeDialog}
        >
          <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
          </svg>
        </button>
      </div>
      <div className="dialog-body updates-body">
        <p>{t("updates.version", { version: updates.version })}</p>
        <ChannelSection updates={updates} locked={locked} fromBeta={fromBeta} />
        <AutoCheckSwitch updates={updates} />
        <StatusArea updates={updates} />
        {state.failure !== null && <FailureBanner code={state.failure} />}
        {state.phase === "available" && state.failure === null && state.offer !== null && (
          <ReleaseNotes offer={state.offer} />
        )}
      </div>
      <Footer updates={updates} />
    </div>
  );
}

function ChannelSection({
  updates,
  locked,
  fromBeta,
}: {
  updates: Updates;
  locked: boolean;
  fromBeta: boolean;
}) {
  const { t } = useI18n();
  const { channel } = updates;
  return (
    <div data-keep-focus>
      <p className="updates-label">{t("updates.channel.label")}</p>
      <SegmentedToggle
        label={t("updates.channel.label")}
        value={channel}
        disabled={locked}
        options={[
          { value: "stable", label: t("updates.channel.stable") },
          { value: "beta", label: t("updates.channel.beta") },
        ]}
        onChange={updates.setChannel}
      />
      <p className="updates-hint">
        {t(channel === "beta" ? "updates.channel.betaHelp" : "updates.channel.stableHelp")}
      </p>
      {locked && <p className="updates-hint">{t("updates.channel.locked")}</p>}
      {channel === "beta" && <p className="updates-note">{t("updates.channel.betaNote")}</p>}
      {channel === "stable" && fromBeta && (
        <p className="updates-note">{t("updates.channel.fromBetaNote")}</p>
      )}
    </div>
  );
}

function AutoCheckSwitch({ updates }: { updates: Updates }) {
  const { t } = useI18n();
  const helpId = useId();
  return (
    <div data-keep-focus>
      <label className="updates-switch">
        <input
          type="checkbox"
          checked={updates.autoCheck}
          aria-describedby={helpId}
          onChange={(event) => updates.setAutoCheck(event.target.checked)}
        />
        <span className="updates-switch-track" aria-hidden="true" />
        {t("updates.auto.label")}
      </label>
      <p className="updates-hint" id={helpId}>
        {t("updates.auto.help")}
      </p>
    </div>
  );
}

function Spinner() {
  return <span className="updates-spinner" aria-hidden="true" />;
}

function useDateFormats() {
  const { language } = useI18n();
  return {
    time: (at: number) =>
      new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(at),
    date: (iso: string | null) => {
      const at = iso === null ? Number.NaN : Date.parse(iso);
      return Number.isNaN(at)
        ? null
        : new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(at);
    },
  };
}

function StatusArea({ updates }: { updates: Updates }) {
  const { t } = useI18n();
  const formats = useDateFormats();
  const { state, version } = updates;
  const offered = state.offer?.version ?? "";
  const released = formats.date(state.offer?.date ?? null);
  const failedHere = state.failure !== null;

  let lines: ReactNode = null;
  switch (state.phase) {
    case "idle":
      lines = failedHere ? null : <p>{t("updates.idle")}</p>;
      break;
    case "checking":
      lines = (
        <p>
          <Spinner />
          {t("updates.checking")}
        </p>
      );
      break;
    case "upToDate":
      lines = (
        <>
          <p className="updates-headline">{t("updates.upToDate", { version })}</p>
          {state.checkedAt !== null && (
            <p className="updates-hint">
              {t("updates.lastChecked", { time: formats.time(state.checkedAt) })}
            </p>
          )}
        </>
      );
      break;
    case "available":
      lines = (
        <>
          <p className="updates-headline">{t("updates.available", { version: offered })}</p>
          {released !== null && (
            <p className="updates-hint">{t("updates.released", { date: released })}</p>
          )}
          <p className="updates-hint">{t("updates.availableHint")}</p>
        </>
      );
      break;
    case "downloading":
      lines = <p className="updates-headline">{t("updates.downloading", { version: offered })}</p>;
      break;
    case "ready":
      lines = (
        <>
          <p className="updates-headline">{t("updates.ready", { version: offered })}</p>
          <p className="updates-hint">{t("updates.readyHint")}</p>
        </>
      );
      break;
    case "restarting":
      lines = (
        <p>
          <Spinner />
          {t("updates.restarting")}
        </p>
      );
      break;
    case "confirm":
      break;
  }

  return (
    <>
      <div className="updates-status" role="status" aria-live="polite">
        {lines}
      </div>
      {state.phase === "downloading" && (
        <>
          <DownloadProgress progress={state.progress} />
          <p className="updates-hint">{t("updates.downloadingHint")}</p>
        </>
      )}
    </>
  );
}

function DownloadProgress({ progress }: { progress: UpdateProgress | null }) {
  const { t, language } = useI18n();
  const units = [
    t("updates.unit.b"),
    t("updates.unit.kb"),
    t("updates.unit.mb"),
    t("updates.unit.gb"),
  ] as const;
  const percent = percentDownloaded(progress);
  const downloaded = formatBytes(progress?.downloaded ?? 0, units, language);
  const text =
    progress?.total == null
      ? t("updates.progress.unknown", { downloaded })
      : t("updates.progress.known", {
          downloaded,
          total: formatBytes(progress.total, units, language),
        });
  return (
    <div className="updates-progress">
      <div
        className="progress-bar"
        role="progressbar"
        aria-label={t("updates.progress.label")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
        aria-valuetext={text}
        data-indeterminate={percent === null}
      >
        <div
          className="progress-bar-fill"
          style={percent === null ? undefined : { width: `${percent}%` }}
        />
      </div>
      <p className="updates-hint updates-progress-text">
        <span>{text}</span>
        {percent !== null && <span>{t("updates.progress.percent", { percent })}</span>}
      </p>
    </div>
  );
}

function ReleaseNotes({ offer }: { offer: UpdateOffer }) {
  const { t } = useI18n();
  const labelId = useId();
  return (
    <div>
      <p className="updates-label" id={labelId}>
        {t("updates.notes.label", { version: offer.version })}
      </p>
      {/* Plain text only: the manifest is not trusted markup. React escapes it; pre-wrap keeps its lines. */}
      {/* biome-ignore lint/a11y/noNoninteractiveTabindex: a scrolling box must take focus so that keyboard users can scroll it (WCAG 2.1.1). */}
      <section className="updates-notes" aria-labelledby={labelId} tabIndex={0}>
        {offer.notes === "" ? t("updates.notes.empty") : offer.notes}
      </section>
    </div>
  );
}

function Footer({ updates }: { updates: Updates }) {
  const { t } = useI18n();
  const { state } = updates;
  const { phase, failure } = state;
  const checking = phase === "checking";
  const button = (
    label: ReactNode,
    onClick: () => void,
    options: { primary?: boolean; initial?: boolean; disabled?: boolean } = {},
  ) => (
    <button
      type="button"
      className={options.primary ? "button button-primary" : "button"}
      aria-disabled={options.disabled ? true : undefined}
      data-initial-focus={options.initial ? "" : undefined}
      onClick={options.disabled ? undefined : onClick}
    >
      {label}
    </button>
  );
  const checkNow = (primary = false) =>
    button(t("updates.checkNow"), updates.checkNow, {
      primary,
      initial: primary,
      disabled: checking,
    });
  const close = (initial = false) => button(t("updates.close"), updates.closeDialog, { initial });
  const tryAgain = button(t("updates.retry"), updates.retry, { primary: true, initial: true });
  const retryIsCheck =
    failure !== null && retryOf(state) === "check" && failureKind(failure) === "other";

  let left: ReactNode = null;
  let right: ReactNode = null;
  if (failure !== null) {
    right = retryIsCheck ? (
      checkNow(true)
    ) : (
      <>
        {close()}
        {tryAgain}
      </>
    );
  } else {
    switch (phase) {
      case "idle":
        right = checkNow(true);
        break;
      case "checking":
        left = checkNow();
        right = close();
        break;
      case "upToDate":
        left = checkNow();
        right = close(true);
        break;
      case "available":
        left = checkNow();
        right = (
          <>
            {button(t("updates.later"), updates.later)}
            {button(t("updates.download"), updates.download, { primary: true, initial: true })}
          </>
        );
        break;
      case "downloading":
        right = close(true);
        break;
      case "ready":
        right = (
          <>
            {button(t("updates.later"), updates.later)}
            {button(t("updates.restart"), updates.askRestart, { primary: true, initial: true })}
          </>
        );
        break;
      case "confirm":
      case "restarting":
        break;
    }
  }
  return (
    <div className="dialog-footer">
      {left}
      <div className="dialog-actions">{right}</div>
    </div>
  );
}
