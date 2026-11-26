import type { ReactNode } from "react";
import "./system.css";

/**
 * The shared pieces of every sidebar view (design: docs/features/197-sidebar-system-redesign-plan.md).
 * A view is `<SvView>` (the `.sv` token scope and the canvas) > `<SvBar>` (no dock or close buttons: the arrow keys dock the
 * window and Esc closes it) > `<SvScroll>` (the scrolling column of cards) > an optional `<ActionBar>` pinned at the bottom.
 */

/* ── icons ── */
const PATHS = {
  monitor: "M3 6a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM8 21h8M12 17v4",
  pr: "M6 3a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM6 16a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM18 16a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM6 8v8M18 16V9a3 3 0 0 0-3-3h-3m0 0 2.5-2.5M12 6l2.5 2.5",
  repo: "M4 5a2 2 0 0 1 2-2h12v15H6a2 2 0 0 0-2 2zM4 20a2 2 0 0 0 2 2h12",
  layers: "m12 3 9 5-9 5-9-5zM3 13l9 5 9-5",
  globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18",
  chat: "M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12z",
  shield: "M12 3 5 6v6c0 4.5 3 7.5 7 9 4-1.5 7-4.5 7-9V6zM9 12l2 2 4-4",
  check: "m5 12 4.5 4.5L19 7",
  x: "M6 6l12 12M18 6 6 18",
  copy: "M11 9h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-7a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2zM5 15V6a2 2 0 0 1 2-2h9",
  sparkle: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z",
  info: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11v5M12 8h.01",
  alert: "M12 4 3 20h18zM12 10v4M12 17h.01",
  file: "M7 3h7l5 5v13H7zM14 3v5h5",
  link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
  mic: "M12 3a3 3 0 0 0-3 3v5a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM5 11a7 7 0 0 0 14 0M12 18v3",
  arrow: "M5 12h14M13 6l6 6-6 6",
} as const;
export type SvIconName = keyof typeof PATHS;

export function SvIcon({ name }: { name: SvIconName }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
      <path d={PATHS[name]} />
    </svg>
  );
}

/* ── shell ── */
export function SvView({ children, className = "", label }: { children: ReactNode; className?: string; label?: string }) {
  return (
    <section className={`sv sv-view ${className}`.trim()} aria-label={label}>
      {children}
    </section>
  );
}

/** The header. There are deliberately no dock arrows and no close button: the arrow keys move the window, Esc closes it. */
export function SvBar({ icon, title, subtitle }: { icon: SvIconName; title: ReactNode; subtitle?: ReactNode }) {
  return (
    <header className="sv-bar" data-tauri-drag-region>
      <div className="sv-bar-logo" data-tauri-drag-region>
        <SvIcon name={icon} />
      </div>
      <div className="sv-bar-titles" data-tauri-drag-region>
        <h1 className="sv-bar-title" data-tauri-drag-region>{title}</h1>
        {subtitle ? <span className="sv-bar-sub" data-tauri-drag-region>{subtitle}</span> : null}
      </div>
    </header>
  );
}

export function SvScroll({ children }: { children: ReactNode }) {
  return (
    <div className="sv-scroll">
      <div className="sv-col">{children}</div>
    </div>
  );
}

export function SvCard({ title, right, children, className = "" }: { title?: ReactNode; right?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`sv-card ${className}`.trim()}>
      {title || right ? (
        <div className="sv-card-hd">
          <span className="sv-cap">{title}</span>
          {right}
        </div>
      ) : null}
      {children}
    </section>
  );
}

/* ── numbers ── */
export function KpiGrid({ children }: { children: ReactNode }) {
  return <div className="sv-kpis">{children}</div>;
}
export function KpiTile({ label, value, sub }: { label: ReactNode; value: ReactNode; sub?: ReactNode }) {
  return (
    <div className="sv-kpi">
      <span className="sv-cap">{label}</span>
      <div className="sv-kpi-v">{value}</div>
      {sub ? <span className="sv-kpi-s">{sub}</span> : null}
    </div>
  );
}

export type BarTone = "ink" | "mid" | "low";
/** A bar with its value on the right. `percent` is clamped to 0..100. */
export function BarRow({ label, value, percent, tone = "ink" }: { label: ReactNode; value: ReactNode; percent: number; tone?: BarTone }) {
  const p = clampPercent(percent);
  return (
    <div className="sv-bar-row">
      <span>{label}</span>
      <span className="sv-bar-val">{value}</span>
      <div className="sv-track" role="img" aria-label={`${p} percent`}>
        <div className={`sv-fill${tone === "ink" ? "" : ` sv-fill--${tone}`}`} style={{ width: `${p}%` }} />
      </div>
    </div>
  );
}
export function clampPercent(n: number): number {
  return Number.isFinite(n) ? Math.min(100, Math.max(0, Math.round(n))) : 0;
}

