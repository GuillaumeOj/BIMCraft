import type { UpdateErrorCode, UpdateOffer, UpdateProgress } from "../api";

/** Where the update flow is (flow of docs ux-flow.md section 2). */
export type Phase =
  | "idle"
  | "checking"
  | "upToDate"
  | "available"
  | "downloading"
  | "ready"
  | "confirm"
  | "restarting";

export type UpdateState = {
  phase: Phase;
  offer: UpdateOffer | null;
  progress: UpdateProgress | null;
  /** The last failure to show, cleared by the next action. */
  failure: UpdateErrorCode | null;
  /** When the last check finished, in milliseconds since the epoch. */
  checkedAt: number | null;
  /** "Later" was chosen for the offer: the title-bar entry is back to the plain button. */
  dismissed: boolean;
};

export type UpdateAction =
  | { type: "checkStarted" }
  | { type: "checked"; offer: UpdateOffer | null; at: number }
  | { type: "checkFailed"; code: UpdateErrorCode; silent: boolean }
  | { type: "downloadStarted" }
  | { type: "progress"; progress: UpdateProgress }
  | { type: "downloaded" }
  | { type: "downloadFailed"; code: UpdateErrorCode }
  | { type: "confirm" }
  | { type: "cancelConfirm" }
  | { type: "installStarted" }
  | { type: "installFailed"; code: UpdateErrorCode }
  | { type: "later" };

export const initialUpdateState: UpdateState = {
  phase: "idle",
  offer: null,
  progress: null,
  failure: null,
  checkedAt: null,
  dismissed: false,
};

/** The transitions of the update flow. Pure: the provider supplies clock and results. */
export function updateReducer(state: UpdateState, action: UpdateAction): UpdateState {
  switch (action.type) {
    case "checkStarted":
      return { ...state, phase: "checking", failure: null, dismissed: false };
    case "checked":
      return {
        ...state,
        phase: action.offer === null ? "upToDate" : "available",
        offer: action.offer,
        checkedAt: action.at,
        failure: null,
      };
    case "checkFailed":
      // A start-up check that fails says nothing: the user did not ask for it.
      return { ...state, phase: "idle", offer: null, failure: action.silent ? null : action.code };
    case "downloadStarted":
      return { ...state, phase: "downloading", progress: null, failure: null };
    case "progress":
      // An event that outruns the end of its download must not leave a stale figure behind.
      return state.phase === "downloading" ? { ...state, progress: action.progress } : state;
    case "downloaded":
      return { ...state, phase: "ready", progress: null };
    case "downloadFailed":
      return { ...state, phase: "available", progress: null, failure: action.code };
    case "confirm":
      return { ...state, phase: "confirm", failure: null };
    case "cancelConfirm":
      return { ...state, phase: "ready" };
    case "installStarted":
      return { ...state, phase: "restarting", failure: null };
    case "installFailed":
      // Without its package the host cannot install: go back to downloading it again.
      return {
        ...state,
        phase: action.code === "not_downloaded" ? "available" : "ready",
        failure: action.code,
      };
    case "later":
      return { ...state, dismissed: true };
  }
}

/** How the error banner is worded: the codes that share a text share a kind. */
export type FailureKind = "network" | "signature" | "manifest" | "install" | "other";

export function failureKind(code: UpdateErrorCode): FailureKind {
  switch (code) {
    case "network":
    case "signature":
    case "manifest":
    case "install":
      return code;
    case "not_downloaded":
      return "install";
    case "no_update":
    case "unavailable":
    case "unknown":
      return "other";
  }
}

/** What the retry button of a failure does. */
export type Retry = "check" | "download" | "confirm";

export function retryOf(state: UpdateState): Retry {
  const { phase, failure } = state;
  if (failure === null || failureKind(failure) === "other") return "check";
  if (phase === "ready") return "confirm";
  return phase === "available" ? "download" : "check";
}

/** The states of the title-bar entry (boards 8.7). */
export type PillKind = "idle" | "available" | "downloading" | "ready" | "failed";

export function pillKind(state: UpdateState): PillKind {
  switch (state.phase) {
    case "available":
      if (state.failure !== null) return "failed";
      return state.dismissed ? "idle" : "available";
    case "downloading":
      return "downloading";
    case "ready":
    case "confirm":
    case "restarting":
      return "ready";
    default:
      return "idle";
  }
}

/** The percentage downloaded, when the size is known. */
export function percentDownloaded(progress: UpdateProgress | null): number | null {
  if (progress === null || progress.total === null || progress.total <= 0) return null;
  return Math.min(100, Math.floor((progress.downloaded / progress.total) * 100));
}

/** Whether the channel can change: a new check would discard a downloaded package. */
export const channelLocked = (state: UpdateState): boolean =>
  ["checking", "downloading", "ready", "confirm", "restarting"].includes(state.phase);

/** Decimal size with one decimal below 10 (`2.4 MB`, `38 MB`). Units come from the caller. */
export function formatBytes(
  bytes: number,
  units: readonly [string, string, string, string],
  locale: string,
): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  // 999.5 already rounds up to 1000: move to the next unit instead of showing "1000 kB".
  while (value >= 999.5 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  // 9.95 and above round to 10, which has no decimal.
  const digits = unit > 0 && value < 9.95 ? 1 : 0;
  const number = new Intl.NumberFormat(locale, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(digits === 0 ? Math.round(value) : value);
  return `${number} ${units[unit]}`;
}
