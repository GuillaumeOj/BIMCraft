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
import { useI18n } from "../i18n/I18nProvider";

/** The project's georeference with its derived values (`georef.get`). */
export type GeorefInfo = NonNullable<Queries["georef.get"]["result"]["georeference"]>;

export type GeorefState =
  | { status: "loading" }
  | { status: "ready"; info: GeorefInfo | null }
  | { status: "error"; reason: string | null };

export type Georef = {
  state: GeorefState;
  dialogOpen: boolean;
  openDialog: () => void;
  closeDialog: () => void;
  /** Shows `message` for a few seconds and announces it; `undoable` adds the undo hint. */
  notify: (message: string, undoable?: boolean) => void;
};

const TOAST_MS = 5000;

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
  const { t } = useI18n();
  const [state, setState] = useState<GeorefState>({ status: "loading" });
  const [dialogOpen, setDialogOpen] = useState(false);
  const [toast, setToast] = useState<{ message: string; undoable: boolean } | null>(null);
  const latest = useRef(0);
  const toastTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const load = useCallback(async () => {
    latest.current += 1;
    const mine = latest.current;
    let next: GeorefState;
    try {
      const { georeference } = await api.query("georef.get", {});
      next = { status: "ready", info: georeference };
    } catch (error) {
      next = { status: "error", reason: apiReason(error) };
    }
    // A slower, older answer must not overwrite a newer one.
    if (mine === latest.current) setState(next);
  }, [api]);

  useEffect(() => {
    void load();
    const unsubscribe = api.onChange(() => void load());
    return () => {
      void unsubscribe.then(
        (stop) => stop(),
        () => {},
      );
    };
  }, [api, load]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isOpenShortcut(event)) return;
      event.preventDefault();
      setDialogOpen(true);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => () => clearTimeout(toastTimer.current), []);

  const notify = useCallback((message: string, undoable = false) => {
    clearTimeout(toastTimer.current);
    setToast({ message, undoable });
    toastTimer.current = setTimeout(() => setToast(null), TOAST_MS);
  }, []);

  const value = useMemo<Georef>(
    () => ({
      state,
      dialogOpen,
      openDialog: () => setDialogOpen(true),
      closeDialog: () => setDialogOpen(false),
      notify,
    }),
    [state, dialogOpen, notify],
  );

  return (
    <GeorefContext.Provider value={value}>
      {children}
      {/* A live region present from the start, so that the first message is announced. */}
      <div className="toast-region" role="status">
        {toast !== null && (
          <p className="toast">
            {toast.message}
            {toast.undoable && <span className="toast-hint"> {t("georef.undoHint")}</span>}
          </p>
        )}
      </div>
    </GeorefContext.Provider>
  );
}

/** The georeference state and the dialog controls. */
export function useGeoref(): Georef {
  const georef = useContext(GeorefContext);
  if (georef === null) throw new Error("useGeoref must be used inside <GeorefProvider>");
  return georef;
}
