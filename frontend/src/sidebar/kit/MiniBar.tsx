import "./kit.css";

export interface MiniBarProps {
  label: string;
  pass: boolean;
  detail?: string;
  className?: string;
}

export function MiniBar({
  label,
  pass,
  detail,
  className = "",
}: MiniBarProps) {
  return (
    <div
      className={`kit-minibar ${pass ? "kit-minibar--pass" : "kit-minibar--fail"} ${className}`}
      data-pass={pass}
    >
      <div className="kit-minibar-top">
        <span className="kit-minibar-label" title={label}>
          {label}
        </span>
        <span
          className={`kit-minibar-glyph ${pass ? "kit-minibar-glyph--pass" : "kit-minibar-glyph--fail"}`}
          aria-label={pass ? "Passed" : "Missing"}
        >
          {pass ? "✓" : "✗"}
        </span>
      </div>

      <div className="kit-minibar-track">
        <div
          className={`kit-minibar-fill ${pass ? "kit-minibar-fill--pass" : "kit-minibar-fill--fail"}`}
          style={{ width: pass ? "100%" : "0%" }}
        />
      </div>

      {detail && (
        <span className="kit-minibar-detail" title={detail}>
          {detail}
        </span>
      )}
    </div>
  );
}
