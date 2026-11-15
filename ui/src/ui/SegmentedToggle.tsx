import { useI18n } from "../i18n/I18nProvider";
import { shortcutProps } from "./shortcut";

type Option<V extends string> = { value: V; label: string };

type SegmentedToggleProps<V extends string> = {
  label: string;
  /** The key that switches between the options, named on each button. */
  shortcut?: string | undefined;
  value: V;
  options: readonly Option<V>[];
  onChange: (value: V) => void;
  /** Greys the choices out and ignores them (a disabled fieldset disables its buttons). */
  disabled?: boolean;
};

/** Exclusive choices side by side (decimal degrees or DMS; all, walls or slabs). */
export function SegmentedToggle<V extends string>({
  label,
  shortcut,
  value,
  options,
  onChange,
  disabled,
}: SegmentedToggleProps<V>) {
  const { t } = useI18n();
  return (
    <fieldset className="segmented" disabled={disabled}>
      <legend className="visually-hidden">{label}</legend>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={option.value === value}
          {...(shortcut === undefined ? {} : shortcutProps(t, option.label, shortcut))}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </fieldset>
  );
}
