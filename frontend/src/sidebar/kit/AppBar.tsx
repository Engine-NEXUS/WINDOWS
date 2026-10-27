import type { ReactNode } from "react";
import "./kit.css";

export interface AppBarProps {
  logo?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  status?: "scanning" | "done" | "error";
  actions?: ReactNode;
  className?: string;
}

export function AppBar({
  logo,
  title,
  subtitle,
  status,
  actions,
  className = "",
}: AppBarProps) {
  return (
    <header className={`kit-app-bar ${className}`} data-tauri-drag-region>
      <div className="kit-app-bar-brand" data-tauri-drag-region>
        {logo && (
          <div className="kit-app-bar-logo" data-tauri-drag-region>
            {logo}
          </div>
        )}
        <div className="kit-app-bar-titles" data-tauri-drag-region>
          <h1
            className="kit-app-bar-title"
            title={typeof title === "string" ? title : undefined}
            data-tauri-drag-region
          >
            {title}
          </h1>
          {subtitle && (
            <span
              className="kit-app-bar-subtitle"
              title={typeof subtitle === "string" ? subtitle : undefined}
              data-tauri-drag-region
            >
              {subtitle}
            </span>
          )}
        </div>
      </div>

      {actions && (
        <div className="kit-app-bar-actions">
          {actions}
          {status && (
            <span
              className={`kit-app-bar-status-dot kit-app-bar-status-dot--${status}`}
              title={`Status: ${status}`}
              aria-label={`Status: ${status}`}
            />
          )}
        </div>
      )}
    </header>
  );
}
