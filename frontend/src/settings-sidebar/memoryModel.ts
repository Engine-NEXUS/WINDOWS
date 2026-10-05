/**
 * Pure model for the Memory tab: types for the `memory_overview` payload, a
 * defensive normaliser (the panel must never crash on a partial/old backend
 * response), and the small bits of copy/validation the UI needs.
 */

export type ProfileSection = "identity" | "can_do" | "needs" | "priorities";

export interface ProfileEntry {
  text: string;
  ts: number;
  /** Contains PII: kept on this machine only (never sent to a cloud model). */
  sensitive: boolean;
}

export interface FactEntry {
  key: string;
  value: string;
  sensitive: boolean;
}

export interface HabitSuggestion {
  key: string;
  text: string;
  reason: string;
}

export interface MemoryOverview {
  profile: Record<ProfileSection, ProfileEntry[]>;
  facts: FactEntry[];
  habits: { top: { name: string; count: number }[]; suggestions: HabitSuggestion[] };
  conversationTurns: number;
}

export const SECTION_META: {
  id: ProfileSection;
  title: string;
  hint: string;
  placeholder: string;
}[] = [
  { id: "identity", title: "Who you are", hint: "Say: “remember that I am …”", placeholder: "e.g. I am a final-year CS student" },
  { id: "can_do", title: "What you can do", hint: "Say: “remember I can …”", placeholder: "e.g. I can code in Rust and Python" },
  { id: "needs", title: "What you need", hint: "Say: “remember that I need …”", placeholder: "e.g. I need to renew my passport" },
  { id: "priorities", title: "Your priorities", hint: "Say: “my priority is …”", placeholder: "e.g. Ship the Ghost Mode fixes" },
];

export function emptyOverview(): MemoryOverview {
  return {
    profile: { identity: [], can_do: [], needs: [], priorities: [] },
    facts: [],
    habits: { top: [], suggestions: [] },
    conversationTurns: 0,
  };
}

function asArray(v: unknown): unknown[] {
  return Array.isArray(v) ? v : [];
}

function str(v: unknown): string {
  return typeof v === "string" ? v : "";
}

function normEntries(v: unknown): ProfileEntry[] {
  return asArray(v)
    .map((e) => {
      const o = (e ?? {}) as Record<string, unknown>;
      return { text: str(o.text), ts: typeof o.ts === "number" ? o.ts : 0, sensitive: o.sensitive === true };
    })
    .filter((e) => e.text.length > 0);
}

/** Turn whatever the backend returned into a fully-populated overview. */
export function normalizeOverview(raw: unknown): MemoryOverview {
  const r = (raw ?? {}) as Record<string, unknown>;
  const p = (r.profile ?? {}) as Record<string, unknown>;
  const h = (r.habits ?? {}) as Record<string, unknown>;
  return {
    profile: {
      identity: normEntries(p.identity),
      can_do: normEntries(p.can_do),
      needs: normEntries(p.needs),
      priorities: normEntries(p.priorities),
    },
    facts: asArray(r.facts)
      .map((f) => {
        const o = (f ?? {}) as Record<string, unknown>;
        return { key: str(o.key), value: str(o.value), sensitive: o.sensitive === true };
      })
      .filter((f) => f.key.length > 0),
    habits: {
      top: asArray(h.top)
        .map((t) => {
          const o = (t ?? {}) as Record<string, unknown>;
          return { name: str(o.name), count: typeof o.count === "number" ? o.count : 0 };
        })
        .filter((t) => t.name.length > 0),
      suggestions: asArray(h.suggestions)
        .map((s) => {
          const o = (s ?? {}) as Record<string, unknown>;
          return { key: str(o.key), text: str(o.text), reason: str(o.reason) };
        })
        .filter((s) => s.key.length > 0 && s.text.length > 0),
    },
    conversationTurns: typeof r.conversation_turns === "number" ? r.conversation_turns : 0,
  };
}

/** Message for the result of `memory_profile_add`. */
export function addOutcomeMessage(outcome: string): string {
  switch (outcome) {
    case "added":
      return "Saved.";
    case "added_private":
      return "Saved — it looks private, so it stays on this PC and is never sent to a cloud model.";
    case "duplicate":
      return "Already saved.";
    default:
      return "Couldn't save that.";
  }
}

/** A remembered fact needs both a name and a value. */
export function isValidFactInput(key: string, value: string): boolean {
  return key.trim().length > 0 && value.trim().length > 0;
}

export function totalProfileEntries(o: MemoryOverview): number {
  return (Object.values(o.profile) as ProfileEntry[][]).reduce((n, list) => n + list.length, 0);
}
