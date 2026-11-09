import { useEffect, useId, useRef } from "react";
import { useI18n } from "../i18n/I18nProvider";

type Props = { name: string; onKeep: () => void; onDelete: () => void };

/** The inline confirmation that replaces the footer; the safe button has the focus (F1-H). */
export function DeleteConfirmation({ name, onKeep, onDelete }: Props) {
  const { t } = useI18n();
  const titleId = useId();
  const keep = useRef<HTMLButtonElement>(null);
  useEffect(() => keep.current?.focus(), []);
  return (
    <div className="dialog-confirm" role="alertdialog" aria-labelledby={titleId}>
      <p id={titleId} className="confirm-title">
        {t("materials.delete.title", { name })}
      </p>
      <p>{t("materials.delete.body")}</p>
      <div className="dialog-actions">
        <button type="button" className="button" ref={keep} onClick={onKeep}>
          {t("materials.delete.keep")}
        </button>
        <button type="button" className="button button-danger" onClick={onDelete}>
          {t("materials.delete.confirm")}
        </button>
      </div>
    </div>
  );
}
