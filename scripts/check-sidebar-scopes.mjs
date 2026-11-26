#!/usr/bin/env node
// Scope gate for the sidebar view family (docs/features/198-sidebar-design-family-analysis-and-plan.md).
//
// Each sidebar view lives in frontend/src/sidebar/views/<name>/ and owns ONE root class, `.view-<name>`. Every selector in that
// folder's stylesheets must start with that class, and every @keyframes name must start with `<name>-`. That is what stops a rule
// written for one view from restyling another (all views stay mounted in one window). A literal colour is a separate rule, enforced
// by scripts/check-hardcoded-colors.mjs (colours live in theme/tokens.css only).
//
//   node scripts/check-sidebar-scopes.mjs            # check, exit 1 on any violation
//   node scripts/check-sidebar-scopes.mjs --selftest # prove the checker catches what it should

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const VIEWS = join(ROOT, "frontend", "src", "sidebar", "views");

export function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Split a stylesheet into top-level blocks: { prelude, body } (body without the outer braces). */
function blocks(css) {
  const out = [];
  let depth = 0;
  let start = 0;
  let preludeStart = 0;
  let prelude = "";
  for (let i = 0; i < css.length; i++) {
    const c = css[i];
    if (c === "{") {
      if (depth === 0) {
        prelude = css.slice(preludeStart, i).trim();
        start = i + 1;
      }
      depth++;
    } else if (c === "}") {
      depth--;
      if (depth === 0) {
        out.push({ prelude, body: css.slice(start, i) });
        preludeStart = i + 1;
      }
    } else if (c === ";" && depth === 0) {
      preludeStart = i + 1; // an @import or @charset line
    }
  }
  return out;
}

/** Every violation in one stylesheet for a view named `name`. */
export function violations(css, name) {
  const root = `.view-${name}`;
  const found = [];
  const walk = (text) => {
    for (const { prelude, body } of blocks(stripComments(text))) {
      if (prelude.startsWith("@keyframes") || prelude.startsWith("@-webkit-keyframes")) {
        const kf = prelude.replace(/^@(-webkit-)?keyframes\s+/, "").trim();
        if (!kf.startsWith(`${name}-`)) found.push(`@keyframes ${kf} must be named ${name}-…`);
        continue;
      }
      if (prelude.startsWith("@media") || prelude.startsWith("@container") || prelude.startsWith("@supports") || prelude.startsWith("@layer")) {
        walk(body);
        continue;
      }
      if (prelude.startsWith("@")) continue; // @font-face and friends: not selectors
      for (const sel of prelude.split(",").map((s) => s.trim()).filter(Boolean)) {
        // `[data-theme="light"] .view-x ...` is allowed: the theme attribute may come first.
        const s = sel.replace(/^\[data-theme="(light|dark)"\]\s+/, "");
        const ok = s === root || s.startsWith(`${root} `) || s.startsWith(`${root}.`) || s.startsWith(`${root}:`) || s.startsWith(`${root}>`) || s.startsWith(`${root}[`);
        if (!ok) found.push(`selector "${sel}" must start with ${root}`);
      }
    }
  };
  walk(css);
  return found;
}

function cssIn(dir) {
  return readdirSync(dir).filter((f) => f.endsWith(".css")).map((f) => join(dir, f));
}

export function checkAll() {
  const report = [];
  if (!existsSync(VIEWS)) return report;
  for (const name of readdirSync(VIEWS)) {
    const dir = join(VIEWS, name);
    if (!statSync(dir).isDirectory()) continue;
    for (const file of cssIn(dir)) {
      for (const v of violations(readFileSync(file, "utf8"), name)) report.push(`${relative(ROOT, file)}: ${v}`);
    }
  }
  return report;
}

function selftest() {
  const bad = violations(`.view-demo .a { color: red } .b { x: y } @keyframes spin { from {a:b} } @media (min-width: 1px) { .c { x: y } }`, "demo");
  const good = violations(`.view-demo { a: b } .view-demo .a, .view-demo:hover { a: b } [data-theme="light"] .view-demo .x { a: b } @keyframes demo-spin { from { a: b } } @container (min-width: 1px) { .view-demo .y { a: b } } /* .z { } */`, "demo");
  const checks = [
    [bad.length === 3, `expected 3 violations, got ${bad.length}: ${bad.join(" | ")}`],
    [bad.some((v) => v.includes('".b"')), "unscoped .b not caught"],
    [bad.some((v) => v.includes("@keyframes spin")), "unprefixed keyframes not caught"],
    [bad.some((v) => v.includes('".c"')), "unscoped rule inside @media not caught"],
    [good.length === 0, `false positives: ${good.join(" | ")}`],
    [violations(".view-demox .a { a: b }", "demo").length === 1, "a similarly named root must not pass"],
  ];
  const failed = checks.filter(([ok]) => !ok);
  if (failed.length) {
    for (const [, msg] of failed) console.error(`selftest FAILED: ${msg}`);
    process.exit(1);
  }
  console.log(`sidebar scope gate selftest OK (${checks.length} checks)`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.includes("--selftest")) selftest();
  else {
    const report = checkAll();
    if (report.length) {
      console.error(`sidebar scope gate FAILED (${report.length}):`);
      for (const r of report) console.error(`  ${r}`);
      process.exit(1);
    }
    console.log("sidebar scope gate OK");
  }
}
