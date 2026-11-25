import { useCallback, useEffect, useRef, useState } from "react";
import "./reminder.css";
import { reminderStackSlot, type Rect } from "./layoutSlots";
import {
  countdownText,
  digitsFor,
  kindLabel,
  MAX_CHIPS,
  parseFired,
  parseReminder,
  parseReminders,
  ringFraction,
  rollRange,
  STRIP_LENGTH,
  TIMING,
  untilWords,
  visibleChips,
  wantsChip,
  type FiredNotice,
  type ReminderEvent,
} from "./reminderModel";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

const RING = 56.55; // circumference of a 9 px radius circle

// ── presentational: the chips (tested without Tauri) ────────────────────────

export interface ChipItem {
  event: ReminderEvent;
  /** Set while the alert for this reminder is on screen. */
  firing?: FiredNotice;
}

export function ReminderChipList({ items, extra, now, startedAt }: { items: ChipItem[]; extra: number; now: number; startedAt: Record<string, number> }) {
  return (
    <>
      {items.map(({ event, firing }) => {
        const left = event.atMs - now;
        const frac = ringFraction(startedAt[event.id] ?? now, event.atMs, now);
        return (
          <div key={event.id} className={firing ? "nx-rem-chip nx-rem-firing" : "nx-rem-chip"} data-id={event.id}>
            <svg width="22" height="22" viewBox="0 0 22 22" aria-hidden="true">
              <circle className="nx-rem-ring-bg" cx="11" cy="11" r="9" fill="none" strokeWidth="2.5" />
              <circle
                className="nx-rem-ring-fg"
                cx="11"
                cy="11"
                r="9"
                fill="none"
                strokeWidth="2.5"
                strokeLinecap="round"
                transform="rotate(-90 11 11)"
                strokeDasharray={RING}
                strokeDashoffset={firing ? 0 : RING * frac}
              />
            </svg>
            <span className="nx-rem-chip-text">{firing ? firing.alert : event.title}</span>
            <span className="nx-rem-chip-time">{firing ? "now" : countdownText(left)}</span>
          </div>
        );
      })}
      {extra > 0 && <div className="nx-rem-chip nx-rem-more">+{extra} more</div>}
    </>
  );
}

// ── the stage component ─────────────────────────────────────────────────────

type Phase = "idle" | "in" | "rolling" | "settled" | "vanish";

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

