import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { useShellState } from "../shell/ShellState";
import "./forms.css";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** The elements of `root` that Tab can reach: enabled, not hidden and not inert. */
function tabStops(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) => element.closest("[inert], [hidden]") === null,
  );
}

/**
 * Wraps Tab and Shift+Tab around the ends of `root` and pulls stray focus back in. The toast
 * region is part of the loop: an Undo raised by the dialog must be reachable from it.
 */
function keepTabInside(
  event: { shiftKey: boolean; preventDefault: () => void },
  root: HTMLElement,
) {
  const toast = document.querySelector<HTMLElement>(".toast-region");
  const stops = [...tabStops(root), ...(toast ? tabStops(toast) : [])];
  const [first, last] = [stops[0], stops.at(-1)];
  const [edge, other] = event.shiftKey ? [first, last] : [last, first];
  const active = document.activeElement;
  const inside = active !== null && stops.includes(active as HTMLElement);
  if (!inside || active === edge) {
    event.preventDefault();
    (inside ? other : first)?.focus();
  }
}

type DialogProps = {
  /** Id of the element that names the dialog. */
  titleId: string;
  /** Called on Esc, unless an inner control already used the key. */
  onCancel: () => void;
  children: ReactNode;
};

/**
 * A modal dialog (UI-06): focus moves in when it opens, Tab stays inside, Esc cancels, and focus
 * returns to the opener when it closes. The content decides what takes the first focus; the
 * dialog itself takes it when the content does not.
 */
export function Dialog({ titleId, onCancel, children }: DialogProps) {
  const dialog = useRef<HTMLDivElement>(null);
  // Read while rendering: the content's effects move the focus before this component's.
  const [opener] = useState(() => document.activeElement);
  const { registerModal } = useShellState();

  useEffect(registerModal, [registerModal]);

  useEffect(() => {
    if (!dialog.current?.contains(document.activeElement)) dialog.current?.focus();
    return () => {
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, [opener]);

  // Tab pressed in the toast, which sits outside the dialog, goes back into the loop.
  useEffect(() => {
    const onToastKey = (event: globalThis.KeyboardEvent) => {
      const toast = document.querySelector(".toast-region");
      const inToast = event.target instanceof Node && toast?.contains(event.target);
      if (event.key === "Tab" && inToast && dialog.current) keepTabInside(event, dialog.current);
    };
    document.addEventListener("keydown", onToastKey);
    return () => document.removeEventListener("keydown", onToastKey);
  }, []);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && !event.defaultPrevented) {
      event.preventDefault();
      onCancel();
    } else if (event.key === "Tab" && dialog.current) {
      keepTabInside(event, dialog.current);
    }
  };

  return (
    <div className="scrim">
      <div
        ref={dialog}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        {children}
      </div>
    </div>
  );
}
