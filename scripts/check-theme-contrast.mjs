#!/usr/bin/env node
// Contrast gate for the design tokens: every text/surface pair that matters, in BOTH themes, against WCAG 2.2 AA.
//   text on its surface .......... 4.5:1   (1.4.3)
//   focus ring, borders that carry meaning, UI components ... 3:1   (1.4.11)
// Reads frontend/src/theme/tokens.css and the Command Hub block of frontend/src/sidebar/unified.css, resolves var()
// chains, composites translucent colours over their surface, and fails on any pair under its threshold.
// A token that is renamed or removed fails loudly here instead of silently dropping out of the check.
//
//   node scripts/check-theme-contrast.mjs          # check, exit 1 on a failure
//   node scripts/check-theme-contrast.mjs --table  # also print every ratio

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const FILES = ["frontend/src/theme/tokens.css", "frontend/src/sidebar/unified.css", "frontend/src/pr-list/pr-list.css", "frontend/src/architect/architect.css", "frontend/src/sidebar/sidebar.css", "frontend/src/companion-hud/companion-hud.css"].map((f) => join(ROOT, f));

/** Custom-property maps per theme: dark = `:root`/dark blocks, light = dark overlaid by the light blocks. */
export function loadThemes(cssTexts) {
  const dark = new Map();
  const lightOnly = new Map();
  for (const css of cssTexts) {
    const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
    for (const m of text.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const selector = m[1].trim();
      const target = /data-theme\s*=\s*["']light["']/.test(selector) ? lightOnly : /:root|data-theme\s*=\s*["']dark["']|^\.gh-dashboard-shell$/.test(selector) ? dark : null;
      if (!target) continue;
      for (const d of m[2].matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) target.set(d[1], d[2].trim());
    }
  }
  return { dark, light: new Map([...dark, ...lightOnly]) };
}

function resolveVars(value, map, depth = 0) {
  if (depth > 8) throw new Error(`var() chain too deep: ${value}`);
  return value.replace(/var\(\s*(--[\w-]+)\s*(?:,[^)]*)?\)/g, (_, name) => {
    if (!map.has(name)) throw new Error(`token ${name} is not defined`);
    return resolveVars(map.get(name), map, depth + 1);
  });
}

/** -> {r,g,b,a} (0-255, a 0-1) */
export function parseColor(raw) {
  const v = raw.trim().toLowerCase();
  let m = v.match(/^#([0-9a-f]{3,8})$/);
  if (m) {
    let h = m[1];
    if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join("");
    return { r: parseInt(h.slice(0, 2), 16), g: parseInt(h.slice(2, 4), 16), b: parseInt(h.slice(4, 6), 16), a: h.length === 8 ? parseInt(h.slice(6, 8), 16) / 255 : 1 };
  }
  m = v.match(/^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+%?))?\s*\)$/);
  if (m) {
    const a = m[4] === undefined ? 1 : m[4].endsWith("%") ? parseFloat(m[4]) / 100 : parseFloat(m[4]);
    return { r: +m[1], g: +m[2], b: +m[3], a };
  }
  throw new Error(`cannot parse colour: ${raw}`);
}

export function over(fg, bg) {
  const a = fg.a;
  return { r: fg.r * a + bg.r * (1 - a), g: fg.g * a + bg.g * (1 - a), b: fg.b * a + bg.b * (1 - a), a: 1 };
}

