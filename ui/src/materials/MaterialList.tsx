import { type KeyboardEvent, useId } from "react";
import type { MaterialSummary } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { SEPARATOR } from "./glyphs";

/** The grade's own name: the library key without its design code ("en1992-1-1/C25/30" → "C25/30"). */
export const gradeName = (grade: string): string => grade.slice(grade.indexOf("/") + 1);

type Props = {
  items: readonly MaterialSummary[];
  selected: string | null;
  onSelect: (id: string) => void;
  onDelete: () => void;
  onRename: () => void;
  listRef: React.RefObject<HTMLDivElement | null>;
};

/**
 * The project's materials as one tab stop: the arrow keys move the selection, Delete starts the
 * deletion and F2 the renaming (aria-activedescendant keeps focus on the list).
 */
export function MaterialList({ items, selected, onSelect, onDelete, onRename, listRef }: Props) {
  const { t } = useI18n();
  const prefix = useId();
  const index = items.findIndex((item) => item.id === selected);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const last = items.length - 1;
    const targets: Partial<Record<string, number>> = {
      ArrowDown: Math.min(index + 1, last),
      ArrowUp: Math.max(index - 1, 0),
      Home: 0,
      End: last,
    };
    const target = targets[event.key];
    if (target !== undefined) {
      event.preventDefault();
      const item = items[target];
      if (item) onSelect(item.id);
    } else if (event.key === "Delete" && index !== -1) {
      event.preventDefault();
      onDelete();
    } else if (event.key === "F2" && index !== -1) {
      event.preventDefault();
      onRename();
    }
  };

  return (
    <div
      ref={listRef}
      className="material-list"
      role="listbox"
      tabIndex={0}
      aria-label={t("materials.list.label")}
      aria-activedescendant={selected === null ? undefined : `${prefix}-${selected}`}
      onKeyDown={onKeyDown}
    >
      {items.map((item) => (
        // biome-ignore lint/a11y/useKeyWithClickEvents: keys are handled by the listbox.
        <div
          key={item.id}
          id={`${prefix}-${item.id}`}
          role="option"
          tabIndex={-1}
          aria-selected={item.id === selected}
          className="material-row"
          onClick={() => onSelect(item.id)}
        >
          <span className="row-text">
            <span className="row-name">{item.name}</span>
            <span className="row-meta">
              {`${t(`materials.category.${item.category}` as MessageKey)}${SEPARATOR}${
                item.grade === null ? t("materials.list.customGrade") : gradeName(item.grade)
              }`}
            </span>
          </span>
          {item.used_by > 0 && (
            <span className="row-used">{t("materials.list.usedBy", { count: item.used_by })}</span>
          )}
        </div>
      ))}
    </div>
  );
}
