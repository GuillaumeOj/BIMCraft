type Option<V extends string> = { value: V; label: string };

type SegmentedToggleProps<V extends string> = {
  label: string;
  value: V;
  options: readonly Option<V>[];
  onChange: (value: V) => void;
};

/** Exclusive choices side by side (decimal degrees or DMS; all, walls or slabs). */
export function SegmentedToggle<V extends string>({
  label,
  value,
  options,
  onChange,
}: SegmentedToggleProps<V>) {
  return (
    <fieldset className="segmented">
      <legend className="visually-hidden">{label}</legend>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={option.value === value}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </fieldset>
  );
}
