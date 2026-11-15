import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import {
  type AppInfo,
  type Channel,
  UpdateError,
  type UpdateErrorCode,
  type UpdatesApi,
  updatesApi,
} from "../api";
import { useI18n } from "../i18n/I18nProvider";
import { useShellState } from "../shell/ShellState";
import { readAutoCheck, readChannel, writeAutoCheck, writeChannel } from "./settings";
import {
  initialUpdateState,
  pillKind,
  retryOf,
  type UpdateState,
  updateReducer,
} from "./updateState";

/** How long the window is ready before the automatic check runs (ux-flow.md section 7). */
export const STARTUP_CHECK_DELAY_MS = 5000;

export type Updates = {
  /** False until the host answered, and for builds that cannot update: nothing is shown then. */
  updatable: boolean;
  version: string;
  state: UpdateState;
  channel: Channel;
  autoCheck: boolean;
  dialogOpen: boolean;
  /** The text of the visually hidden live region (polite). */
  announcement: string;
  /** Opens the Updates dialog, and with `check` starts a check when none is running. */
  openDialog: (options?: { check?: boolean }) => void;
  closeDialog: () => void;
  setChannel: (channel: Channel) => void;
  setAutoCheck: (on: boolean) => void;
  checkNow: () => void;
  download: () => void;
  later: () => void;
  askRestart: () => void;
  cancelRestart: () => void;
  restart: () => void;
  /** Runs what the error's "Try again" does. */
  retry: () => void;
};

const UpdatesContext = createContext<Updates | null>(null);

const codeOf = (error: unknown): UpdateErrorCode =>
  error instanceof UpdateError ? error.code : "unknown";

type Props = {
  children: ReactNode;
  api?: UpdatesApi;
  startupDelayMs?: number;
};

/**
 * The update flow of the program (spec D4): the state, the saved choices, the automatic check
 * and every call to the host. The notice and the dialog only render it.
 */
export function UpdatesProvider({
  children,
  api = updatesApi,
  startupDelayMs = STARTUP_CHECK_DELAY_MS,
}: Props) {
  const { t } = useI18n();
  const { modalOpen } = useShellState();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [state, dispatch] = useReducer(updateReducer, initialUpdateState);
  const [channel, setChannelState] = useState(readChannel);
  const [autoCheck, setAutoCheckState] = useState(readAutoCheck);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [startupDue, setStartupDue] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const startupDone = useRef(false);
  // Read when a check ends: a start-up check fails silently only while nobody is looking.
  const dialogOpenRef = useRef(false);
  dialogOpenRef.current = dialogOpen;
  const announced = useRef(new Set<string>());
  const updatable = info?.updatable === true;

  useEffect(() => {
    let current = true;
    void api.appInfo().then((result) => current && setInfo(result));
    return () => {
      current = false;
    };
  }, [api]);

  const runCheck = useCallback(
    async (on: Channel, silent: boolean) => {
      dispatch({ type: "checkStarted" });
      try {
        const offer = await api.checkForUpdate(on);
        dispatch({ type: "checked", offer, at: Date.now() });
      } catch (error) {
        dispatch({
          type: "checkFailed",
          code: codeOf(error),
          silent: silent && !dialogOpenRef.current,
        });
      }
    },
    [api],
  );

  // The one automatic check: a few seconds after start, never over a dialog, silent on failure.
  useEffect(() => {
    if (!updatable || !autoCheck || startupDone.current) return;
    const timer = setTimeout(() => setStartupDue(true), startupDelayMs);
    return () => clearTimeout(timer);
  }, [updatable, autoCheck, startupDelayMs]);

  useEffect(() => {
    if (!startupDue || modalOpen || !autoCheck || startupDone.current) return;
    startupDone.current = true;
    // When the user got there first (a manual check, a download), there is nothing left to do.
    if (state.phase === "idle") void runCheck(channel, true);
  }, [startupDue, modalOpen, autoCheck, channel, runCheck, state.phase]);

  const checkNow = useCallback(() => void runCheck(channel, false), [channel, runCheck]);

  const download = useCallback(async () => {
    dispatch({ type: "downloadStarted" });
    try {
      await api.downloadUpdate((progress) => dispatch({ type: "progress", progress }));
      dispatch({ type: "downloaded" });
    } catch (error) {
      dispatch({ type: "downloadFailed", code: codeOf(error) });
    }
  }, [api]);

  const restart = useCallback(async () => {
    dispatch({ type: "installStarted" });
    try {
      await api.installUpdate();
    } catch (error) {
      dispatch({ type: "installFailed", code: codeOf(error) });
    }
  }, [api]);

  const setChannel = useCallback(
    (next: Channel) => {
      writeChannel(next);
      setChannelState(next);
      void runCheck(next, false);
    },
    [runCheck],
  );

  const setAutoCheck = useCallback((on: boolean) => {
    writeAutoCheck(on);
    setAutoCheckState(on);
  }, []);

  const openDialog = useCallback(
    (options: { check?: boolean } = {}) => {
      setDialogOpen(true);
      const busy = ["checking", "downloading", "ready", "confirm", "restarting"];
      if (options.check === true && !busy.includes(state.phase)) void runCheck(channel, false);
    },
    [channel, runCheck, state.phase],
  );

  const retry = useCallback(() => {
    const action = retryOf(state);
    if (action === "download") void download();
    else if (action === "confirm") dispatch({ type: "confirm" });
    else checkNow();
  }, [state, download, checkNow]);

  // Announce what needs the user while the dialog is closed, once per version and kind. What the
  // open dialog shows counts as announced.
  const version = state.offer?.version ?? "";
  const kind = pillKind(state);
  useEffect(() => {
    const key = `${kind}:${version}`;
    if (kind === "idle" || kind === "downloading" || announced.current.has(key)) return;
    announced.current.add(key);
    if (dialogOpen) return;
    setAnnouncement(t(`updates.announce.${kind}`, { version }));
  }, [dialogOpen, kind, version, t]);

  const value = useMemo<Updates>(
    () => ({
      updatable,
      version: info?.version ?? "",
      state,
      channel,
      autoCheck,
      dialogOpen: updatable && dialogOpen,
      announcement,
      openDialog,
      closeDialog: () => setDialogOpen(false),
      setChannel,
      setAutoCheck,
      checkNow,
      download: () => void download(),
      later: () => {
        dispatch({ type: "later" });
        setDialogOpen(false);
      },
      askRestart: () => dispatch({ type: "confirm" }),
      cancelRestart: () => dispatch({ type: "cancelConfirm" }),
      restart: () => void restart(),
      retry,
    }),
    [
      updatable,
      info,
      state,
      channel,
      autoCheck,
      dialogOpen,
      announcement,
      openDialog,
      setChannel,
      setAutoCheck,
      checkNow,
      download,
      restart,
      retry,
    ],
  );

  return (
    <UpdatesContext.Provider value={value}>
      {children}
      {/* In place before anything is announced; absent when the build cannot update. */}
      {updatable && (
        <div className="visually-hidden" role="status">
          {announcement}
        </div>
      )}
    </UpdatesContext.Provider>
  );
}

/** The update flow: state, saved choices and actions. */
export function useUpdates(): Updates {
  const updates = useContext(UpdatesContext);
  if (updates === null) throw new Error("useUpdates must be used inside <UpdatesProvider>");
  return updates;
}
