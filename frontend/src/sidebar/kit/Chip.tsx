import type { ReactNode } from "react";
import "./kit.css";

export type ChipTone = "neutral" | "good" | "warn" | "bad";

export interface ChipProps {
  children: ReactNode;
  tone?: ChipTone;
  dot?: string;
  className?: string;
  title?: string;
}

export function Chip({
  children,
  tone = "neutral",
  dot,
  className = "",
  title,
}: ChipProps) {
  return (
    <span
      className={`kit-chip kit-chip--${tone} ${className}`}
      title={title}
    >
      {dot && <span className="kit-chip-dot" style={{ backgroundColor: dot }} />}
      <span className="kit-chip-label">{children}</span>
    </span>
  );
}

export interface DropdownChipOption {
  label: string;
  value: string;
}

export interface DropdownChipProps {
  value: string;
  options: Array<string | DropdownChipOption>;
  onChange: (value: string) => void;
  className?: string;
  ariaLabel?: string;
}

export function DropdownChip({
  value,
  options,
  onChange,
  className = "",
  ariaLabel,
}: DropdownChipProps) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className={`kit-dropdown-chip ${className}`}
      aria-label={ariaLabel}
    >
      {options.map((opt) => {
        const val = typeof opt === "string" ? opt : opt.value;
        const lbl = typeof opt === "string" ? opt : opt.label;
        return (
          <option key={val} value={val}>
            {lbl}
          </option>
        );
      })}
    </select>
  );
}
