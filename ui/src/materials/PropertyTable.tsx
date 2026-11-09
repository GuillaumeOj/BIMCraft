import { type KeyboardEvent, useId, useRef, useState } from "react";
import { type MaterialProperty, materialRefusal } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { Language, MessageKey, MessageParams } from "../i18n/i18n";
import { ErrorIcon } from "../ui/ErrorIcon";
import { failureMessage } from "./messages";
import {
  displayUnit,
  formatBounds,
  formatValue,
  formatWithUnit,
  parseValue,
  SYMBOLS,
} from "./units";

type Save = (property: MaterialProperty, value: number | null) => Promise<void>;

type TableProps = {
  properties: readonly MaterialProperty[];
  /** Null makes the table read-only (the grade preview). */
  onSave: Save | null;
  /** Called with a failure that is not about the field being edited. */
  onFailure: (error: unknown) => void;
};

/** The properties grouped by key: a partial factor has one line per annex in one row. */
function byKey(properties: readonly MaterialProperty[]): MaterialProperty[][] {
  const groups = new Map<string, MaterialProperty[]>();
  for (const property of properties) {
    groups.set(property.key, [...(groups.get(property.key) ?? []), property]);
  }
  return [...groups.values()];
}

const idOf = (property: MaterialProperty) => `${property.key}:${property.annex ?? ""}`;

/** The properties of one tab as a table: name and symbol, value in a fixed unit, and source. */
export function PropertyTable({ properties, onSave, onFailure }: TableProps) {
  const { t } = useI18n();
  const [editing, setEditing] = useState<string | null>(null);
  return (
    <table className="property-table">
      <thead>
        <tr>
          <th scope="col">{t("materials.table.property")}</th>
          <th scope="col" className="property-value-head">
            {t("materials.table.value")}
          </th>
          <th scope="col">{t("materials.table.source")}</th>
        </tr>
      </thead>
      <tbody>
        {byKey(properties).map((group) => (
          <PropertyRow
            key={group.map(idOf).join()}
            group={group}
            editing={editing}
            setEditing={setEditing}
            onSave={onSave}
            onFailure={onFailure}
          />
        ))}
      </tbody>
    </table>
  );
}

type RowProps = {
  group: readonly MaterialProperty[];
  editing: string | null;
  setEditing: (id: string | null) => void;
  onSave: Save | null;
  onFailure: (error: unknown) => void;
};

function PropertyRow({ group, editing, setEditing, onSave, onFailure }: RowProps) {
  const { t } = useI18n();
  const [first] = group;
  if (first === undefined) return null;
  const name = t(`materials.property.${first.key}` as MessageKey);
  const symbol = SYMBOLS[first.key];
  const edited = group.some((property) => property.source && "entered_by_user" in property.source);
  return (
    <tr className="property-row" data-user={edited}>
      <th scope="row" className="property-name">
        {symbol ? `${name} · ${symbol}` : name}
      </th>
      <td className="property-values">
        {group.map((property) => (
          <ValueCell
            key={idOf(property)}
            property={property}
            label={name}
            editing={editing === idOf(property)}
            setEditing={setEditing}
            onSave={onSave}
            onFailure={onFailure}
          />
        ))}
      </td>
      <td className="property-sources">
        {group.map((property) => (
          <SourceCell
            key={idOf(property)}
            property={property}
            label={name}
            onSave={onSave}
            onFailure={onFailure}
          />
        ))}
      </td>
    </tr>
  );
}

type CellProps = {
  property: MaterialProperty;
  label: string;
  onSave: Save | null;
  onFailure: (error: unknown) => void;
};

function annexPrefix(property: MaterialProperty, t: (key: MessageKey) => string) {
  return property.annex === undefined ? "" : `${t(`materials.annex.${property.annex}`)} `;
}

function ValueCell({
  property,
  label,
  editing,
  setEditing,
  onSave,
  onFailure,
}: CellProps & { editing: boolean; setEditing: (id: string | null) => void }) {
  const { t, language } = useI18n();
  const prefix = annexPrefix(property, t);
  const { value } = property;
  const shown =
    value === null ? t("materials.value.none") : formatWithUnit(property, value, language);
  if (editing && onSave !== null) {
    return (
      <ValueEditor
        property={property}
        label={`${prefix}${label}`}
        onDone={() => setEditing(null)}
        onSave={onSave}
        onFailure={onFailure}
      />
    );
  }
  if (onSave === null) {
    return (
      <p className="property-line">
        {prefix}
        {shown}
      </p>
    );
  }
  const editLabel = `${t("materials.value.edit", { property: label })}: ${prefix}${shown}`;
  const addLabel = `${t("materials.value.add")} ${prefix}${label}`;
  const open = () => setEditing(idOf(property));
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "F2") open();
  };
  if (value === null) {
    return (
      <p className="property-line">
        <span aria-hidden="true">{shown}</span>{" "}
        <button
          type="button"
          className="link-button"
          // The visible text comes first, so that the name contains it.
          aria-label={addLabel}
          onClick={open}
          onKeyDown={onKeyDown}
        >
          {t("materials.value.add")}
        </button>
      </p>
    );
  }
  return (
    <p className="property-line">
      <button
        type="button"
        className="value-button"
        aria-label={editLabel}
        onClick={open}
        onKeyDown={onKeyDown}
      >
        {prefix}
        {shown}
      </button>
    </p>
  );
}

