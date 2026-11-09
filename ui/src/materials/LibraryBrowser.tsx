import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import {
  addFromLibrary,
  type GradeDetail,
  type GradeSummary,
  getGrade,
  listMaterials,
  MATERIAL_CATEGORIES,
  type MaterialCategory,
  searchLibrary,
  useApi,
} from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { Banner } from "../ui/Banner";
import { TextField } from "../ui/TextField";
import { BACK_ARROW, CHECK_MARK, SEPARATOR } from "./glyphs";
import { gradeName } from "./MaterialList";
import { nameFailureMessage } from "./messages";
import { PropertyTable } from "./PropertyTable";
import { designCodeLabel } from "./units";
import { useDebounced } from "./useDebounced";

const PAGE = 50;

type Props = {
  /** Null when there is nothing to go back to (the project has no material yet). */
  back: { label: string; onBack: () => void } | null;
  onAdded: (id: string, name: string) => void;
  onFailure: (error: unknown) => void;
};

type Page =
  | { status: "loading" }
  | { status: "ready"; items: GradeSummary[]; total: number }
  | { status: "error" };

/** The "Add from library" sub-view: search, filter, preview and add. */
export function LibraryBrowser({ back, onAdded, onFailure }: Props) {
  const { t } = useI18n();
  const api = useApi();
  const [search, setSearch] = useState("");
  const query = useDebounced(search, 200);
  const [category, setCategory] = useState<MaterialCategory | null>(null);
  const [page, setPage] = useState<Page>({ status: "loading" });
  const [selected, setSelected] = useState<GradeSummary | null>(null);
  const [grade, setGrade] = useState<GradeDetail | null>(null);
  const [name, setName] = useState("");
  const [nameError, setNameError] = useState<string | undefined>();
  const [busy, setBusy] = useState(false);
  const [nameId, searchId] = [useId(), useId()];
  // The grades of the whole project, not the filtered list of the left pane.
  const [projectGrades, setProjectGrades] = useState<ReadonlySet<string | null>>(new Set());
  const list = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const prefix = useId();
  const latest = useRef(0);

  useEffect(() => {
    let current = true;
    listMaterials(api).then(
      (items) => current && setProjectGrades(new Set(items.map((item) => item.grade))),
      () => undefined,
    );
    return () => {
      current = false;
    };
  }, [api]);

  // The first field takes the focus when the sub-view opens.
  useEffect(() => root.current?.querySelector("input")?.focus(), []);

  const load = (offset: number, replace: boolean) => {
    latest.current += 1;
    const mine = latest.current;
    searchLibrary(api, { search: query, category, offset, limit: PAGE }).then(
      (result) => {
        // An older answer must not replace a newer one.
        if (mine !== latest.current) return;
        setPage((current) => ({
          status: "ready",
          items: [...(replace || current.status !== "ready" ? [] : current.items), ...result.items],
          total: result.total,
        }));
      },
      () => mine === latest.current && setPage({ status: "error" }),
    );
  };

  // biome-ignore lint/correctness/useExhaustiveDependencies: `load` reads the same query and category.
  useEffect(() => load(0, true), [query, category, api]);

  const select = (item: GradeSummary) => {
    setSelected(item);
    setName(item.name);
    setNameError(undefined);
  };

  useEffect(() => {
    if (selected === null) return setGrade(null);
    let current = true;
    getGrade(api, selected.key).then(
      (detail) => current && setGrade(detail),
      (error: unknown) => current && onFailure(error),
    );
    return () => {
      current = false;
    };
  }, [api, selected, onFailure]);

  const items = page.status === "ready" ? page.items : [];
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const at = items.findIndex((item) => item.key === selected?.key);
    const target = { ArrowDown: Math.min(at + 1, items.length - 1), ArrowUp: Math.max(at - 1, 0) }[
      event.key as "ArrowDown" | "ArrowUp"
    ];
    if (target === undefined) return;
    event.preventDefault();
    const next = items[target];
    if (next) select(next);
  };

  const add = async () => {
    if (selected === null || busy) return;
    setBusy(true);
    try {
      const trimmed = name.trim();
      const id = await addFromLibrary(
        api,
        selected.key,
        trimmed === selected.name ? undefined : trimmed,
      );
      onAdded(id, trimmed);
    } catch (failure) {
      const key = nameFailureMessage(failure);
      if (key !== null) {
        setNameError(t(key));
        root.current?.querySelector<HTMLInputElement>(`#${CSS.escape(nameId)}`)?.focus();
      } else {
        onFailure(failure);
      }
    } finally {
      setBusy(false);
    }
  };

  const shownProperties = grade?.properties.filter((property) => property.value !== null) ?? [];

  return (
    <div className="subview library" ref={root}>
      {back !== null && (
        <button type="button" className="link-button back-link" onClick={back.onBack}>
          {BACK_ARROW} {back.label}
        </button>
      )}
      <h3 className="detail-name">{t("materials.library.title")}</h3>
      <TextField
        id={searchId}
        label={t("materials.library.search.label")}
        value={search}
        onChange={setSearch}
        inputMode="text"
      />
      <fieldset className="chips">
        <legend className="visually-hidden">{t("materials.category.label")}</legend>
        {MATERIAL_CATEGORIES.map((value) => (
          <button
            key={value}
            type="button"
            className="chip"
            aria-pressed={category === value}
            onClick={() => setCategory(category === value ? null : value)}
          >
            {category === value && <span aria-hidden="true">{`${CHECK_MARK} `}</span>}
            {t(`materials.category.${value}` as MessageKey)}
          </button>
        ))}
      </fieldset>
      <div className="library-panes">
        <div>
          {page.status === "error" && <Banner>{t("materials.library.loadError")}</Banner>}
          {page.status === "ready" && items.length === 0 && (
            <p className="empty-note">{t("materials.library.empty", { query })}</p>
          )}
          <div
            ref={list}
            className="material-list library-results"
            role="listbox"
            tabIndex={0}
            aria-label={t("materials.library.results")}
            aria-activedescendant={selected === null ? undefined : `${prefix}-${selected.key}`}
            onKeyDown={onKeyDown}
          >
            {items.map((item) => (
              // biome-ignore lint/a11y/useKeyWithClickEvents: keys are handled by the listbox.
              <div
                key={item.key}
                id={`${prefix}-${item.key}`}
                role="option"
                tabIndex={-1}
                aria-selected={item.key === selected?.key}
                className="material-row"
                onClick={() => select(item)}
              >
                <span className="row-text">
                  <span className="row-name">{item.name}</span>
                  <span className="row-meta">
                    {item.design_code === null
                      ? gradeName(item.key)
                      : designCodeLabel(item.design_code)}
                    {projectGrades.has(item.key) &&
                      `${SEPARATOR}${t("materials.library.inProject")}`}
                  </span>
                </span>
              </div>
            ))}
          </div>
          {page.status === "ready" && items.length < page.total && (
            <button
              type="button"
              className="button button-small"
              onClick={() => load(items.length, false)}
            >
              {t("materials.library.more")}
            </button>
          )}
        </div>
        <div className="library-preview">
          {selected !== null && grade !== null && (
            <>
              <h4 className="preview-title">
                {t("materials.library.preview", { name: grade.name })}
              </h4>
              <PropertyTable properties={shownProperties} onSave={null} onFailure={onFailure} />
              <TextField
                id={nameId}
                label={t("materials.library.name")}
                value={name}
                onChange={(value) => {
                  setName(value);
                  setNameError(undefined);
                }}
                help={t("materials.library.nameHelp")}
                error={nameError}
                inputMode="text"
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void add();
                  }
                }}
              />
              <button
                type="button"
                className="button button-primary"
                disabled={busy}
                onClick={() => void add()}
              >
                {t("materials.library.add")}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
