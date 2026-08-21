import { CaretDown } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";

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
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [open]);

  const selectOption = (option: SelectOption<T>) => {
    onChange(option.value);
    setOpen(false);
  };

  return (
    <div ref={rootRef} className="app-select-wrap" data-open={open}>
      <button
        type="button"
        className={`app-select ${className}`}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={placeholder}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key === "Escape") setOpen(false);
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <span className="app-select__label">{selected?.label ?? placeholder ?? "请选择"}</span>
        <CaretDown className="app-select__icon" size={16} weight="bold" aria-hidden="true" />
      </button>
      <div className="app-select-menu" data-open={open} role="listbox" aria-label={placeholder ?? "选项"} aria-hidden={!open}>
        {options.map((option) => <button
          key={String(option.value)}
          type="button"
          role="option"
          tabIndex={open ? 0 : -1}
          aria-selected={selected?.value === option.value}
          className="app-select-option"
          data-selected={selected?.value === option.value}
          onClick={() => selectOption(option)}
        >
          <span>{option.label}</span>
          {selected?.value === option.value ? <span className="app-select-option__check" aria-hidden="true">✓</span> : null}
        </button>)}
      </div>
    </div>
  );
}
