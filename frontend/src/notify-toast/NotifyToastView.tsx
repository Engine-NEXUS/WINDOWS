import { forwardRef } from "react";
import { moreLabel, statusLabel, type ToastPayload } from "./notifyToastModel";
import "./notify-toast.css";

interface Props {
  payload: ToastPayload;
  fading?: boolean;
  onOpen: () => void;
  onClose: () => void;
  onPause: (paused: boolean) => void;
}

/** The card itself (no Tauri, no timers): source + status, the headline, up to four detail rows, one action.
 *  Compact on purpose: 320 px wide, as tall as its content. */
export const NotifyToastView = forwardRef<HTMLDivElement, Props>(function NotifyToastView({ payload: p, fading = false, onOpen, onClose, onPause }, ref) {
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
        <span className="nt-dot" aria-hidden="true" />
        <span className="nt-source">{p.provider}</span>
        <span className={`nt-status nt-status--${p.status}`}>{statusLabel(p.status)}</span>
        {more && <span className="nt-more">{more}</span>}
        <button type="button" className="nt-close" aria-label="Dismiss" onClick={onClose}>
          ×
        </button>
      </div>
      <button type="button" className="nt-body" onClick={onOpen}>
        <span className="nt-headline">{p.headline}</span>
        {p.rows.length > 0 && (
          <span className="nt-rows">
            {p.rows.map((r) => (
              <span className="nt-row" key={r.label}>
                <span className="nt-label">{r.label}</span>
                <span className="nt-value">{r.value}</span>
              </span>
            ))}
          </span>
        )}
      </button>
      <div className="nt-foot">
        <button type="button" className="nt-action" onClick={onOpen}>
          {p.action}
        </button>
      </div>
    </div>
  );
});
