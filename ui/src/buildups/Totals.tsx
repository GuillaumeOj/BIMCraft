import type { BuildUpPreview } from "../api";
import { formatNumber, formatPlain } from "../georef/units";
import { useI18n } from "../i18n/I18nProvider";
import { pluralKey } from "./messages";

/**
 * The totals the core computed for the draft (`buildups.preview`): the UI shows them and does
 * no material maths. Nothing is shown as a number until the core has answered.
 */
export function Totals({ preview }: { preview: BuildUpPreview | null }) {
  const { t, language } = useI18n();
  const resistance = preview?.thermal_resistance;
  const partial = preview?.layers_without_thermal_data ?? 0;
  return (
    <div className="totals">
      <p>
        {t("buildups.total.thickness", {
          total: preview === null ? "—" : formatPlain(preview.total_thickness * 1000, 1, language),
        })}
      </p>
      <p>
        {resistance === undefined || resistance === null
          ? t("buildups.total.resistanceNone")
          : t("buildups.total.resistance", { value: formatNumber(resistance, 2, language) })}
      </p>
      {partial > 0 && (
        <p className="totals-note">
          {t(pluralKey(language, partial, "buildups.total.partial"), { count: partial })}
        </p>
      )}
    </div>
  );
}
