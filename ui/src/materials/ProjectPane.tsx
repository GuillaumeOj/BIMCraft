import type { RefObject } from "react";
import { MATERIAL_CATEGORIES, type MaterialCategory, type MaterialSummary } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { Banner } from "../ui/Banner";
import { MaterialList } from "./MaterialList";
import type { Loaded } from "./useMaterialsData";

type Props = {
  searchId: string;
  search: string;
  setSearch: (search: string) => void;
  /** The search text that was applied (after the delay), for the "no match" message. */
  query: string;
  category: MaterialCategory | null;
  setCategory: (category: MaterialCategory | null) => void;
  list: Loaded<MaterialSummary[]>;
  /** The project has no material at all, and no filter hides any. */
  empty: boolean;
  selected: string | null;
  onSelect: (id: string) => void;
  onDelete: () => void;
  onRename: () => void;
  onLibrary: () => void;
  onNew: () => void;
  listRef: RefObject<HTMLDivElement | null>;
  libraryRef: RefObject<HTMLButtonElement | null>;
  newRef: RefObject<HTMLButtonElement | null>;
};

/** The left pane: search, category filter, the project's materials and the two add buttons. */
export function ProjectPane(props: Props) {
  const { t } = useI18n();
  const { list, empty, query, category } = props;
  const items = list.status === "ready" ? list.value : [];
  const filtering = query.trim() !== "" || category !== null;
  return (
    <div className="materials-left">
      {!empty && (
        <>
          <div className="search-field">
            <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
              <circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
              <path d="M10.5 10.5 14 14" stroke="currentColor" strokeWidth="1.5" />
            </svg>
            <input
              id={props.searchId}
              type="search"
              className="search"
              aria-label={t("materials.search.label")}
              placeholder={t("materials.search.placeholder")}
              value={props.search}
              onChange={(event) => props.setSearch(event.target.value)}
            />
          </div>
          <select
            className="select"
            aria-label={t("materials.category.label")}
            value={category ?? ""}
            onChange={(event) =>
              props.setCategory(MATERIAL_CATEGORIES.find((c) => c === event.target.value) ?? null)
            }
          >
            <option value="">{t("materials.category.all")}</option>
            {MATERIAL_CATEGORIES.map((value) => (
              <option key={value} value={value}>
                {t(`materials.category.${value}` as MessageKey)}
              </option>
            ))}
          </select>
        </>
      )}
      {list.status === "loading" && <p className="empty-note">{t("materials.loading")}</p>}
      {list.status === "error" && <Banner>{t("materials.error.load")}</Banner>}
      {empty && <p className="empty-note">{t("materials.empty")}</p>}
      {list.status === "ready" && items.length === 0 && filtering && (
        <p className="empty-note">{t("materials.list.noMatch", { query })}</p>
      )}
      {items.length > 0 && (
        <MaterialList
          items={items}
          selected={props.selected}
          onSelect={props.onSelect}
          onDelete={props.onDelete}
          onRename={props.onRename}
          listRef={props.listRef}
        />
      )}
      <div className="materials-actions">
        <button
          type="button"
          className="button button-primary"
          ref={props.libraryRef}
          onClick={props.onLibrary}
        >
          {t("materials.action.addFromLibrary")}
        </button>
        <button type="button" className="button" ref={props.newRef} onClick={props.onNew}>
          {t("materials.action.new")}
        </button>
      </div>
    </div>
  );
}
