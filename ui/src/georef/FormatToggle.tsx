import { useI18n } from "../i18n/I18nProvider";
import { SegmentedToggle } from "../ui/SegmentedToggle";
import type { GeographicFormat } from "./pointFormat";

/** The Decimal degrees / DMS choice, shared by the dialog and the Coordinates panel. */
export function FormatToggle({
  value,
  onChange,
}: {
  value: GeographicFormat;
  onChange: (format: GeographicFormat) => void;
}) {
  const { t } = useI18n();
  return (
    <SegmentedToggle
      label={t("georef.format.label")}
      value={value}
      options={[
        { value: "decimal", label: t("georef.format.decimal") },
        { value: "dms", label: t("georef.format.dms") },
      ]}
      onChange={onChange}
    />
  );
}
