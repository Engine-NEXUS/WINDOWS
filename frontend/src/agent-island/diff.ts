/**
 * Line diff for the Agent Island's live code pane.
 *
 * Ported from `windows/src/core/diff.ts` of Louis-CFM/coucou (MIT, Copyright (c) 2026 Louis Raillé; notice in
 * THIRD_PARTY_NOTICES.md): an LCS line diff with 3-line context hunks, and the same size guards. Changed: the
 * input is NEXUS's `EditView` (agent_monitor.rs sends the old and new text of an Edit / MultiEdit / Write), and the
 * parts that only coucou's ticker used are dropped.
 */

export type DiffKind = "context" | "added" | "removed";

export interface DiffLine {
  kind: DiffKind;
  text: string;
  /** 1-based; -1 for pure adds. */
  origLine: number;
  /** 1-based; -1 for pure removes. */
  newLine: number;
}

export interface DiffHunk {
  origStart: number;
  newStart: number;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  added: number;
  removed: number;
  hunks: DiffHunk[];
  tooLarge: boolean;
  isNewFile: boolean;
}

export interface EditPair {
  old: string;
  new: string;
}

/** What agent_monitor.rs sends: `EditView`. */
export interface EditView {
  path: string;
  kind: "edit" | "multi" | "write";
  pairs: EditPair[];
  tooLarge: boolean;
}

export const DIFF_MAX_BYTES = 200 * 1024;
export const DIFF_MAX_LINES = 4000;
/** LCS is O(m·n): bail out before the quadratic blowup. */
const DIFF_MAX_CELLS = 1_000_000;
const CONTEXT = 3;

const encoder = new TextEncoder();
const utf8Length = (s: string) => encoder.encode(s).length;

/** Last path component, Windows or POSIX separators. */
export function fileName(path: string): string {
  const cleaned = path.replace(/[\\/]+$/, "");
  const idx = Math.max(cleaned.lastIndexOf("\\"), cleaned.lastIndexOf("/"));
  return idx >= 0 ? cleaned.slice(idx + 1) : cleaned;
}

function splitLines(text: string): string[] {
  const parts = text.replace(/\r\n/g, "\n").split("\n");
  if (parts[parts.length - 1] === "") parts.pop();
  return parts;
}

function fromEdit(oldText: string, newText: string, path: string): FileDiff {
  const oldLines = splitLines(oldText);
  const newLines = splitLines(newText);
  if (
    utf8Length(oldText) + utf8Length(newText) > DIFF_MAX_BYTES ||
    oldLines.length + newLines.length > DIFF_MAX_LINES ||
    oldLines.length * newLines.length > DIFF_MAX_CELLS
  ) {
    return countFallback(oldLines, newLines, path);
  }
  const flat = buildDiffLines(oldLines, newLines);
  let added = 0;
  let removed = 0;
  for (const l of flat) {
    if (l.kind === "added") added++;
    else if (l.kind === "removed") removed++;
  }
  return { path, added, removed, hunks: buildHunks(flat, CONTEXT), tooLarge: false, isNewFile: false };
}

function fromNew(content: string, path: string): FileDiff {
  const lines = splitLines(content);
  if (utf8Length(content) > DIFF_MAX_BYTES || lines.length > DIFF_MAX_LINES) {
    return { path, added: lines.length, removed: 0, hunks: [], tooLarge: true, isNewFile: true };
  }
  const diffLines: DiffLine[] = lines.map((text, i) => ({ kind: "added", text, origLine: -1, newLine: i + 1 }));
  return {
    path,
    added: diffLines.length,
    removed: 0,
    hunks: diffLines.length ? [{ origStart: 0, newStart: 1, lines: diffLines }] : [],
    tooLarge: false,
    isNewFile: true,
  };
}

