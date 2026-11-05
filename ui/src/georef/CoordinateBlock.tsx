import { useId } from "react";
import { useI18n } from "../i18n/I18nProvider";
import { SegmentedToggle } from "../ui/SegmentedToggle";
import { TextField } from "../ui/TextField";
import {
  BLOCK_TITLE,
  FIELD_LABELS,
  type GeographicFormat,
  type System,
  type Texts,
} from "./pointFormat";

const METRE = "m";
const DEGREE = "°";

type CoordinateBlockProps = {
  system: System;
  /** The second line: the CRS or datum name, or the unit of the local axes. */
  caption: string;
  texts: Texts;
  invalid: readonly [boolean, boolean, boolean];
  /** True while the user is typing in this block: the others are computed from it. */
  isSource: boolean;
  /** Title of the source block, when another block is the source. */
  computedFrom: string | null;
  /** A message replacing the "computed from" line. */
  error: string | null;
  format: GeographicFormat;
  onFormat: (format: GeographicFormat) => void;
  canCopy: boolean;
  onEdit: (index: number, text: string) => void;
  onEnter: () => void;
  onCopy: () => void;
};

/** One of the three blocks of the Coordinates panel: a point in one coordinate system. */
export function CoordinateBlock({
  system,
  caption,
  texts,
  invalid,
  isSource,
  computedFrom,
  error,
  format,
  onFormat,
  canCopy,
  onEdit,
  onEnter,
  onCopy,
}: CoordinateBlockProps) {
  const { t } = useI18n();
  const prefix = useId();
  const title = t(BLOCK_TITLE[system]);
  const geographic = system === "geographic";
  const labels = FIELD_LABELS[system];

  return (
    <section
      className="coordinate-block"
      data-source={isSource}
      aria-labelledby={`${prefix}-title`}
    >
      <div className="block-header">
        <h3 id={`${prefix}-title`}>{title}</h3>
        {isSource && <span className="badge">{t("georef.block.edited")}</span>}
        <button
          type="button"
          className="button button-small"
          aria-label={t("georef.copyBlock", { block: title })}
          disabled={!canCopy}
          onClick={onCopy}
        >
          {t("georef.copy")}
        </button>
      </div>
      <p className="block-caption">{caption}</p>
      {geographic && (
        <SegmentedToggle
          label={t("georef.format.label")}
          value={format}
          options={[
            { value: "decimal", label: t("georef.format.decimal") },
            { value: "dms", label: t("georef.format.dms") },
          ]}
          onChange={onFormat}
        />
      )}
      {texts.map((text, index) => {
        const label = labels[index] ?? "georef.axis.x";
        const isAngle = geographic && index < 2;
        const unit = isAngle ? (format === "decimal" ? DEGREE : undefined) : METRE;
        return (
          <TextField
            // The three fields of a block never reorder.
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed positions
            key={index}
            id={`${prefix}-${index}`}
            variant="compact"
            label={t(label)}
            value={text}
            unit={unit}
            error={invalid[index] ? t("georef.error.number") : undefined}
            onChange={(value) => onEdit(index, value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                onEnter();
              }
            }}
          />
        );
      })}
      {geographic && <p className="block-caption">{t("georef.block.heightApprox")}</p>}
      {error !== null ? (
        <p className="block-caption block-error">{error}</p>
      ) : (
        computedFrom !== null && (
          <p className="block-caption">{t("georef.block.computed", { source: computedFrom })}</p>
        )
      )}
    </section>
  );
}
