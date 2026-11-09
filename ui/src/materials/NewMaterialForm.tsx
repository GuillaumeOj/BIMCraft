import { useEffect, useId, useRef, useState } from "react";
import { createMaterial, MATERIAL_CATEGORIES, type MaterialCategory, useApi } from "../api";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { TextField } from "../ui/TextField";
import { BACK_ARROW } from "./glyphs";
import { failureMessage } from "./messages";

type Props = {
  onBack: () => void;
  backLabel: string;
  onCreated: (id: string, name: string) => void;
  onFailure: (error: unknown) => void;
};

/** The "New material" sub-view: a name and a category; the values come later, in the detail. */
export function NewMaterialForm({ onBack, backLabel, onCreated, onFailure }: Props) {
  const { t } = useI18n();
  const api = useApi();
  const [name, setName] = useState("");
  const [category, setCategory] = useState<MaterialCategory>("concrete");
  const [error, setError] = useState<string | undefined>();
  const [busy, setBusy] = useState(false);
  const [nameId, categoryId, titleId] = [useId(), useId(), useId()];
  const form = useRef<HTMLFormElement>(null);

  useEffect(() => form.current?.querySelector("input")?.focus(), []);

  const submit = async () => {
    if (busy) return;
    setBusy(true);
    try {
      const trimmed = name.trim();
      const id = await createMaterial(api, trimmed, category);
      onCreated(id, trimmed);
    } catch (failure) {
      const key = failureMessage(failure);
      if (key === "materials.error.invalid_name" || key === "materials.error.duplicate_name") {
        setError(t(key));
        form.current?.querySelector("input")?.focus();
      } else {
        onFailure(failure);
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      ref={form}
      className="subview"
      aria-labelledby={titleId}
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <button type="button" className="link-button back-link" onClick={onBack}>
        {BACK_ARROW} {backLabel}
      </button>
      <h3 id={titleId} className="detail-name">
        {t("materials.new.title")}
      </h3>
      <TextField
        id={nameId}
        label={t("materials.new.name")}
        value={name}
        onChange={(value) => {
          setName(value);
          setError(undefined);
        }}
        error={error}
        inputMode="text"
      />
      <div className="field field-stacked">
        <div className="field-label">
          <label htmlFor={categoryId}>{t("materials.new.category")}</label>
        </div>
        <select
          id={categoryId}
          className="select"
          value={category}
          onChange={(event) =>
            setCategory(MATERIAL_CATEGORIES.find((c) => c === event.target.value) ?? category)
          }
        >
          {MATERIAL_CATEGORIES.map((value) => (
            <option key={value} value={value}>
              {t(`materials.category.${value}` as MessageKey)}
            </option>
          ))}
        </select>
      </div>
      <div>
        <button type="submit" className="button button-primary" disabled={busy}>
          {t("materials.new.create")}
        </button>
      </div>
    </form>
  );
}
