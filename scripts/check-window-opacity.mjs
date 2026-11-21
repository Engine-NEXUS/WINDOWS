#!/usr/bin/env node
// Window Opacity Regression Guard
// Ensures every registered panel window surface is solid opaque (alpha 1)
// and has no backdrop-filter / blurred backdrop image layers.
//
// Usage:
//   node scripts/check-window-opacity.mjs
//   node scripts/check-window-opacity.mjs --selftest
// From frontend/:
//   node ../scripts/check-window-opacity.mjs
//   node ../scripts/check-window-opacity.mjs --selftest

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = join(SCRIPT_DIR, "..");
const REGISTRY_PATH = join(SCRIPT_DIR, "window-opacity-registry.json");

/** Remove CSS comments while keeping string structure intact */
export function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Balanced-brace CSS rule scanner */
export function parseRules(css) {
  // Statement at-rules have no block; left in, they glue onto the next selector and hide its whole rule.
  const clean = stripComments(css).replace(/@(?:import|charset|namespace)[^;{]*;/gi, "");
  const rules = [];

  function scan(text) {
    let i = 0;
    while (i < text.length) {
      const openBrace = text.indexOf("{", i);
      if (openBrace === -1) break;

      const prelude = text.slice(i, openBrace).trim();

      let depth = 1;
      let j = openBrace + 1;
      while (j < text.length && depth > 0) {
        if (text[j] === "{") depth++;
        else if (text[j] === "}") depth--;
        j++;
      }

      const body = text.slice(openBrace + 1, j - 1);
      i = j;

      if (prelude.startsWith("@")) {
        // At-rule (@media, @supports, etc.): scan nested rules
        scan(body);
      } else {
        const selectors = prelude
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean);
        rules.push({ selectors, prelude, body });
      }
    }
  }

  scan(clean);
  return rules;
}

/** Parse property declarations from a CSS rule body */
export function parseDeclarations(body) {
  const decls = [];
  let cur = "";
  let parenDepth = 0;
  for (let i = 0; i < body.length; i++) {
    const ch = body[i];
    if (ch === "(") parenDepth++;
    else if (ch === ")") parenDepth--;

    if (ch === ";" && parenDepth === 0) {
      const colon = cur.indexOf(":");
      if (colon !== -1) {
        const prop = cur.slice(0, colon).trim().toLowerCase();
        const val = cur.slice(colon + 1).trim();
        decls.push({ prop, val });
      }
      cur = "";
    } else {
      cur += ch;
    }
  }
  if (cur.trim()) {
    const colon = cur.indexOf(":");
    if (colon !== -1) {
      const prop = cur.slice(0, colon).trim().toLowerCase();
      const val = cur.slice(colon + 1).trim();
      decls.push({ prop, val });
    }
  }
  return decls;
}

/** Check if a CSS color value contains literal rgba/hsla/#rrggbbaa/#rgba with alpha < 1 */
export function checkTranslucentColor(value) {
  // rgba(r, g, b, alpha)
  const rgbaRegex = /rgba\s*\(\s*[^,)]+\s*,\s*[^,)]+\s*,\s*[^,)]+\s*,\s*([0-9.]+)(%?)\s*\)/gi;
  let match;
  while ((match = rgbaRegex.exec(value)) !== null) {
    const num = parseFloat(match[1]);
    const alpha = match[2] === "%" ? num / 100 : num;
    if (alpha < 1) {
      return `rgba alpha ${alpha} < 1`;
    }
  }

  // hsla(h, s, l, alpha)
  const hslaRegex = /hsla\s*\(\s*[^,)]+\s*,\s*[^,)]+\s*,\s*[^,)]+\s*,\s*([0-9.]+)(%?)\s*\)/gi;
  while ((match = hslaRegex.exec(value)) !== null) {
    const num = parseFloat(match[1]);
    const alpha = match[2] === "%" ? num / 100 : num;
    if (alpha < 1) {
      return `hsla alpha ${alpha} < 1`;
    }
  }

  // Modern space syntax: rgb(r g b / a), hsl(h s l / a), and rgba/hsla written that way.
  const slashRegex = /(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\s*\([^)]*\/\s*([0-9.]+)(%?)\s*\)/gi;
  while ((match = slashRegex.exec(value)) !== null) {
    const num = parseFloat(match[1]);
    const alpha = match[2] === "%" ? num / 100 : num;
    if (alpha < 1) {
      return `slash alpha ${alpha} < 1`;
    }
  }

  // The `transparent` keyword (alone, in a gradient or mixed in with color-mix()).
  if (/(^|[^a-z-])transparent([^a-z-]|$)/i.test(value)) {
    return "transparent keyword";
  }

  // 8-digit hex #rrggbbaa
  const hex8Regex = /#([0-9a-fA-F]{6})([0-9a-fA-F]{2})\b/g;
  while ((match = hex8Regex.exec(value)) !== null) {
    const alpha = parseInt(match[2], 16) / 255;
    if (alpha < 1) {
      return `#rrggbbaa alpha ${alpha.toFixed(2)} < 1`;
    }
  }

  // 4-digit hex #rgba
  const hex4Regex = /#([0-9a-fA-F]{3})([0-9a-fA-F])\b/g;
  while ((match = hex4Regex.exec(value)) !== null) {
    const alpha = parseInt(match[2] + match[2], 16) / 255;
    if (alpha < 1) {
      return `#rgba alpha ${alpha.toFixed(2)} < 1`;
    }
  }

  return null;
}

