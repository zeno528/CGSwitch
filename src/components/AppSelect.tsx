interface SelectOption<T extends string | number = string> {
  label: string;
  value: T;
}

interface AppSelectProps<T extends string | number> {
  value: T | null | undefined;
  options: SelectOption<T>[];
  onChange: (value: T) => void;
  placeholder?: string;
  disabled?: boolean;
  className?: string;
}

export function AppSelect<T extends string | number>({
  value,
  options,
  onChange,
  placeholder,
  disabled,
  className = "",
}: AppSelectProps<T>) {
  const selected = options.find((option) => String(option.value) === String(value));
  return (
    <select
      className={`app-select ${className}`}
      value={selected ? String(selected.value) : ""}
      onChange={(event) => {
        const option = options.find((candidate) => String(candidate.value) === event.target.value);
        if (option) onChange(option.value);
      }}
      disabled={disabled}
      aria-label={placeholder}
    >
      {placeholder && !selected ? <option value="">{placeholder}</option> : null}
      {options.map((option) => <option key={String(option.value)} value={String(option.value)}>{option.label}</option>)}
    </select>
  );
}
