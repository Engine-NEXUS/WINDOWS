/**
 * Normalise the payload of the Rust `get_learned_corrections` command.
 *
 * Rust returns `Vec<LearnedCorrection>`, which serialises as OBJECTS
 * (`{ from, to, context_before, count, … }`). The frontend used to destructure
 * each entry as a `[from, to]` TUPLE — that throws on an object, the
 * surrounding `catch {}` swallowed it, and `learnedCorrections` stayed empty,
 * so every learned STT correction was silently never applied.
 *
 * Accepts both shapes (objects and legacy tuples) and drops anything that
 * isn't a pair of non-empty strings.
 */
export interface LearnedCorrectionPair {
  from: string;
  to: string;
  /** The word that preceded `from` when it was learned ("" = start of sentence / unknown). */
  context?: string;
}

export function normalizeLearnedCorrections(raw: unknown): LearnedCorrectionPair[] {
  if (!Array.isArray(raw)) return [];
  const out: LearnedCorrectionPair[] = [];
  for (const item of raw) {
    let from: unknown;
    let to: unknown;
    let context: unknown;
    if (Array.isArray(item)) {
      [from, to] = item;
    } else if (item && typeof item === "object") {
      const o = item as Record<string, unknown>;
      from = o.from;
      to = o.to;
      context = o.context_before;
    }
    if (typeof from === "string" && typeof to === "string" && from.length > 0 && to.length > 0) {
      out.push(typeof context === "string" && context.length > 0 ? { from, to, context } : { from, to });
    }
  }
  return out;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** One correction as a whole-word, case-insensitive pattern. A learned `in`->`im` must never
 *  rewrite "painting", and a correction learned after "in" must not fire after other words. */
function patternFor(c: LearnedCorrectionPair): RegExp {
  const word = "(?<![\\w'])" + escapeRegExp(c.from) + "(?![\\w'])";
  const ctx = c.context ? "(?<=(?<![\\w'])" + escapeRegExp(c.context) + "\\s+)" : "";
  return new RegExp(ctx + word, "gi");
}

export interface AppliedCorrection {
  from: string;
  to: string;
}

/**
 * Apply learned corrections to a transcript. Returns the new text and which corrections actually
 * changed something (so the caller can count hits). Whole words only; honours the learned context.
 */
export function applyLearnedCorrections(
  transcript: string,
  corrections: LearnedCorrectionPair[],
): { text: string; applied: AppliedCorrection[] } {
  let text = transcript;
  const applied: AppliedCorrection[] = [];
  for (const c of corrections) {
    const re = patternFor(c);
    const next = text.replace(re, c.to);
    if (next !== text) {
      applied.push({ from: c.from, to: c.to });
      text = next;
    }
  }
  return { text, applied };
}
