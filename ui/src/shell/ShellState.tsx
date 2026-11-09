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
import { historyDepth, undoLastStep, useApi } from "../api";
import { useI18n } from "../i18n/I18nProvider";

/** Something the toast can undo: runs the undo, and rejects when it cannot. */
type Undo = () => Promise<void>;

type Toast = { id: number; message: string; undoable: boolean };

export type ShellState = {
  /** Whether a modal dialog is open: the global shortcuts then stay quiet. */
  modalOpen: boolean;
  /** Tells the shell that a modal dialog opened; the returned call tells it that it closed. */
  registerModal: () => () => void;
  /**
   * Shows `message` for a few seconds and announces it. With `undoable`, the toast offers Undo for
   * as long as the step it announces is the last one of the history.
   */
  notify: (message: string, options?: { undoable?: boolean }) => void;
};

// A toast with Undo stays longer: the user needs time to reach the button.
const TOAST_MS = 5000;
const UNDO_TOAST_MS = 7000;

const ShellStateContext = createContext<ShellState | null>(null);

/**
 * What the shell shares between its dialogs: how many modal dialogs are open and the toast.
 * Neither belongs to one dialog: a shortcut must stay quiet behind any of them, and a toast
 * outlives the dialog that raised it.
 */
export function ShellStateProvider({ children }: { children: ReactNode }) {
  const api = useApi();
  const { t } = useI18n();
  const [modals, setModals] = useState(0);
  const [toast, setToast] = useState<Toast | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const counter = useRef(0);
  // The depth of the history right after the step the toast announces.
  const announcedDepth = useRef<number | null>(null);

  useEffect(() => () => clearTimeout(timer.current), []);

  const registerModal = useCallback(() => {
    setModals((count) => count + 1);
    return () => setModals((count) => count - 1);
  }, []);

  const notify = useCallback(
    (message: string, options: { undoable?: boolean } = {}) => {
      clearTimeout(timer.current);
      counter.current += 1;
      const id = counter.current;
      const undoable = options.undoable === true;
      announcedDepth.current = null;
      setToast({ id, message, undoable });
      timer.current = setTimeout(() => setToast(null), undoable ? UNDO_TOAST_MS : TOAST_MS);
      if (undoable) {
        historyDepth(api).then(
          (depth) => {
            if (counter.current === id) announcedDepth.current = depth;
          },
          () =>
            setToast((current) => (current?.id === id ? { ...current, undoable: false } : current)),
        );
      }
    },
    [api],
  );

  // Any later change to the model makes Undo undo something else, so the toast stops offering it.
  const undoable = toast?.undoable === true;
  useEffect(() => {
    if (!undoable) return;
    const unsubscribe = api
      .onChange(async () => {
        const announced = announcedDepth.current;
        if (announced === null) return;
        const depth = await historyDepth(api).catch(() => null);
        if (depth !== announced) {
          setToast((current) => (current ? { ...current, undoable: false } : current));
        }
      })
      .catch(() => undefined);
    return () => {
      void unsubscribe.then((stop) => stop?.());
    };
  }, [api, undoable]);

  const undo: Undo = async () => {
    setToast(null);
    await undoLastStep(api);
  };

  const value = useMemo<ShellState>(
    () => ({ modalOpen: modals > 0, registerModal, notify }),
    [modals, registerModal, notify],
  );

  return (
    <ShellStateContext.Provider value={value}>
      {children}
      {/* A live region present from the start, so that the first message is announced. */}
      <div className="toast-region" role="status">
        {toast !== null && (
          <p className="toast">
            <span>{toast.message}</span>
            {toast.undoable && (
              <button
                type="button"
                className="toast-action"
                onClick={() => undo().catch(() => notify(t("toast.undoFailed")))}
              >
                {t("toast.undo")}
              </button>
            )}
          </p>
        )}
      </div>
    </ShellStateContext.Provider>
  );
}

/** The modal state and the toast of the shell. */
export function useShellState(): ShellState {
  const state = useContext(ShellStateContext);
  if (state === null) throw new Error("useShellState must be used inside <ShellStateProvider>");
  return state;
}