/** The diff of one tool call, or null when it changes nothing. */
export function diffOf(edit: EditView): FileDiff | null {
  if (edit.tooLarge) return { path: edit.path, added: 0, removed: 0, hunks: [], tooLarge: true, isNewFile: edit.kind === "write" };
  if (edit.kind === "write") {
    const content = edit.pairs[0]?.new ?? "";
    if (!content) return null;
    return fromNew(content, edit.path);
  }
  let added = 0;
  let removed = 0;
  let tooLarge = false;
  const hunks: DiffHunk[] = [];
  for (const p of edit.pairs) {
    if (!p.old && !p.new) continue;
    const d = fromEdit(p.old, p.new, edit.path);
    added += d.added;
    removed += d.removed;
    hunks.push(...d.hunks);
    if (d.tooLarge) tooLarge = true;
  }
  if (added === 0 && removed === 0 && !tooLarge) return null;
  return { path: edit.path, added, removed, hunks, tooLarge, isNewFile: false };
}

function buildDiffLines(oldLines: string[], newLines: string[]): DiffLine[] {
  const m = oldLines.length;
  const n = newLines.length;
  const w = n + 1;
  // dp[i*w + j] = LCS length of oldLines[0..<i] and newLines[0..<j]; lengths stay under DIFF_MAX_LINES.
  const dp = new Uint16Array((m + 1) * w);
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[i * w + j] =
        oldLines[i - 1] === newLines[j - 1] ? dp[(i - 1) * w + j - 1] + 1 : Math.max(dp[(i - 1) * w + j], dp[i * w + j - 1]);
    }
  }
  const matches: [number, number][] = [];
  let i = m;
  let j = n;
  while (i > 0 && j > 0) {
    if (oldLines[i - 1] === newLines[j - 1]) {
      matches.push([i - 1, j - 1]);
      i--;
      j--;
    } else if (dp[(i - 1) * w + j] >= dp[i * w + j - 1]) {
      i--;
    } else {
      j--;
    }
  }
  matches.reverse();

  const result: DiffLine[] = [];
  let prevOld = -1;
  let prevNew = -1;
  const removedLine = (k: number): DiffLine => ({ kind: "removed", text: oldLines[k], origLine: k + 1, newLine: -1 });
  const addedLine = (k: number): DiffLine => ({ kind: "added", text: newLines[k], origLine: -1, newLine: k + 1 });
  for (const [oi, ni] of matches) {
    for (let k = prevOld + 1; k < oi; k++) result.push(removedLine(k));
    for (let k = prevNew + 1; k < ni; k++) result.push(addedLine(k));
    result.push({ kind: "context", text: oldLines[oi], origLine: oi + 1, newLine: ni + 1 });
    prevOld = oi;
    prevNew = ni;
  }
  for (let k = prevOld + 1; k < m; k++) result.push(removedLine(k));
  for (let k = prevNew + 1; k < n; k++) result.push(addedLine(k));
  return result;
}

function buildHunks(lines: DiffLine[], context: number): DiffHunk[] {
  const merged: [number, number][] = [];
  lines.forEach((line, i) => {
    if (line.kind === "context") return;
    const start = Math.max(0, i - context);
    const end = Math.min(lines.length - 1, i + context);
    const last = merged[merged.length - 1];
    if (last && start <= last[1] + 1) last[1] = Math.max(last[1], end);
    else merged.push([start, end]);
  });
  return merged.map(([start, end]) => {
    const hunkLines = lines.slice(start, end + 1);
    return {
      origStart: hunkLines.find((l) => l.origLine > 0)?.origLine ?? 1,
      newStart: hunkLines.find((l) => l.newLine > 0)?.newLine ?? 1,
      lines: hunkLines,
    };
  });
}

function countFallback(oldLines: string[], newLines: string[], path: string): FileDiff {
  const oldSet = new Set(oldLines);
  const newSet = new Set(newLines);
  const added = newLines.filter((l) => l !== "" && !oldSet.has(l)).length;
  const removed = oldLines.filter((l) => l !== "" && !newSet.has(l)).length;
  return { path, added, removed, hunks: [], tooLarge: true, isNewFile: false };
}
