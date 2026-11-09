import { type ReactNode, useEffect, useRef, useState } from "react";

export type MenuItem = { label: string; onSelect: () => void };

type Props = {
  /** What the button shows. */
  children: ReactNode;
  /** Names the button when its content is only a symbol; the visible text must come first. */
  label?: string;
  className?: string;
  items: readonly MenuItem[];
};

/**
 * A button that opens a menu of actions (`aria-haspopup="menu"`): Escape closes it, and focus
 * returns to the button. The first item takes focus when it opens; arrows move between items.
 */
export function ActionMenu({ children, label, className = "button", items }: Props) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) menu.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
  }, [open]);

  const close = () => {
    setOpen(false);
    button.current?.focus();
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    const step = { ArrowDown: 1, ArrowUp: -1 }[event.key as "ArrowDown" | "ArrowUp"];
    if (step === undefined) return;
    event.preventDefault();
    const stops = [...(menu.current?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];
    const at = stops.indexOf(document.activeElement as HTMLElement);
    stops[(at + step + stops.length) % stops.length]?.focus();
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: Escape and arrows are handled for the menu inside.
    <div className="menu-anchor" onKeyDown={onKeyDown}>
      <button
        type="button"
        ref={button}
        className={className}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={label}
        onClick={() => setOpen((value) => !value)}
      >
        {children}
      </button>
      {open && (
        <div className="menu" role="menu" ref={menu}>
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
