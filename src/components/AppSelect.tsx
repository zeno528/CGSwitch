import { Check, ChevronDown } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";

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
  renderLabel?: (option: SelectOption<T>) => ReactNode;
}

export function AppSelect<T extends string | number>({
  value,
  options,
  onChange,
  placeholder,
  disabled,
  className = "",
  renderLabel,
}: AppSelectProps<T>) {
  const selected = options.find((option) => String(option.value) === String(value));
  const [open, setOpen] = useState(false);
  const [placement, setPlacement] = useState<"bottom" | "top">("bottom");
  const [menuStyle, setMenuStyle] = useState<CSSProperties>();
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [open]);

  useLayoutEffect(() => {
    if (!open) return;
    const updatePosition = () => {
      const root = rootRef.current;
      const menu = menuRef.current;
      if (!root || !menu) return;
      const rect = root.getBoundingClientRect();
      const gap = 6;
      const menuHeight = menu.scrollHeight;
      const below = window.innerHeight - rect.bottom - gap;
      const above = rect.top - gap;
      const nextPlacement = below < menuHeight && above > below ? "top" : "bottom";
      setPlacement(nextPlacement);
      setMenuStyle({
        left: `${rect.left}px`,
        width: `${rect.width}px`,
        ...(nextPlacement === "top" ? { top: "auto", bottom: `${window.innerHeight - rect.top + gap}px` } : { top: `${rect.bottom + gap}px`, bottom: "auto" }),
      });
    };
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [open, options.length]);

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
        <span className="app-select__label">{selected ? renderLabel?.(selected) ?? selected.label : placeholder ?? "请选择"}</span>
        <ChevronDown className="app-select__icon" size={16} strokeWidth={2} aria-hidden="true" />
      </button>
      <div ref={menuRef} className="app-select-menu" data-open={open} data-placement={placement} style={menuStyle} role="listbox" aria-label={placeholder ?? "选项"} aria-hidden={!open}>
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
          <span>{renderLabel?.(option) ?? option.label}</span>
          {selected?.value === option.value ? <Check className="app-select-option__check" size={16} strokeWidth={2.5} aria-hidden="true" /> : null}
        </button>)}
      </div>
    </div>
  );
}
