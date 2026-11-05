import { type KeyboardEvent, useId, useMemo, useState } from "react";
import type { CrsSummary } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import { crsLabel } from "./formModel";

export type CrsList =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; crs: readonly CrsSummary[] };

type CrsComboboxProps = {
  id: string;
  list: CrsList;
  selected: CrsSummary | null;
  onSelect: (crs: CrsSummary) => void;
  /** Ids of the elements that describe the field (help, error). */
  describedBy: string | undefined;
  invalid: boolean;
};

/** The CRSs whose code, name or area of use contains every word of `query`. */
function matching(list: readonly CrsSummary[], query: string): readonly CrsSummary[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return list.filter((crs) => {
    const haystack = `${crs.epsg} ${crs.name} ${crs.area}`.toLowerCase();
    return words.every((word) => haystack.includes(word));
  });
}

/**
 * Searchable list of coordinate systems (the ARIA combobox pattern): type a code or a name,
 * move with the arrow keys, choose with Enter or a click. Esc closes the list first.
 */
export function CrsCombobox({
  id,
  list,
  selected,
  onSelect,
  describedBy,
  invalid,
}: CrsComboboxProps) {
  const { t } = useI18n();
  const listboxId = useId();
  const [open, setOpen] = useState(false);
  // The text being typed, or null while the field shows the chosen system.
  const [query, setQuery] = useState<string | null>(null);
  const [active, setActive] = useState(0);

  const results = useMemo(
    () => (list.status === "ready" ? matching(list.crs, query ?? "") : []),
    [list, query],
  );
  const text = query ?? (selected === null ? "" : crsLabel(selected));
  const optionId = (index: number) => `${listboxId}-${index}`;

  const close = () => {
    setOpen(false);
    setQuery(null);
  };
  const choose = (crs: CrsSummary) => {
    onSelect(crs);
    close();
  };
  const move = (to: number) => {
    setOpen(true);
    setActive(Math.min(Math.max(to, 0), Math.max(results.length - 1, 0)));
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const keys: Partial<Record<string, () => void>> = {
      ArrowDown: () => move(open ? active + 1 : active),
      ArrowUp: () => move(active - 1),
      Home: () => open && move(0),
      End: () => open && move(results.length - 1),
      Enter: () => {
        const crs = results[active];
        if (open && crs !== undefined) choose(crs);
      },
      Escape: close,
    };
    // Esc closes only the list; Enter only chooses while the list is open (else it applies).
    const action = keys[event.key];
    if (action === undefined) return;
    if (event.key === "Escape" && !open) return;
    if (event.key === "Enter" && !open) return;
    event.preventDefault();
    action();
  };

  let status = "";
  if (list.status === "loading") status = t("georef.crs.loading");
  else if (list.status === "error") status = t("georef.crs.loadError");
  else if (open) {
    status =
      results.length === 0
        ? t("georef.crs.empty", { query: query ?? "" })
        : t("georef.crs.results", { count: results.length });
  }

  return (
    <div className="combobox">
      <div className="field-input" data-invalid={invalid}>
        <input
          id={id}
          type="text"
          role="combobox"
          autoComplete="off"
          spellCheck={false}
          placeholder={t("georef.crs.placeholder")}
          aria-expanded={open}
          aria-controls={listboxId}
          aria-autocomplete="list"
          aria-activedescendant={open && results[active] ? optionId(active) : undefined}
          aria-invalid={invalid}
          aria-describedby={describedBy}
          value={text}
          onChange={(event) => {
            setQuery(event.target.value);
            setActive(0);
            setOpen(true);
          }}
          onClick={() => setOpen(true)}
          onBlur={close}
          onKeyDown={onKeyDown}
        />
      </div>
      <div className="visually-hidden" role="status">
        {status}
      </div>
      {open && (
        <div id={listboxId} className="listbox" role="listbox" aria-label={t("georef.crs.label")}>
          {list.status !== "ready" || results.length === 0 ? (
            <div className="listbox-note">{status}</div>
          ) : (
            results.map((crs, index) => (
              // Options are chosen with the keyboard from the input; the click is for the pointer.
              // biome-ignore lint/a11y/useKeyWithClickEvents: keys are handled on the combobox input
              <div
                key={crs.epsg}
                id={optionId(index)}
                role="option"
                tabIndex={-1}
                aria-selected={crs.epsg === selected?.epsg}
                data-active={index === active}
                // Keeps the focus in the input, so that blur does not close the list first.
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => choose(crs)}
              >
                <span className="option-name">{crsLabel(crs)}</span>
                <span className="option-area">{crs.area}</span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
