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

import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
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
  const clean = stripComments(css);
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

/** Escape special regex characters in a selector string */
function escapeRegex(str) {
  return str.replace(/[-[\]{}()*+?.,\\^$|#\s]/g, "\\$&");
}

/** Check CSS content for a set of registered surface selectors */
export function checkCssContent(filePath, cssContent, registeredSelectors) {
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
              const translucentReason = checkTranslucentColor(val);
              if (translucentReason) {
                violations.push(
                  `${filePath} ${sel}: translucent ${prop} (${translucentReason}) in "${val}"`
                );
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

/** Built-in selftest fixtures */
export function runSelfTest() {
  console.log("[check-window-opacity] Running selftest fixtures...");

  const passingCss = `
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

  console.log("PASS: Selftest passed (all 6 fixtures classified correctly).");
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

  for (const [relFile, selectors] of Object.entries(registry)) {
    const absPath = join(ROOT, relFile);
    if (!existsSync(absPath)) {
      allViolations.push(`${relFile}: file does not exist on disk (registry rotted)`);
      continue;
    }

    const content = readFileSync(absPath, "utf8");
    const violations = checkCssContent(relFile, content, selectors);
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
    `PASS: Verified ${checkedCount} CSS files in opacity registry (0 violations, all window surfaces solid).`
  );
  process.exit(0);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