/* ── pills, chips, filters ── */
export type PillTone = "default" | "solid" | "outline" | "dash";
export function Pill({ tone = "default", icon, children }: { tone?: PillTone; icon?: SvIconName; children: ReactNode }) {
  return (
    <span className={`sv-pill${tone === "default" ? "" : ` sv-pill--${tone}`}`}>
      {icon ? <SvIcon name={icon} /> : null}
      {children}
    </span>
  );
}
export function SvChip({ icon, children }: { icon?: SvIconName; children: ReactNode }) {
  return (
    <span className="sv-chip">
      {icon ? <SvIcon name={icon} /> : null}
      {children}
    </span>
  );
}

/** Severity without colour: High = solid + filled triangle, Medium = outline + diamond, Low = dashed + circle, each with its word. */
export interface SeverityLook {
  tone: PillTone;
  symbol: string;
  word: string;
  rank: number;
}
export function severityOf(raw: unknown): SeverityLook {
  const s = String(raw ?? "").trim().toLowerCase();
  if (s === "high" || s === "critical" || s === "error" || s === "major") return { tone: "solid", symbol: "▲", word: "High", rank: 0 };
  if (s === "medium" || s === "moderate" || s === "minor" || s === "warning" || s === "warn") return { tone: "outline", symbol: "◆", word: "Medium", rank: 1 };
  return { tone: "dash", symbol: "●", word: "Low", rank: 2 };
}
export function SeverityPill({ severity }: { severity: unknown }) {
  const v = severityOf(severity);
  return (
    <Pill tone={v.tone}>
      <span aria-hidden="true">{v.symbol}</span>
      {v.word}
    </Pill>
  );
}

export function SegPill({ items, active, onChange, label }: { items: string[]; active: number; onChange: (i: number) => void; label?: string }) {
  return (
    <div className="sv-seg" role="group" aria-label={label}>
      {items.map((t, i) => (
        <button key={`${t}-${i}`} type="button" aria-pressed={i === active} onClick={() => onChange(i)}>
          {t}
        </button>
      ))}
    </div>
  );
}

/* ── lists ── */
export interface NumberedItem {
  key?: string | number;
  title: ReactNode;
  text?: ReactNode;
}
export function NumberedList({ items }: { items: NumberedItem[] }) {
  return (
    <ol className="sv-list">
      {items.map((it, i) => (
        <li className="sv-li" key={it.key ?? i}>
          <span className="sv-num" aria-hidden="true">{i + 1}</span>
          <div className="sv-grow">
            <b className="sv-li-t">{it.title}</b>
            {it.text ? <p className="sv-p">{it.text}</p> : null}
          </div>
        </li>
      ))}
    </ol>
  );
}

export type CalloutKind = "note" | "tip" | "important" | "warning" | "caution";
const CALLOUT_WORD: Record<CalloutKind, string> = { note: "Note", tip: "Tip", important: "Important", warning: "Warning", caution: "Caution" };
export function calloutKindOf(raw: unknown): CalloutKind {
  const s = String(raw ?? "").trim().toLowerCase();
  return (["note", "tip", "important", "warning", "caution"] as const).find((k) => k === s) ?? "note";
}
export function Callout({ kind = "note", children }: { kind?: CalloutKind; children: ReactNode }) {
  const strong = kind === "important" || kind === "warning" || kind === "caution";
  return (
    <div className={`sv-callout${strong ? "" : " sv-callout--dim"}`} role="note">
      <SvIcon name={strong ? "alert" : "info"} />
      <div className="sv-grow">
        <span className="sv-callout-w">{CALLOUT_WORD[kind]}</span>
        <div className="sv-p">{children}</div>
      </div>
    </div>
  );
}

/* ── actions ── */
/** `sticky` pins the bar to the bottom of the scrolling column it sits in (used inside the assistant view's scroll area). */
export function ActionBar({ children, sticky }: { children: ReactNode; sticky?: boolean }) {
  return <footer className={`sv-actions${sticky ? " sv-actions--sticky" : ""}`}>{children}</footer>;
}
export function SvButton({
  children,
  onClick,
  primary,
  disabled,
  keyHint,
}: {
  children: ReactNode;
  onClick?: () => void;
  primary?: boolean;
  disabled?: boolean;
  keyHint?: string;
}) {
  return (
    <button type="button" className={`sv-btn${primary ? " sv-btn--primary" : ""}`} onClick={onClick} disabled={disabled}>
      {children}
      {keyHint ? <kbd className="sv-kbd">{keyHint}</kbd> : null}
    </button>
  );
}

/** A thin bar that drains over `seconds`; the voice answer window made visible. */
export function VoiceTimer({ seconds, runKey }: { seconds: number; runKey?: string | number }) {
  return (
    <div className="sv-timer" aria-hidden="true">
      <i key={runKey} style={{ animationDuration: `${Math.max(1, seconds)}s` }} />
    </div>
  );
}
