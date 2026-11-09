import type { KeyboardEvent } from "react";
import { BUILDUP_FUNCTIONS, type BuildUpFunction, type MaterialSummary } from "../api";
import { formatNumber } from "../georef/units";
import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { ARROW_DOWN, ARROW_UP, REMOVE_MARK } from "../materials/glyphs";
import { ErrorIcon } from "../ui/ErrorIcon";
import { DEFAULT_PRIORITY, type DraftLayer, defaultCore, effectiveCore, PRIORITIES } from "./draft";
import type { RowErrors } from "./errors";

/** The value of the material select that opens the Materials dialog. */
export const MANAGE = "__manage__";
const USE_DEFAULT = "default";

export type LayerRowProps = {
  index: number;
  count: number;
  layer: DraftLayer;
  /** The usage's face label of the first or last layer, if this is one of them. */
  face: string | null;
  materials: readonly MaterialSummary[];
  errors: RowErrors;
  /** The core's thermal resistance of this layer; null when the material has no λ. */
  resistance: number | null | undefined;
  selected: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<DraftLayer>) => void;
  onManage: () => void;
  onMove: (to: number) => void;
  onRemove: () => void;
  onAdd: () => void;
};

const control = (name: string) => ({ "data-control": name });
const errorId = (layer: DraftLayer, field: string) => `${layer.key}-${field}-error`;

/** The materials a layer of `fn` may use: an air gap takes generic materials only. */
function allowed(materials: readonly MaterialSummary[], fn: BuildUpFunction) {
  return fn === "air_gap"
    ? materials.filter((material) => material.category === "generic")
    : materials;
}

