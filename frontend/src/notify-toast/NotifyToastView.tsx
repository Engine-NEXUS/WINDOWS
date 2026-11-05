import { forwardRef } from "react";
import { moreLabel, statusLabel, type ToastPayload } from "./notifyToastModel";
import { ProviderLogo } from "./ProviderLogo";
import "./notify-toast.css";

interface Props {
  payload: ToastPayload;
  fading?: boolean;
  onOpen: () => void;
  onClose: () => void;
  onPause: (paused: boolean) => void;
}

function StatusGlyph({ status }: { status: ToastPayload["status"] }) {
  if (status === "failed") {
    return (
      <svg className="nt-glyph" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <circle cx="8" cy="8" r="7" stroke="currentColor" strokeWidth="1.5" fill="none" />
        <path d="M5.5 5.5l5 5M10.5 5.5l-5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      </svg>
    );
  }
  if (status === "recovered") {
    return (
      <svg className="nt-glyph" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <circle cx="8" cy="8" r="7" stroke="currentColor" strokeWidth="1.5" fill="none" />
        <path
          d="M4.5 8.2l2.3 2.3 4.7-4.7"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          fill="none"
        />
      </svg>
    );
  }
  return (
    <svg className="nt-glyph" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      <circle cx="8" cy="8" r="7" stroke="currentColor" strokeWidth="1.5" fill="none" />
      <circle cx="8" cy="5" r="1" fill="currentColor" />
      <path d="M8 7.5v4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

/** The alert toast: compact 320 px card with app logo, source + status, status glyph beside headline, detail rows, text actions. */
export const NotifyToastView = forwardRef<HTMLDivElement, Props>(function NotifyToastView(
  { payload: p, fading = false, onOpen, onClose, onPause },
  ref
) {
  const more = moreLabel(p.count);
  return (
    <div
      ref={ref}
      className={`nt-card nt-card--${p.status}${fading ? " nt-card--out" : ""}`}
      role="status"
      aria-live="polite"
      aria-label={`${p.provider}: ${p.headline}`}
      onMouseEnter={() => onPause(true)}
      onMouseLeave={() => onPause(false)}
      onFocus={() => onPause(true)}
      onBlur={() => onPause(false)}
    >
      <div className="nt-head">
        <ProviderLogo icon={p.icon} provider={p.provider} />
        <span className="nt-source">{p.provider}</span>
        <span className={`nt-status nt-status--${p.status}`}>{statusLabel(p.status)}</span>
        {more && <span className="nt-more">{more}</span>}
        <button type="button" className="nt-close" aria-label="Dismiss" onClick={onClose}>
          ×
        </button>
      </div>
      <button type="button" className="nt-body" onClick={onOpen}>
        <div className="nt-title-row">
          <StatusGlyph status={p.status} />
          <span className="nt-headline">{p.headline}</span>
        </div>
        {p.rows.length > 0 && (
          <div className="nt-rows">
            {p.rows.map((r, i) => (
              <div className="nt-row" key={`${r.label}-${i}`}>
                <span className="nt-label">{r.label}</span>
                <span className="nt-value">{r.value}</span>
              </div>
            ))}
          </div>
        )}
      </button>
      <div className="nt-foot">
        <button type="button" className="nt-btn nt-btn--primary" onClick={onOpen}>
          {p.action}
        </button>
        <button type="button" className="nt-btn nt-btn--secondary" onClick={onClose}>
          Dismiss
        </button>
      </div>
    </div>
  );
});
