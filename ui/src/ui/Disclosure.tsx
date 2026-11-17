import { type ReactNode, useId } from "react";

type DisclosureProps = {
  title: string;
  /** What the section holds, shown at the right of the title: it stays readable while closed. */
  status?: string;
  open: boolean;
  onToggle: (open: boolean) => void;
  children: ReactNode;
};

/** A section that opens and closes ("Advanced"); its content stays mounted while closed. */
export function Disclosure({ title, status, open, onToggle, children }: DisclosureProps) {
  const id = useId();
  return (
    <section className="disclosure">
      <button
        type="button"
        className="disclosure-toggle"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => onToggle(!open)}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" data-open={open}>
          <path d="M4 2l4 4-4 4" fill="none" stroke="currentColor" strokeWidth="1.5" />
        </svg>
        {title}
        {status !== undefined && <span className="disclosure-status">{status}</span>}
      </button>
      <div id={id} hidden={!open}>
        {children}
      </div>
    </section>
  );
}
