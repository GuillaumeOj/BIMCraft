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

/** Wraps Tab and Shift+Tab around the ends of `root`, and pulls stray focus back in. */
function keepTabInside(event: KeyboardEvent, root: HTMLElement) {
  const stops = tabStops(root);
  const [first, last] = [stops[0], stops.at(-1)];
  const [edge, other] = event.shiftKey ? [first, last] : [last, first];
  if (!root.contains(document.activeElement) || document.activeElement === edge) {
    event.preventDefault();
    (root.contains(document.activeElement) ? other : first)?.focus();
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
