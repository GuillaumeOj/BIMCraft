import { useI18n } from "../i18n/I18nProvider";
import type { MessageKey } from "../i18n/i18n";
import { type Draft, effectiveCore, thicknessInMetres } from "./draft";

// A thin layer must stay visible: the strip is schematic, not to scale (UX).
const MIN_SIZE = 16;
const MIN_MEMBRANE = 3;
const STRIP = 360;

type Props = { draft: Draft; selected: number | null };

/**
 * The schematic section of the build-up: one block per layer, sized by its thickness, from one
 * face to the other. Decorative: the table is the accessible source.
 */
export function SectionPreview({ draft, selected }: Props) {
  const { t } = useI18n();
  const vertical = draft.usage === "slab";
  const total = draft.layers.reduce((sum, layer) => sum + thicknessInMetres(layer.thickness), 0);
  const size = (thickness: number, isMembrane: boolean) => {
    const floor = isMembrane && thickness === 0 ? MIN_MEMBRANE : MIN_SIZE;
    return Math.max(floor, total === 0 ? floor : (thickness / total) * STRIP);
  };
  const [start, end] = [
    `buildups.layers.face.${draft.usage}.start`,
    `buildups.layers.face.${draft.usage}.end`,
  ];

  return (
    <div className="section-preview" aria-hidden="true" data-usage={draft.usage}>
      <p className="preview-heading">
        <span>{t("buildups.preview.title")}</span>
        <span className="preview-note">{t("buildups.preview.note")}</span>
      </p>
      <div className="preview-body">
        <span className="preview-face">{t(start as MessageKey)}</span>
        <ol className="preview-strip">
          {draft.layers.map((layer, index) => {
            const thickness = thicknessInMetres(layer.thickness);
            const length = size(thickness, layer.function === "membrane");
            return (
              <li
                key={layer.key}
                className="preview-layer"
                data-function={layer.function}
                data-core={effectiveCore(layer)}
                data-selected={selected === index}
                style={vertical ? { height: length } : { width: length }}
              >
                <span className="preview-number">{index + 1}</span>
              </li>
            );
          })}
        </ol>
        <span className="preview-face">{t(end as MessageKey)}</span>
      </div>
      {draft.layers.some(effectiveCore) && (
        <p className="preview-core">{t("buildups.preview.core")}</p>
      )}
    </div>
  );
}