/**
 * Custom-property table: name -> every value it is given anywhere in `cssTexts` (all themes, all files).
 * The checker cannot know which theme is active, so a surface is solid only if EVERY definition is.
 */
export function collectTokens(cssTexts) {
  const tokens = new Map();
  for (const css of cssTexts) {
    for (const rule of parseRules(css)) {
      for (const { prop, val } of parseDeclarations(rule.body)) {
        if (!prop.startsWith("--")) continue;
        if (!tokens.has(prop)) tokens.set(prop, new Set());
        tokens.get(prop).add(val.replace(/\s*!important\s*$/i, "").trim());
      }
    }
  }
  return tokens;
}

/** First top-level `var(--name[, fallback])` in `value`, or null. Handles nested parentheses. */
function findVar(value) {
  const at = value.indexOf("var(");
  if (at === -1) return null;
  let depth = 0;
  for (let i = at + 3; i < value.length; i++) {
    if (value[i] === "(") depth++;
    else if (value[i] === ")") {
      depth--;
      if (depth === 0) {
        const inner = value.slice(at + 4, i);
        const comma = inner.indexOf(",");
        const name = (comma === -1 ? inner : inner.slice(0, comma)).trim();
        const fallback = comma === -1 ? null : inner.slice(comma + 1).trim();
        return { start: at, end: i + 1, name, fallback };
      }
    }
  }
  return null;
}

/**
 * Every concrete value `value` can take after substituting custom properties (bounded: depth 10,
 * 64 results). An undefined token without a fallback yields the marker `unresolved(--name)`.
 */
export function resolveValues(value, tokens, depth = 0) {
  const v = findVar(value);
  if (!v) return [value];
  if (depth > 10) return [`unresolved(${v.name}: too deep)`];
  const defs = tokens.get(v.name);
  const options = defs && defs.size ? [...defs] : v.fallback !== null ? [v.fallback] : [`unresolved(${v.name})`];
  const out = [];
  for (const opt of options) {
    const substituted = value.slice(0, v.start) + opt + value.slice(v.end);
    for (const r of resolveValues(substituted, tokens, depth + 1)) {
      if (!out.includes(r)) out.push(r);
      if (out.length >= 64) return out;
    }
  }
  return out;
}