function luminance({ r, g, b }) {
  const lin = (c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function ratio(fg, bg) {
  const solidBg = over(bg, { r: 0, g: 0, b: 0, a: 1 }); // a surface is opaque by rule
  const f = over(fg, solidBg);
  const [hi, lo] = [luminance(f), luminance(solidBg)].sort((a, b) => b - a);
  return (hi + 0.05) / (lo + 0.05);
}

const TEXT = 4.5;
const UI = 3;
const SURFACES = ["--ui-surface-app", "--ui-surface-card", "--ui-surface-raised"];

/** [foreground token, background token, minimum ratio, what it is] */
export const PAIRS = [
  ...SURFACES.flatMap((s) => [
    ["--ui-text-primary", s, TEXT, "primary text"],
    ["--ui-text-secondary", s, TEXT, "secondary text"],
    ["--ui-text-tertiary", s, TEXT, "tertiary text"],
    ["--ui-accent-text", s, TEXT, "accent as text / link"],
    ["--ui-danger-text", s, TEXT, "danger text"],
    ["--ui-warning-text", s, TEXT, "warning text"],
    ["--ui-success-text", s, TEXT, "success text"],
    ["--ui-attention-text", s, TEXT, "attention text"],
    ["--ui-info-text", s, TEXT, "info text"],
    ["--ui-sev-critical", s, TEXT, "severity: critical"],
    ["--ui-sev-major", s, TEXT, "severity: major"],
    ["--ui-sev-minor", s, TEXT, "severity: minor"],
    ["--ui-sev-trivial", s, TEXT, "severity: trivial"],
    ["--ui-sev-info", s, TEXT, "severity: info"],
    ["--ui-sev-unknown", s, TEXT, "severity: unknown"],
    ["--ui-focus-ring", s, UI, "focus ring"],
    ["--ui-border-strong", s, UI, "meaningful border"],
  ]),
  ["--ui-text-primary", "--ui-surface-sunken", TEXT, "text on a code block"],
  ["--ui-on-accent", "--ui-attention-solid", TEXT, "text on an attention fill"],
  ["--ui-on-success", "--ui-success-solid", TEXT, "text on a success fill"],
  ["--ui-on-accent", "--ui-accent-solid", TEXT, "text on an accent button"],
  ["--ui-on-accent", "--ui-danger-solid", TEXT, "text on a danger button"],
  // Stage overlay chips and cards: solid dark in both themes by rule, so each pair is measured in both.
  ...["--stage-text", "--stage-text-muted", "--stage-accent", "--stage-attention", "--stage-ok"].map((fg) => [fg, "--stage-surface", TEXT, "stage chip text"]),
  ["--stage-text", "--stage-surface-live", TEXT, "text on the live hand chip"],
  // GitHub dashboard palette (pr-list/pr-list.css): GitHub-style colours, scoped to the dashboard shell.
  ...["--gh-bg", "--gh-surface", "--gh-surface-subtle"].flatMap((s) => [
    ["--gh-text-primary", s, TEXT, "dashboard text"],
    ["--gh-text-secondary", s, TEXT, "dashboard secondary text"],
    ["--gh-text-muted", s, TEXT, "dashboard muted text"],
    ["--gh-accent", s, TEXT, "dashboard accent as text"],
  ]),
  // Architecture Mapper palette (architect/architect.css).
  ...["--apple-surface-base", "--apple-surface-sidebar", "--apple-surface-muted", "--apple-surface-raised"].flatMap((s) => [
    ["--apple-text-primary", s, TEXT, "mapper text"],
    ["--apple-text-secondary", s, TEXT, "mapper secondary text"],
    ["--apple-text-tertiary", s, TEXT, "mapper tertiary text"],
    ["--apple-text-accent", s, TEXT, "mapper accent as text"],
  ]),
  // Calibration HUD (companion-hud/companion-hud.css).
  ["--hud-text", "--hud-bg", TEXT, "hud text"],
  ["--hud-dim", "--hud-bg", TEXT, "hud secondary text"],
  ["--hud-accent", "--hud-bg", UI, "hud accent (non-text)"],
  // Command Hub palette (sidebar/unified.css), still the live one until the views migrate.
  ...["--hub-bg", "--hub-card"].flatMap((s) => [
    ["--hub-text", s, TEXT, "hub text"],
    ["--hub-text-dim", s, TEXT, "hub secondary text"],
    ["--hub-text-faint", s, TEXT, "hub faint text"],
    ["--hub-accent", s, TEXT, "hub accent as text"],
    ["--hub-danger", s, TEXT, "hub danger as text"],
  ]),
];

function evaluate(themes) {
  const rows = [];
  for (const [name, map] of Object.entries(themes)) {
    for (const [fgTok, bgTok, min, what] of PAIRS) {
      let ok = false;
      let value = NaN;
      let note = "";
      try {
        const bg = parseColor(resolveVars(map.get(bgTok) ?? (() => { throw new Error(`token ${bgTok} is not defined`); })(), map));
        const fg = parseColor(resolveVars(map.get(fgTok) ?? (() => { throw new Error(`token ${fgTok} is not defined`); })(), map));
        value = ratio(fg, bg);
        ok = value >= min;
      } catch (e) {
        note = e.message;
      }
      rows.push({ theme: name, fgTok, bgTok, min, what, value, ok, note });
    }
  }
  return rows;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const themes = loadThemes(FILES.map((f) => readFileSync(f, "utf8")));
  const rows = evaluate(themes);
  if (process.argv.includes("--table")) {
    for (const r of rows) console.log(`${r.ok ? "ok  " : "FAIL"} ${r.theme.padEnd(5)} ${r.value ? r.value.toFixed(2).padStart(6) : "   n/a"}:1 (min ${r.min})  ${r.fgTok} on ${r.bgTok}  [${r.what}]${r.note ? "  " + r.note : ""}`);
  }
  const bad = rows.filter((r) => !r.ok);
  if (bad.length) {
    console.log(`theme contrast gate FAILED (${bad.length} of ${rows.length} pairs):`);
    for (const r of bad) console.log(`  ${r.theme}: ${r.fgTok} on ${r.bgTok} = ${Number.isNaN(r.value) ? "?" : r.value.toFixed(2)}:1, needs ${r.min}:1 (${r.what})${r.note ? " - " + r.note : ""}`);
    process.exit(1);
  }
  console.log(`theme contrast gate OK (${rows.length} pairs, dark and light)`);
}
