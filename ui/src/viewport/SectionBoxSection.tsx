import { type KeyboardEvent, useId, useState } from "react";
import { formatNumber, LENGTH_DECIMALS, parseNumber } from "../georef/units";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { shortcutProps } from "../ui/shortcut";
import type { ViewReport } from "./ViewportBridge";
import {
  AXES,
  AXIS_NAMES,
  type Axis,
  extentIsValid,
  type SectionBox,
  type Side,
  withExtent,
} from "./viewState";

/** The chord that switches the section box on and off. */
const ALT_B = "Alt+B";

type FieldKey = `${Side}${Axis}`;
type ErrorKey = "view.section.errorRange" | "view.section.errorNumber";

type Props = { report: ViewReport };

/**
 * The "Section box" section of the View panel (board 3.1, amended by 3.26): on and off, how cut
 * faces look, and the six extents as typed numbers in metres, the keyboard equivalent of dragging
 * a face. Enter applies a value, Esc restores it; a minimum that is not below its maximum is
 * refused here, before anything is sent.
 */
export function SectionBoxSection({ report }: Props) {
  const { t } = useI18n();
  const ids = useId();
  const { view, send, selectionCount } = report;
  const box = view?.sectionBox ?? null;
  const enabled = box?.enabled === true;
  // The first field with a refused value, and why.
  const [problem, setProblem] = useState<{ field: FieldKey; key: ErrorKey } | null>(null);

  const off = () => send({ kind: "section_box", enabled: false });
  const turnOffProps = shortcutProps(t, t("view.section.turnOff"), ALT_B);

  return (
    <div className="view-section">
      <label className="view-check">
        <input
          type="checkbox"
          checked={enabled}
          disabled={view === null}
          aria-keyshortcuts={ALT_B}
          onChange={(event) => send({ kind: "section_box", enabled: event.target.checked })}
        />
        {t("view.section.active")}
        <kbd>{ALT_B}</kbd>
      </label>
      <div className="view-row">
        <label htmlFor={`${ids}-cut`}>{t("view.section.cutFaces")}</label>
        <select
          id={`${ids}-cut`}
          className="select"
          value={view?.cutFaces ?? "tinted"}
          disabled={view === null}
          onChange={(event) =>
            send({
              kind: "cut_faces",
              style: event.target.value === "none" ? "none" : "tinted",
            })
          }
        >
          <option value="tinted">{t("view.section.tinted")}</option>
          <option value="none">{t("view.section.none")}</option>
        </select>
      </div>
      {enabled && box !== null && (
        <>
          <ExtentGrid
            box={box}
            problem={problem}
            onProblem={setProblem}
            onApply={(next) => send({ kind: "section_box_extents", min: next.min, max: next.max })}
            describedBy={`${ids}-help`}
            errorId={`${ids}-error`}
          />
          {problem !== null && (
            <p className="field-error" id={`${ids}-error`}>
              {t(problem.key)}
            </p>
          )}
          <p className="section-help" id={`${ids}-help`}>
            {t("view.section.help")}
          </p>
          <div className="view-buttons">
            <button
              type="button"
              className="button button-small"
              aria-disabled={selectionCount === 0}
              onClick={() => {
                if (selectionCount > 0) send({ kind: "section_box_fit_selection" });
              }}
            >
              {t("view.section.fit")}
            </button>
            <button
              type="button"
              className="button button-small"
              onClick={() => {
                setProblem(null);
                send({ kind: "section_box_reset" });
              }}
            >
              {t("view.section.reset")}
            </button>
            <button type="button" className="ghost-button" {...turnOffProps} onClick={off}>
              {t("view.section.turnOff")}
            </button>
          </div>
        </>
      )}
    </div>
  );
}

type GridProps = {
  box: SectionBox;
  problem: { field: FieldKey; key: ErrorKey } | null;
  onProblem: (problem: { field: FieldKey; key: ErrorKey } | null) => void;
  onApply: (box: SectionBox) => void;
  describedBy: string;
  errorId: string;
};

function ExtentGrid({ box, problem, onProblem, onApply, describedBy, errorId }: GridProps) {
  const { t } = useI18n();
  return (
    <fieldset className="extent-grid">
      <legend className="visually-hidden">{t("view.section.extents")}</legend>
      <span />
      <span className="extent-head">{t("view.section.min")}</span>
      <span className="extent-head">{t("view.section.max")}</span>
      {AXES.map((axis) => (
        <ExtentRow
          key={axis}
          axis={axis}
          box={box}
          problem={problem}
          onProblem={onProblem}
          onApply={onApply}
          describedBy={describedBy}
          errorId={errorId}
        />
      ))}
    </fieldset>
  );
}

function ExtentRow({ axis, ...props }: GridProps & { axis: Axis }) {
  const { t } = useI18n();
  return (
    <>
      <span className="extent-axis" aria-hidden="true">
        {AXIS_NAMES[axis]}
      </span>
      {(["min", "max"] as const).map((side) => (
        <ExtentField
          key={side}
          {...props}
          axis={axis}
          side={side}
          label={`${t(`view.section.${side}` as MessageKey)} ${AXIS_NAMES[axis]}`}
        />
      ))}
    </>
  );
}

function ExtentField({
  box,
  axis,
  side,
  label,
  problem,
  onProblem,
  onApply,
  describedBy,
  errorId,
}: GridProps & { axis: Axis; side: Side; label: string }) {
  const { language, t } = useI18n();
  const field: FieldKey = `${side}${axis}`;
  const current = (side === "min" ? box.min : box.max)[axis];
  // What the user is typing; null shows the viewport's value.
  const [draft, setDraft] = useState<string | null>(null);
  const invalid = problem?.field === field;

  const restore = () => {
    setDraft(null);
    if (invalid) onProblem(null);
  };
  const apply = () => {
    if (draft === null) return;
    const value = parseNumber(draft);
    if (value === null) {
      onProblem({ field, key: "view.section.errorNumber" });
    } else if (!extentIsValid(box, axis, side, value)) {
      onProblem({ field, key: "view.section.errorRange" });
    } else {
      restore();
      if (value !== current) onApply(withExtent(box, axis, side, value));
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      apply();
    } else if (event.key === "Escape") {
      // The canvas must not also hear it, nor a dialog around.
      event.preventDefault();
      event.stopPropagation();
      restore();
    }
  };

  return (
    <div className="field-input extent-field" data-invalid={invalid}>
      <input
        type="text"
        inputMode="decimal"
        autoComplete="off"
        spellCheck={false}
        aria-label={label}
        aria-invalid={invalid}
        aria-describedby={invalid ? `${describedBy} ${errorId}` : describedBy}
        value={draft ?? formatNumber(current, LENGTH_DECIMALS, language)}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={onKeyDown}
        onBlur={apply}
      />
      <span aria-hidden="true">{t("viewport.typed.unit")}</span>
    </div>
  );
}