/** The subject (last compound) of a selector: `.a .b:hover > .c.d` -> `.c.d`. */
export function subjectCompound(selector) {
  const parts = selector.trim().split(/\s*[>+~]\s*|\s+/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : "";
}

/** Escape special regex characters in a selector string */
function escapeRegex(str) {
  return str.replace(/[-[\]{}()*+?.,\\^$|#\s]/g, "\\$&");
}

/** Check CSS content for a set of registered surface selectors */
export function checkCssContent(filePath, cssContent, registeredSelectors, tokens = collectTokens([cssContent])) {
  const violations = [];
  const rules = parseRules(cssContent);
  const foundSelectors = new Set();

  for (const target of registeredSelectors) {
    const escaped = escapeRegex(target);
    const targetPattern = new RegExp(`(?:^|[^a-zA-Z0-9_-])${escaped}(?=[^a-zA-Z0-9_-]|$)`);

    for (const rule of rules) {
      for (const sel of rule.selectors) {
        if (!targetPattern.test(sel)) continue;
        foundSelectors.add(target);
        // Only rules whose SUBJECT is the surface paint it; `.surface .child {}` paints a child.
        if (!targetPattern.test(subjectCompound(sel))) continue;
        // A hidden state (display: none) paints nothing.
        if (/display\s*:\s*none/i.test(rule.body)) continue;

        const isPseudo = /::?(after|before)\b/.test(sel);
        const decls = parseDeclarations(rule.body);

        if (isPseudo) {
          // Check ::after / ::before for --sidebar-backdrop-image
          for (const { prop, val } of decls) {
            if (
              (prop === "background-image" || prop === "background") &&
              val.includes("--sidebar-backdrop-image")
            ) {
              violations.push(
                `${filePath} ${sel}: sets background-image containing --sidebar-backdrop-image`
              );
            }
          }
        } else {
          // Check the surface itself
          for (const { prop, val } of decls) {
            if (prop === "backdrop-filter" || prop === "-webkit-backdrop-filter") {
              if (val.trim().toLowerCase() !== "none") {
                violations.push(
                  `${filePath} ${sel}: has ${prop}: ${val} (must be none or omitted)`
                );
              }
            } else if (prop === "background" || prop === "background-color") {
              for (const concrete of resolveValues(val, tokens)) {
                const unresolved = concrete.match(/unresolved\((--[\w-]+)/);
                const translucentReason = unresolved
                  ? `token ${unresolved[1]} is never defined`
                  : checkTranslucentColor(concrete);
                if (translucentReason) {
                  violations.push(
                    `${filePath} ${sel}: translucent ${prop} (${translucentReason}) in "${val}"` +
                      (concrete !== val ? ` -> "${concrete}"` : "")
                  );
                  break;
                }
              }
            }
          }
        }
      }
    }
  }

  for (const target of registeredSelectors) {
    if (!foundSelectors.has(target)) {
      violations.push(`${filePath} ${target}: registered selector not found in file (registry rotted)`);
    }
  }

  return violations;
}

/** Every .css file under `dir` (recursive), skipping node_modules and dist. */
export function listCss(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "dist") continue;
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...listCss(full));
    else if (name.endsWith(".css")) out.push(full);
  }
  return out;
}

/**
 * Blur anywhere in the panel CSS, registered or not. The fullscreen stage overlay (frontend/src/stage/)
 * is exempt: it is not a panel window, and CSS backdrop-filter is a no-op on its transparent WebView.
 */
export function blurOutsideStage(relPath, cssContent) {
  const norm = relPath.replace(/\\/g, "/");
  if (norm.includes("frontend/src/stage/")) return [];
  const out = [];
  for (const rule of parseRules(cssContent)) {
    for (const { prop, val } of parseDeclarations(rule.body)) {
      if ((prop === "backdrop-filter" || prop === "-webkit-backdrop-filter") && !/^none\b/i.test(val.trim())) {
        out.push(`${norm} ${rule.prelude}: ${prop}: ${val} (blur outside the stage)`);
      }
    }
  }
  return out;
}

/** Built-in selftest fixtures */
export function runSelfTest() {
  console.log("[check-window-opacity] Running selftest fixtures...");

  const passingCss = `
    :root { --apple-surface-base: #0a0a0c; }
    .sidebar-card {
      background-color: var(--apple-surface-base);
      border: 1px solid var(--apple-border-strong);
      color: #ffffff;
    }
    .sidebar-card:hover {
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
    }
    .sidebar-card::before {
      content: "";
    }
  `;

  const failingCssBlur = `
    .sidebar-card {
      background: var(--apple-surface-base);
      backdrop-filter: blur(20px);
    }
  `;

  const failingCssRgba = `
    .sidebar-card {
      background: rgba(28, 28, 34, 0.72);
    }
  `;

  const failingCssHex = `
    .sidebar-card {
      background: #15151c80;
    }
  `;

  const failingCssBackdropImage = `
    .sidebar-card {
      background: #111;
    }
    .sidebar-card::after {
      background-image: var(--sidebar-backdrop-image);
    }
  `;

  const failingCssMissingSelector = `
    .unrelated-element {
      background: #000;
    }
  `;

  const passViolations = checkCssContent("test-pass.css", passingCss, [".sidebar-card"]);
  if (passViolations.length !== 0) {
    console.error("FAIL: Passing fixture produced violations:", passViolations);
    return false;
  }

  const blurViolations = checkCssContent("test-blur.css", failingCssBlur, [".sidebar-card"]);
  if (!blurViolations.some((v) => v.includes("backdrop-filter"))) {
    console.error("FAIL: Failing blur fixture did not detect backdrop-filter violation:", blurViolations);
    return false;
  }

  const rgbaViolations = checkCssContent("test-rgba.css", failingCssRgba, [".sidebar-card"]);
  if (!rgbaViolations.some((v) => v.includes("translucent"))) {
    console.error("FAIL: Failing rgba fixture did not detect translucent violation:", rgbaViolations);
    return false;
  }

  const hexViolations = checkCssContent("test-hex.css", failingCssHex, [".sidebar-card"]);
  if (!hexViolations.some((v) => v.includes("translucent"))) {
    console.error("FAIL: Failing hex fixture did not detect translucent violation:", hexViolations);
    return false;
  }

  const imgViolations = checkCssContent("test-img.css", failingCssBackdropImage, [".sidebar-card"]);
  if (!imgViolations.some((v) => v.includes("--sidebar-backdrop-image"))) {
    console.error("FAIL: Failing backdrop-image fixture did not detect violation:", imgViolations);
    return false;
  }

  const missingViolations = checkCssContent("test-missing.css", failingCssMissingSelector, [".sidebar-card"]);
  if (!missingViolations.some((v) => v.includes("not found"))) {
    console.error("FAIL: Missing selector fixture did not detect rot violation:", missingViolations);
    return false;
  }

  const tokenCss = `
    :root { --surface: #101014; --glass: rgba(10, 10, 10, 0.5); --alias: var(--glass); }
    [data-theme="light"] { --surface: rgb(250 250 250 / 0.6); }
    .solid-card { background: var(--missing, #222); }
    .glass-card { background-color: var(--alias); }
    .theme-card { background: var(--surface); }
    .ghost-card { background: var(--never-defined); }
    .host .child-card { background: var(--glass); }
    .host { background: #000; }
    .host.hidden { display: none; background: transparent; }
    .clear-card { background: transparent; }
  `;
  const t = collectTokens([tokenCss]);
  const v = (sel) => checkCssContent("t.css", tokenCss, [sel], t);
  const cases = [
    [".solid-card", 0, "fallback hex is solid"],
    [".glass-card", 1, "alias to an rgba token"],
    [".theme-card", 1, "one theme's slash-alpha value"],
    [".ghost-card", 1, "undefined token"],
    [".host", 0, "child rules and display:none states do not paint the host"],
    [".clear-card", 1, "transparent keyword"],
  ];
  for (const [sel, want, why] of cases) {
    const got = v(sel).length;
    if (got !== want) {
      console.error(`FAIL: ${sel} (${why}): expected ${want} violation(s), got ${got}:`, v(sel));
      return false;
    }
  }
  // A file that starts with @import must not hide its first rules (they once glued onto the import).
  const importCss = [
    '@import "a.css";',
    '@import "b.css";',
    ":root { --s: rgba(0, 0, 0, 0.5); }",
    ".imp-card { background: var(--s); }",
  ].join("\n");
  if (checkCssContent("imp.css", importCss, [".imp-card"]).length !== 1) {
    console.error("FAIL: rules after @import statements were not parsed");
    return false;
  }
  if (blurOutsideStage("frontend/src/x/panel.css", ".p { backdrop-filter: blur(4px); }").length !== 1) {
    console.error("FAIL: blur in an unregistered panel file was not caught");
    return false;
  }
  if (blurOutsideStage("frontend/src/stage/ghost.css", ".p { backdrop-filter: blur(4px); }").length !== 0) {
    console.error("FAIL: the stage overlay must be exempt from the blur scan");
    return false;
  }
  if (blurOutsideStage("frontend/src/x/panel.css", ".p { backdrop-filter: none !important; }").length !== 0) {
    console.error("FAIL: backdrop-filter: none must pass");
    return false;
  }

  console.log("PASS: Selftest passed (all 16 fixtures classified correctly).");
  return true;
}

/** Main execution */
function main() {
  const args = process.argv.slice(2);
  const isSelfTest = args.includes("--selftest");

  if (isSelfTest) {
    const ok = runSelfTest();
    process.exit(ok ? 0 : 1);
  }

  if (!existsSync(REGISTRY_PATH)) {
    console.error(`FATAL: Registry file missing at ${REGISTRY_PATH}`);
    process.exit(1);
  }

  let registry;
  try {
    registry = JSON.parse(readFileSync(REGISTRY_PATH, "utf8"));
  } catch (err) {
    console.error(`FATAL: Failed to parse ${REGISTRY_PATH}:`, err.message);
    process.exit(1);
  }

  let allViolations = [];
  let checkedCount = 0;
  const allCss = listCss(join(ROOT, "frontend", "src"));
  const cssTexts = allCss.map((f) => readFileSync(f, "utf8"));
  const tokens = collectTokens(cssTexts);
  allCss.forEach((f, i) => {
    allViolations.push(...blurOutsideStage(relative(ROOT, f), cssTexts[i]));
  });

  for (const [relFile, selectors] of Object.entries(registry)) {
    const absPath = join(ROOT, relFile);
    if (!existsSync(absPath)) {
      allViolations.push(`${relFile}: file does not exist on disk (registry rotted)`);
      continue;
    }

    const content = readFileSync(absPath, "utf8");
    const violations = checkCssContent(relFile, content, selectors, tokens);
    if (violations.length > 0) {
      allViolations.push(...violations);
    }
    checkedCount++;
  }

  if (allViolations.length > 0) {
    console.error(`\nFAILED: Found ${allViolations.length} window opacity violation(s):`);
    for (const v of allViolations) {
      console.error(`  ${v}`);
    }
    process.exit(1);
  }

  console.log(
    `PASS: Verified ${checkedCount} registered CSS files (tokens resolved across ${allCss.length} files, no blur outside the stage; 0 violations).`
  );
  process.exit(0);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