/** One layer of the table, with the message row that follows it. */
export function LayerRow(props: LayerRowProps) {
  const { layer, index, count, errors } = props;
  const { t, language } = useI18n();
  const n = index + 1;

  // Alt+↑/↓ moves the layer from a button or the checkbox; text fields and selects keep the keys.
  const onKeyDown = (event: KeyboardEvent<HTMLTableRowElement>) => {
    const target = event.target;
    if (event.ctrlKey && event.key === "Enter") {
      event.preventDefault();
      props.onAdd();
    } else if (event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
      const movable =
        target instanceof HTMLButtonElement ||
        (target instanceof HTMLInputElement && target.type === "checkbox");
      if (!movable) return;
      event.preventDefault();
      props.onMove(event.key === "ArrowUp" ? index - 1 : index + 1);
    }
  };

  const messages = [errors.thickness, errors.material, errors.priority, errors.core].filter(
    (message): message is MessageKey => message !== undefined,
  );
  const withMaterial = (id: string) => ({
    "aria-describedby": errors.material ? errorId(layer, id) : undefined,
  });

  return (
    <>
      <tr
        className="layer-row"
        data-layer={layer.key}
        data-selected={props.selected}
        data-error={errors.inCoreSpan}
        onFocus={props.onSelect}
        onKeyDown={onKeyDown}
      >
        <th scope="row" className="layer-position">
          <span>{n}</span>
          {props.face !== null && <span className="layer-face">{props.face}</span>}
        </th>
        <td>
          <select
            className="select"
            aria-label={t("buildups.function.label", { n })}
            value={layer.function}
            onChange={(event) =>
              props.onChange({
                function:
                  BUILDUP_FUNCTIONS.find((fn) => fn === event.target.value) ?? layer.function,
              })
            }
            {...control("function")}
          >
            {BUILDUP_FUNCTIONS.map((fn) => (
              <option key={fn} value={fn}>
                {t(`buildups.function.${fn}` as MessageKey)}
              </option>
            ))}
          </select>
        </td>
        <td>
          <select
            className="select"
            aria-label={t("buildups.material.label", { n })}
            aria-invalid={errors.material !== undefined}
            value={layer.material ?? ""}
            onChange={(event) => {
              if (event.target.value === MANAGE) props.onManage();
              else props.onChange({ material: event.target.value || null });
            }}
            {...control("material")}
            {...withMaterial("material")}
          >
            <option value="">{t("buildups.material.choose")}</option>
            {allowed(props.materials, layer.function).map((material) => (
              <option key={material.id} value={material.id}>
                {material.name}
              </option>
            ))}
            <option value={MANAGE}>{t("buildups.material.manage")}</option>
          </select>
        </td>
        <td>
          <div
            className="field-input layer-thickness"
            data-invalid={errors.thickness !== undefined}
          >
            <input
              type="text"
              inputMode="decimal"
              autoComplete="off"
              spellCheck={false}
              aria-label={t("buildups.thickness.label", { n })}
              aria-invalid={errors.thickness !== undefined}
              aria-describedby={errors.thickness ? errorId(layer, "thickness") : undefined}
              value={layer.thickness}
              onChange={(event) => props.onChange({ thickness: event.target.value })}
              {...control("thickness")}
            />
          </div>
        </td>
        <td>
          <div className="priority-cell">
            <select
              className="select"
              aria-label={t("buildups.priority.label", { n })}
              aria-invalid={errors.priority !== undefined}
              value={layer.priority === null ? USE_DEFAULT : String(layer.priority)}
              onChange={(event) => {
                const chosen = PRIORITIES.find((value) => String(value) === event.target.value);
                props.onChange({ priority: chosen ?? null });
              }}
              {...control("priority")}
            >
              <option value={USE_DEFAULT}>
                {t("buildups.priority.default", { n: DEFAULT_PRIORITY[layer.function] })}
              </option>
              {PRIORITIES.map((value) => (
                <option key={value} value={String(value)}>
                  {value}
                </option>
              ))}
            </select>
            {layer.priority !== null && (
              <span className="badge">{t("buildups.priority.modified")}</span>
            )}
          </div>
        </td>
        <td className="layer-core">
          <input
            type="checkbox"
            aria-label={t("buildups.core.label", { n })}
            aria-invalid={errors.core !== undefined || errors.inCoreSpan}
            checked={effectiveCore(layer)}
            onChange={(event) => {
              const checked = event.target.checked;
              props.onChange({ core: checked === defaultCore(layer) ? null : checked });
            }}
            {...control("core")}
          />
        </td>
        <td
          className="layer-resistance"
          title={props.resistance == null ? t("buildups.resistance.why") : undefined}
        >
          {props.resistance == null
            ? t("buildups.resistance.none")
            : formatNumber(props.resistance, 2, language)}
        </td>
        <td className="layer-actions">
          <button
            type="button"
            className="icon-button"
            aria-label={t("buildups.layers.moveUp", { n })}
            aria-disabled={index === 0}
            onClick={() => props.onMove(index - 1)}
            {...control("up")}
          >
            <span aria-hidden="true">{ARROW_UP}</span>
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label={t("buildups.layers.moveDown", { n })}
            aria-disabled={index === count - 1}
            onClick={() => props.onMove(index + 1)}
            {...control("down")}
          >
            <span aria-hidden="true">{ARROW_DOWN}</span>
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label={t("buildups.layers.remove", { n })}
            aria-disabled={count === 1}
            onClick={props.onRemove}
            {...control("remove")}
          >
            <span aria-hidden="true">{REMOVE_MARK}</span>
          </button>
        </td>
      </tr>
      {messages.length > 0 && (
        <tr className="layer-messages" data-error={errors.inCoreSpan}>
          <td />
          <td colSpan={7}>
            {errors.thickness && (
              <Message id={errorId(layer, "thickness")} text={t(errors.thickness)} />
            )}
            {errors.material && (
              <Message id={errorId(layer, "material")} text={t(errors.material)} />
            )}
            {errors.priority && (
              <Message id={errorId(layer, "priority")} text={t(errors.priority)} />
            )}
            {errors.core && <Message id={errorId(layer, "core")} text={t(errors.core)} />}
          </td>
        </tr>
      )}
    </>
  );
}

function Message({ id, text }: { id: string; text: string }) {
  return (
    <p id={id} className="field-error">
      <ErrorIcon />
      {text}
    </p>
  );
}
