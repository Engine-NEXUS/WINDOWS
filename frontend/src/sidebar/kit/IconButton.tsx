import type { ReactNode } from "react";
import "./kit.css";

export interface IconButtonProps {
  label: string;
  onClick?: () => void;
  children: ReactNode;
  active?: boolean;
  className?: string;
  disabled?: boolean;
}

export function IconButton({
  label,
  onClick,
  children,
  active = false,
  className = "",
  disabled = false,
}: IconButtonProps) {
  return (
    <button
      type="button"
      className={`kit-icon-btn ${active ? "kit-icon-btn--active" : ""} ${className}`}
      onClick={onClick}
      disabled={disabled}
      title={label}
      aria-label={label}
    >
      {children}
    </button>
  );
}
