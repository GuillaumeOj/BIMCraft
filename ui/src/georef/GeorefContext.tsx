import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { apiReason, type Queries, useApi } from "../api";
import { useShellState } from "../shell/ShellState";

/** The project's georeference with its derived values (`georef.get`). */
export type GeorefInfo = NonNullable<Queries["georef.get"]["result"]["georeference"]>;

export type GeorefState =
  | { status: "loading" }
  /** `reloadError` is set when a later reload failed: `info` is then the last known value. */
  | { status: "ready"; info: GeorefInfo | null; reloadError: { reason: string | null } | null }
  | { status: "error"; reason: string | null };

export type Georef = {
  state: GeorefState;
  dialogOpen: boolean;
  openDialog: () => void;
  closeDialog: () => void;
};

const GeorefContext = createContext<Georef | null>(null);

// Ctrl+Shift+L on Windows and Linux, Cmd+Shift+L on macOS. Ctrl+Alt is avoided: it is AltGr.
const isOpenShortcut = (event: KeyboardEvent) =>
  (event.ctrlKey || event.metaKey) &&
  event.shiftKey &&
  !event.altKey &&
  event.key.toLowerCase() === "l";

/**
 * Keeps the project's georeference up to date for the dialog and the panel: it is read at start
 * and again after every `model.changed` notification (undo included). It also owns the open state
 * of the dialog, its shortcut and the toast.
 */
export function GeorefProvider({ children }: { children: ReactNode }) {
  const api = useApi();
  const [state, setState] = useState<GeorefState>({ status: "loading" });
  const [dialogOpen, setDialogOpen] = useState(false);
  const { modalOpen } = useShellState();
  const latest = useRef(0);

  const load = useCallback(async () => {
    latest.current += 1;
    const mine = latest.current;
    let info: GeorefInfo | null = null;
    let failed: { reason: string | null } | null = null;
    try {
      info = (await api.query("georef.get", {})).georeference;
    } catch (error) {
      failed = { reason: apiReason(error) };
    }
    // A slower, older answer must not overwrite a newer one.
    if (mine !== latest.current) return;
    setState((current) => {
      if (failed !== null) {
        // A failed reload keeps the last known state, so that typed input is not lost.
        return current.status === "ready"
          ? { ...current, reloadError: failed }
          : { status: "error", reason: failed.reason };
      }
      // An unchanged georeference keeps its object, so that every model change does not reset
      // the form and the panel built on it.
      if (current.status === "ready" && JSON.stringify(current.info) === JSON.stringify(info)) {
        return current.reloadError === null ? current : { ...current, reloadError: null };
      }
      return { status: "ready", info, reloadError: null };
    });
  }, [api]);

  useEffect(() => {
    void load();
    // Without a notification channel (a browser outside the desktop window) it stays unsubscribed.
    const unsubscribe = api.onChange(() => void load()).catch(() => undefined);
    return () => {
      void unsubscribe.then((stop) => stop?.());
    };
  }, [api, load]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // A modal dialog keeps the keyboard.
      if (!isOpenShortcut(event) || modalOpen) return;
      event.preventDefault();
      setDialogOpen(true);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [modalOpen]);

  const value = useMemo<Georef>(
    () => ({
      state,
      dialogOpen,
      openDialog: () => setDialogOpen(true),
      closeDialog: () => setDialogOpen(false),
    }),
    [state, dialogOpen],
  );

  return <GeorefContext.Provider value={value}>{children}</GeorefContext.Provider>;
}

/** The georeference state and the dialog controls. */
export function useGeoref(): Georef {
  const georef = useContext(GeorefContext);
  if (georef === null) throw new Error("useGeoref must be used inside <GeorefProvider>");
  return georef;
}
