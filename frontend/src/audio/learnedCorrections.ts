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
}

export function normalizeLearnedCorrections(raw: unknown): LearnedCorrectionPair[] {
  if (!Array.isArray(raw)) return [];
  const out: LearnedCorrectionPair[] = [];
  for (const item of raw) {
    let from: unknown;
    let to: unknown;
    if (Array.isArray(item)) {
      [from, to] = item;
    } else if (item && typeof item === "object") {
      from = (item as Record<string, unknown>).from;
      to = (item as Record<string, unknown>).to;
    }
    if (typeof from === "string" && typeof to === "string" && from.length > 0 && to.length > 0) {
      out.push({ from, to });
    }
  }
  return out;
}
