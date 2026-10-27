import { type ReactNode } from "react";

function cn(...classes: (string | undefined | null | false)[]): string {
  return classes.filter(Boolean).join(" ");
}

export interface OptionCardProps {
  icon?: ReactNode;
  title: string;
  desc?: string;
  badge?: string;
  selected: boolean;
  onClick: () => void;
  disabled?: boolean;
  className?: string;
}

export function OptionCard({
  icon,
  title,
  desc,
  badge,
  selected,
  onClick,
  disabled = false,
  className,
}: OptionCardProps) {
  return (
    <div
      className={cn(
        "nx-option-card",
        selected && "is-selected",
        disabled && "is-disabled",
        className
      )}
      onClick={() => {
        if (!disabled) onClick();
      }}
      role="radio"
      aria-checked={selected}
      tabIndex={disabled ? -1 : 0}
      onKeyDown={(e) => {
        if (!disabled && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          onClick();
        }
      }}
    >
      {icon && <div className="nx-option-icon">{icon}</div>}
      <div className="nx-option-meta">
        <div className="nx-option-header">
          <span className="nx-option-title">{title}</span>
          {badge && <span className="nx-option-badge">{badge}</span>}
        </div>
        {desc && <div className="nx-option-desc">{desc}</div>}
      </div>
      <div className="nx-option-check" aria-hidden="true">
        {selected ? <span className="nx-option-check-glyph">✓</span> : null}
      </div>
    </div>
  );
}

export interface SwitchCardProps {
  icon?: ReactNode;
  title: string;
  desc?: string;
  badge?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
  extraAction?: ReactNode;
}

export function SwitchCard({
  icon,
  title,
  desc,
  badge,
  checked,
  onChange,
  disabled = false,
  className,
  extraAction,
}: SwitchCardProps) {
  return (
    <div
      className={cn(
        "nx-option-card nx-switch-card",
        checked && "is-active",
        disabled && "is-disabled",
        className
      )}
      onClick={() => {
        if (!disabled) onChange(!checked);
      }}
      role="switch"
      aria-checked={checked}
      tabIndex={disabled ? -1 : 0}
      onKeyDown={(e) => {
        if (!disabled && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          onChange(!checked);
        }
      }}
    >
      {icon && <div className="nx-option-icon">{icon}</div>}
      <div className="nx-option-meta">
        <div className="nx-option-header">
          <span className="nx-option-title">{title}</span>
          {badge && <span className="nx-option-badge">{badge}</span>}
        </div>
        {desc && <div className="nx-option-desc">{desc}</div>}
        {extraAction && (
          <div
            className="nx-option-extra"
            onClick={(e) => e.stopPropagation()}
          >
            {extraAction}
          </div>
        )}
      </div>
      <div
        className={cn("nx-option-switch", checked && "is-on")}
        aria-hidden="true"
      >
        <div className="nx-option-switch-thumb" />
      </div>
    </div>
  );
}
