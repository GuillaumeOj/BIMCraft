import { type KeyboardEvent, useId, useState } from "react";
import { useI18n } from "../i18n/I18nProvider";
import "./forms.css";

type TextFieldProps = {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** The unit written inside the field. */
  unit?: string | undefined;
  /** The message under the field; the field is then invalid. */
  error?: string | undefined;
  /** Plain-words help, shown by the info button and read as the field's description. */
  help?: string;
  disabled?: boolean;
  readOnly?: boolean;
  /** "compact" puts the label on the left, as in the coordinates panel. */
  variant?: "stacked" | "compact";
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
  /** Ids of other elements that describe the field. */
  describedBy?: string | undefined;
  /** Makes an error come from a group message shown elsewhere. */
  invalid?: boolean;
};

/** A labelled text field for numbers (UI-06: visible label, linked error and help). */
export function TextField({
  id,
  label,
  value,
  onChange,
  unit,
  error,
  help,
  disabled = false,
  readOnly = false,
  variant = "stacked",
  onKeyDown,
  describedBy,
  invalid = false,
}: TextFieldProps) {
  const { t } = useI18n();
  const [helpOpen, setHelpOpen] = useState(false);
  const [errorId, helpId] = [`${useId()}-error`, `${useId()}-help`];
  const described = [help ? helpId : null, error ? errorId : null, describedBy]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={`field field-${variant}`}>
      <div className="field-label">
        <label htmlFor={id}>{label}</label>
        {help && (
          <button
            type="button"
            className="info-button"
            aria-expanded={helpOpen}
            aria-controls={helpId}
            aria-label={t("georef.help", { label })}
            onClick={() => setHelpOpen((open) => !open)}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
              <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" />
              <path d="M8 7v4M8 5v.01" stroke="currentColor" strokeWidth="1.5" />
            </svg>
          </button>
        )}
      </div>
      <div className="field-input" data-invalid={error !== undefined || invalid}>
        <input
          id={id}
          type="text"
          inputMode="decimal"
          autoComplete="off"
          spellCheck={false}
          value={value}
          disabled={disabled}
          readOnly={readOnly}
          aria-invalid={error !== undefined || invalid}
          aria-describedby={described || undefined}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={onKeyDown}
        />
        {unit && <span aria-hidden="true">{unit}</span>}
      </div>
      {help && (
        <p id={helpId} className="field-help" hidden={!helpOpen}>
          {help}
        </p>
      )}
      {error && (
        <p id={errorId} className="field-error">
          {error}
        </p>
      )}
    </div>
  );
}
