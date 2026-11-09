import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import {
  type MaterialDetail as Detail,
  type MaterialProperty,
  renameMaterial,
  setPropertyValue,
  useApi,
} from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { TextField } from "../ui/TextField";
import { MORE_DOTS } from "./glyphs";
import { nameFailureMessage } from "./messages";
import { PropertyTable } from "./PropertyTable";
import { designCodeLabel, type PropertyTab, tabOf } from "./units";

const TABS = ["mechanical", "thermal", "graphics", "environmental"] as const;
type Tab = (typeof TABS)[number];
const AVAILABLE: readonly Tab[] = ["mechanical", "thermal"];

type Props = {
  material: Detail;
  renaming: boolean;
  setRenaming: (renaming: boolean) => void;
  /** The number of objects using the material when the user tried to delete it, else null. */
  inUse: number | null;
  onDelete: () => void;
  onFailure: (error: unknown) => void;
  announce: (message: string) => void;
};

/** The detail of one project material: header, behaviour law, tabs and the property table. */
export function MaterialDetail({
  material,
  renaming,
  setRenaming,
  inUse,
  onDelete,
  onFailure,
  announce,
}: Props) {
  const { t } = useI18n();
  const api = useApi();
  const [tab, setTab] = useState<Tab>("mechanical");
  const [menuOpen, setMenuOpen] = useState(false);
  const ids = useId();
  const tabButtons = useRef<Partial<Record<Tab, HTMLButtonElement | null>>>({});

  const save = async (property: MaterialProperty, value: number | null) => {
    await setPropertyValue(api, material.id, property, value);
    if (value === null && property.library_value !== null) {
      const name = t(`materials.property.${property.key}` as MessageKey);
      announce(t("materials.reset.done", { property: name }));
    }
  };

  const onTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    // Arrows move from the tab that has focus, which may be a disabled one.
    const at = TABS.findIndex((name) => tabButtons.current[name] === event.currentTarget);
    const count = TABS.length;
    const targets: Partial<Record<string, number>> = {
      ArrowRight: (at + 1) % count,
      ArrowLeft: (at + count - 1) % count,
      Home: 0,
      End: count - 1,
    };
    const next = TABS[targets[event.key] ?? -1];
    if (next === undefined) return;
    event.preventDefault();
    // A disabled tab can be reached by the arrows but cannot be opened.
    if (AVAILABLE.includes(next)) setTab(next);
    tabButtons.current[next]?.focus();
  };

  const shown = properties(material, tab);
  const law = material.law;
  const source = law.source;
  const lawNote = t("materials.law.note");
  const category = t(`materials.category.${material.category}` as MessageKey);
  const code =
    material.design_code === null
      ? t("materials.detail.noCode")
      : designCodeLabel(material.design_code);

  return (
    <div className="material-detail">
      <div className="detail-header">
        <div className="detail-title">
          {renaming ? (
            <RenameField
              material={material}
              onDone={() => setRenaming(false)}
              onFailure={onFailure}
            />
          ) : (
            <h3 className="detail-name">{material.name}</h3>
          )}
          <p className="detail-sub">{t("materials.detail.designCode", { category, code })}</p>
        </div>
        {/* biome-ignore lint/a11y/noStaticElementInteractions: Escape on the button or its menu closes the menu. */}
        <div
          className="menu-anchor"
          onKeyDown={(event) => {
            if (event.key === "Escape" && menuOpen) {
              event.preventDefault();
              setMenuOpen(false);
            }
          }}
        >
          <button
            type="button"
            className="icon-button"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            aria-label={t("materials.menu.label", { name: material.name })}
            onClick={() => setMenuOpen((open) => !open)}
          >
            <span aria-hidden="true">{MORE_DOTS}</span>
          </button>
          {menuOpen && (
            <div className="menu" role="menu">
              <button
                type="button"
                role="menuitem"
                ref={(element) => element?.focus()}
                onClick={() => {
                  setMenuOpen(false);
                  setRenaming(true);
                }}
              >
                {t("materials.action.rename")}
              </button>

              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMenuOpen(false);
                  onDelete();
                }}
              >
                {t("materials.action.delete")}
              </button>
            </div>
          )}
        </div>
      </div>
      {inUse !== null && (
        <p className="banner banner-info" role="status">
          {t(inUse === 1 ? "materials.delete.inUseOne" : "materials.delete.inUseOther", {
            count: inUse,
            name: material.name,
          })}
        </p>
      )}
      <div className="law-card">
        <p>{t("materials.law.line", { law: t(`materials.law.${law.kind}` as MessageKey) })}</p>
        <p className="law-note">
          {source === null || !("document" in source)
            ? lawNote
            : t("materials.law.withSource", {
                source: t("materials.source.clause", {
                  document: source.document,
                  clause: source.clause,
                }),
                note: lawNote,
              })}
        </p>
      </div>
      <div role="tablist" aria-label={t("materials.tab.label")} className="tab-list">
        {TABS.map((name) => {
          const available = AVAILABLE.includes(name);
          return (
            <button
              key={name}
              type="button"
              role="tab"
              id={`${ids}-tab-${name}`}
              className="tab"
              aria-selected={name === tab}
              aria-disabled={available ? undefined : true}
              aria-controls={`${ids}-panel`}
              tabIndex={name === tab ? 0 : -1}
              title={available ? undefined : t("materials.tab.unavailable")}
              ref={(element) => {
                tabButtons.current[name] = element;
              }}
              onClick={() => available && setTab(name)}
              onKeyDown={onTabKeyDown}
            >
              {t(`materials.tab.${name}` as MessageKey)}
            </button>
          );
        })}
      </div>
      <div role="tabpanel" id={`${ids}-panel`} aria-labelledby={`${ids}-tab-${tab}`}>
        <PropertyTable properties={shown} onSave={save} onFailure={onFailure} />
      </div>
    </div>
  );
}

/** The properties of `tab` that a material shows: those it has and those it could have. */
function properties(material: Detail, tab: Tab): MaterialProperty[] {
  const wanted: PropertyTab = tab === "thermal" ? "thermal" : "mechanical";
  return material.properties.filter((property) => tabOf(property) === wanted);
}

function RenameField({
  material,
  onDone,
  onFailure,
}: {
  material: Detail;
  onDone: () => void;
  onFailure: (error: unknown) => void;
}) {
  const { t } = useI18n();
  const api = useApi();
  const [name, setName] = useState(material.name);
  const [error, setError] = useState<string | undefined>();
  const id = useId();
  const field = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const input = field.current?.querySelector("input");
    input?.focus();
    input?.select();
  }, []);

  const commit = async () => {
    const trimmed = name.trim();
    if (trimmed === material.name) return onDone();
    try {
      await renameMaterial(api, material.id, trimmed);
      onDone();
    } catch (failure) {
      const key = nameFailureMessage(failure);
      if (key !== null) {
        setError(t(key));
      } else {
        onFailure(failure);
        onDone();
      }
    }
  };

  return (
    <div ref={field}>
      <TextField
        id={id}
        label={t("materials.rename.label")}
        value={name}
        onChange={setName}
        error={error}
        inputMode="text"
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            void commit();
          } else if (event.key === "Escape") {
            event.preventDefault();
            onDone();
          }
        }}
      />
    </div>
  );
}