function reducedMotion(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * The universal reminder animation. Rust (`reminder.rs`) owns the list and sends three events; this only draws them:
 *   reminder:roll   a reminder was just set: roll the clock from now to its time, then let it vanish in place
 *   reminder:update the whole list: one countdown chip each, soonest first, beside the orb capsule
 *   reminder:fire   the time has come: the chip turns into the alert for a few seconds
 * The spoken line is not drawn here: NEXUS says it through the normal speech path and the response caption shows it.
 * Never interactive, so it needs no hitbox. Placement and animation are applied through the CSSOM (the stage CSP drops
 * React inline styles).
 */
export function ReminderRoll() {
  const [list, setList] = useState<ReminderEvent[]>([]);
  const [countdown, setCountdown] = useState(true);
  const [firing, setFiring] = useState<Record<string, FiredNotice>>({});
  const [now, setNow] = useState(Date.now());
  const [orb, setOrb] = useState<Rect | null>(null);
  const [roll, setRoll] = useState<{ key: number; event: ReminderEvent } | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [label, setLabel] = useState("Now");
  const [sub, setSub] = useState("");
  const [ap, setAp] = useState("");
  const [hourBlank, setHourBlank] = useState(false);

  const startedAt = useRef<Record<string, number>>({});
  const stripRefs = useRef<(HTMLSpanElement | null)[]>([null, null, null, null]);
  const idx = useRef<number[]>([0, 0, 0, 0]);
  const chipsRef = useRef<HTMLDivElement | null>(null);
  const token = useRef(0);

  // events from Rust, the user's setting, the orb's position
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    const offs: Array<() => void> = [];
    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");
      const toCss = (r: Rect): Rect => {
        const d = window.devicePixelRatio || 1;
        return { x: r.x / d, y: r.y / d, w: r.w / d, h: r.h / d };
      };
      offs.push(
        await listen<unknown>("reminder:update", (e) => {
          if (!live) return;
          const next = parseReminders(e.payload);
          for (const r of next) if (!(r.id in startedAt.current)) startedAt.current[r.id] = Date.now();
          setList(next);
        }),
        await listen<unknown>("reminder:roll", (e) => {
          const ev = parseReminder(e.payload);
          if (live && ev) setRoll((prev) => ({ key: (prev?.key ?? 0) + 1, event: ev }));
        }),
        await listen<unknown>("reminder:fire", (e) => {
          const n = parseFired(e.payload);
          if (!live || !n) return;
          setFiring((f) => ({ ...f, [n.id]: n }));
          setTimeout(() => live && setFiring((f) => {
            const { [n.id]: _gone, ...rest } = f;
            return rest;
          }), TIMING.alertShown);
        }),
        await listen<{ reminderCountdown?: boolean }>("settings:updated", (e) => {
          if (live && typeof e.payload?.reminderCountdown === "boolean") setCountdown(e.payload.reminderCountdown);
        }),
        await listen<Rect>("stage:orb_rect", (e) => live && e.payload && setOrb(toCss(e.payload))),
      );
      invoke<Rect | null>("get_pending_orb_rect").then((r) => live && r && setOrb(toCss(r))).catch(() => {});
      // a reminder that was set before this stage mounted
      invoke("reminder_list").then((l) => {
        if (!live) return;
        const next = parseReminders(l);
        for (const r of next) if (!(r.id in startedAt.current)) startedAt.current[r.id] = Date.now();
        setList(next);
      }).catch(() => {});
      invoke<{ reminderCountdown?: boolean }>("get_settings").then((s) => {
        if (live && typeof s?.reminderCountdown === "boolean") setCountdown(s.reminderCountdown);
      }).catch(() => {});
    })();
    return () => {
      live = false;
      offs.forEach((f) => f());
    };
  }, []);

  // the countdown ticks once a second while anything is waiting
  useEffect(() => {
    if (list.length === 0 && Object.keys(firing).length === 0) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [list.length, Object.keys(firing).length]);

  // where the chips hang (CSSOM: the stage CSP drops React inline styles)
  useEffect(() => {
    const el = chipsRef.current;
    if (!el) return;
    const slot = reminderStackSlot(orb, { w: window.innerWidth, h: window.innerHeight });
    el.style.left = `${slot.left}px`;
    el.style.top = `${slot.top}px`;
    el.style.maxWidth = `${slot.maxWidth}px`;
  }, [orb, list.length, firing]);

  const setStrip = useCallback((k: number, i: number, instant: boolean, delay: number) => {
    const el = stripRefs.current[k];
    if (!el) return;
    idx.current[k] = i;
    if (instant) el.classList.add("nx-rem-nt");
    el.style.setProperty("--d", String(delay));
    el.style.setProperty("--i", String(i));
    if (instant) {
      void el.offsetWidth;
      el.classList.remove("nx-rem-nt");
    }
  }, []);

  // the roll: now -> reminder time, hold, vanish in place
  useEffect(() => {
    if (!roll) return;
    const my = ++token.current;
    const speed = reducedMotion() ? 0.01 : 1;
    const t = (ms: number) => sleep(ms * speed);
    const alive = () => token.current === my;
    void (async () => {
      const cur = digitsFor(Date.now());
      const want = digitsFor(roll.event.atMs);
      [cur.h1, cur.h2, cur.m1, cur.m2].forEach((d, k) => setStrip(k, d, true, 0));
      setHourBlank(cur.h1 === 0);
      setAp(cur.ap);
      setLabel("Now");
      setSub("");
      setPhase("in");
      await t(TIMING.titleIn + TIMING.toLabel);
      if (!alive()) return;
      setLabel("Reminder at");
      setPhase("rolling");
      [want.h1, want.h2, want.m1, want.m2].forEach((d, k) => {
        const r = rollRange(k, idx.current[k], d);
        setStrip(k, r.from, true, 0);
        setStrip(k, r.to, false, (TIMING.stagger / 1000) * k * speed);
      });
      setHourBlank(want.h1 === 0);
      setTimeout(() => alive() && setAp(want.ap), 700 * speed);
      await t(TIMING.rollMs + TIMING.stagger * 3 + TIMING.settle);
      if (!alive()) return;
      setPhase("settled");
      setSub(untilWords(roll.event.atMs - Date.now()));
      await t(TIMING.holdAfter);
      if (!alive()) return;
      setPhase("vanish");
      await t(TIMING.vanish);
      if (!alive()) return;
      setPhase("idle");
    })();
    return () => {
      token.current++;
    };
  }, [roll, setStrip]);

  // chips: waiting reminders the user wants on screen, plus any alert that is showing
  const waiting = list.filter((e) => wantsChip(e, countdown, false));
  const alerts = Object.values(firing)
    .filter((n) => !list.some((e) => e.id === n.id))
    .map<ChipItem>((n) => ({ event: { id: n.id, kind: "reminder", title: n.title, atMs: now, spoken: "", alert: n.alert, source: "" }, firing: n }));
  const { shown, extra } = visibleChips(waiting);
  const items: ChipItem[] = [
    ...alerts,
    ...shown.map<ChipItem>((e) => ({ event: e, firing: firing[e.id] })),
  ].slice(0, MAX_CHIPS + alerts.length);

  const targetDigits = roll ? digitsFor(roll.event.atMs) : null;

  const renderCol = (k: number) => (
    <span key={k} className={k === 0 && hourBlank ? "nx-rem-col nx-rem-blank" : "nx-rem-col"}>
      <span
        className="nx-rem-strip"
        ref={(el) => {
          stripRefs.current[k] = el;
        }}
      >
        {Array.from({ length: STRIP_LENGTH }, (_, i) => (
          <span key={i}>{k === 0 && i % 10 === 0 ? " " : i % 10}</span>
        ))}
      </span>
    </span>
  );

  return (
    <>
      <div className="nx-rem-wrap" aria-hidden={phase === "idle"}>
        <div className={`nx-rem-panel nx-rem-${phase}`} role="status" aria-live="polite">
          <div className="nx-rem-label">{label}</div>
          <div className="nx-rem-clock" aria-label={phase === "idle" || !targetDigits ? "" : `Reminder at ${targetDigits.text}`}>
            {[0, 1].map(renderCol)}
            <span className="nx-rem-colon">:</span>
            {[2, 3].map(renderCol)}
            <span className="nx-rem-ampm">{ap}</span>
            <span className={phase === "settled" || phase === "vanish" ? "nx-rem-rule nx-rem-rule-on" : "nx-rem-rule"} />
          </div>
          <div className="nx-rem-title">
            <b>{roll ? kindLabel(roll.event.kind) : ""}</b>
            <span>{roll?.event.title ?? ""}</span>
          </div>
          <div className="nx-rem-sub">{sub}</div>
        </div>
      </div>
      <div className="nx-rem-chips" ref={chipsRef} role="status" aria-live="polite">
        <ReminderChipList items={items} extra={extra} now={now} startedAt={startedAt.current} />
      </div>
    </>
  );
}
