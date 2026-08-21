import { useState, type KeyboardEvent } from "react";

interface AppSwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  size?: "sm" | "md";
  label?: string;
}

export function AppSwitch({ checked, onCheckedChange, disabled, size = "md", label }: AppSwitchProps) {
  const [pressed, setPressed] = useState(false);
  const toggle = () => onCheckedChange(!checked);
  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === " ") {
      event.preventDefault();
      setPressed(true);
    }
  };
  const handleKeyUp = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === " ") {
      setPressed(false);
      toggle();
    }
  };

  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={`app-switch app-switch--${size}`}
      data-state={checked ? "checked" : "unchecked"}
      data-pressed={pressed ? "true" : undefined}
      onClick={toggle}
      onKeyDown={handleKeyDown}
      onKeyUp={handleKeyUp}
      onBlur={() => setPressed(false)}
    >
      <span className="app-switch-rail" aria-hidden="true"><span className="app-switch-thumb" /></span>
    </button>
  );
}