function SourceCell({ property, label, onSave, onFailure }: CellProps) {
  const { t, language } = useI18n();
  const { source } = property;
  const user = source !== null && "entered_by_user" in source;
  const canReset = onSave !== null && user && property.library_value !== null;
  const [failed, setFailed] = useState(false);
  return (
    <div className="property-source">
      {annexPrefix(property, t) && <span className="annex-tag">{annexPrefix(property, t)}</span>}
      {source !== null && !user && (
        <span>
          {t("materials.source.clause", { document: source.document, clause: source.clause })}
        </span>
      )}
      {user && <span className="badge">{t("materials.source.user")}</span>}
      {canReset && (
        <span className="tooltip-anchor">
          <button
            type="button"
            className="link-button"
            aria-describedby={`${idOf(property)}-tip`}
            data-failed={failed}
            onClick={() => {
              onSave(property, null).catch((error: unknown) => {
                setFailed(true);
                onFailure(error);
              });
            }}
          >
            {t("materials.reset")}
            <span className="visually-hidden">{`  (${annexPrefix(property, t)}${label})`}</span>
          </button>
          <span className="tooltip" role="tooltip" id={`${idOf(property)}-tip`}>
            {t("materials.reset.tooltip", {
              value:
                property.library_value === null
                  ? ""
                  : formatWithUnit(property, property.library_value, language),
            })}
          </span>
        </span>
      )}
    </div>
  );
}

function ValueEditor({
  property,
  label,
  onDone,
  onSave,
  onFailure,
}: {
  property: MaterialProperty;
  label: string;
  onDone: () => void;
  onSave: Save;
  onFailure: (error: unknown) => void;
}) {
  const { t, language } = useI18n();
  const [text, setText] = useState(
    property.value === null ? "" : formatValue(property, property.value, language),
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const cancelled = useRef(false);
  const errorId = useId();
  const { unit } = displayUnit(property);

  const commit = async () => {
    if (busy || cancelled.current) return;
    const edit = readEdit(text, property, language);
    if (edit.kind === "unchanged") return onDone();
    if (edit.kind === "invalid") {
      setError(withRange(t("materials.error.invalid_value"), rangeHelp(property, language, t)));
      return;
    }
    setBusy(true);
    try {
      await onSave(property, edit.value);
      onDone();
    } catch (failure) {
      const message = fieldFailure(failure, property, language, t);
      if (message !== null) {
        setError(message);
      } else {
        onFailure(failure);
        onDone();
      }
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void commit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelled.current = true;
      onDone();
    }
  };

  return (
    <div className="value-editor">
      <div className="field-input" data-invalid={error !== null}>
        <input
          type="text"
          inputMode="decimal"
          autoComplete="off"
          spellCheck={false}
          // The row's own name is on screen; the field needs it for assistive technology.
          aria-label={t("materials.value.field", { property: label, unit })}
          aria-invalid={error !== null}
          aria-describedby={error === null ? undefined : errorId}
          ref={(element) => element?.focus()}
          value={text}
          onChange={(event) => setText(event.target.value)}
          onBlur={() => void commit()}
          onKeyDown={onKeyDown}
        />
        {unit && <span aria-hidden="true">{unit}</span>}
      </div>
      {error !== null && (
        <p id={errorId} className="field-error">
          <ErrorIcon />
          {error}
        </p>
      )}
    </div>
  );
}

/** The field's own message for a refusal about its value, or null for any other failure. */
function fieldFailure(
  failure: unknown,
  property: MaterialProperty,
  language: Language,
  t: (key: MessageKey, params?: MessageParams) => string,
): string | null {
  const reason = materialRefusal(failure)?.reason;
  if (reason === "required_property") return t(failureMessage(failure));
  if (reason !== "invalid_value") return null;
  return withRange(t(failureMessage(failure)), rangeHelp(property, language, t));
}

/** The message with the accepted range after it, when there is one. */
const withRange = (message: string, range: string | null): string =>
  range === null ? message : `${message} ${range}`;

/** "Between 0 and 0.5.", "At least 0.", "At most 1."; null for a property with no bound. */
function rangeHelp(
  property: MaterialProperty,
  language: Language,
  t: (key: MessageKey, params?: MessageParams) => string,
): string | null {
  const { min, max } = formatBounds(property, language);
  if (min !== null && max !== null) return t("materials.error.range", { min, max });
  if (min !== null) return t("materials.error.rangeMin", { min });
  return max === null ? null : t("materials.error.rangeMax", { max });
}

type Edit = { kind: "unchanged" } | { kind: "invalid" } | { kind: "value"; value: number | null };

/** What the typed `text` asks for: nothing new, a value (null clears it), or a malformed number. */
function readEdit(text: string, property: MaterialProperty, language: Language): Edit {
  const trimmed = text.trim();
  const before = property.value === null ? "" : formatValue(property, property.value, language);
  if (trimmed === before) return { kind: "unchanged" };
  if (trimmed === "") return { kind: "value", value: null };
  const value = parseValue(property, trimmed);
  return value === null ? { kind: "invalid" } : { kind: "value", value };
}
