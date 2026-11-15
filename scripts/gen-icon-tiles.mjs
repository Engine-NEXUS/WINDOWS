// Picks the tile each bundled icon sits on so the mark is visible: the brand colour when the mark contrasts with it
// (white marks), else a light tile, else a dark one. Writes frontend/src/settings-sidebar/hub/activityIconTiles.json.
//   node scripts/gen-icon-tiles.mjs          write      node scripts/gen-icon-tiles.mjs --check   fail on drift
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(root, "frontend", "public", "activity-logos");
const brand = JSON.parse(readFileSync(join(root, "frontend/src/settings-sidebar/hub/activityIconColors.json"), "utf8"));
const out = join(root, "frontend/src/settings-sidebar/hub/activityIconTiles.json");
const LIGHT = "#f4f4f6", DARK = "#1c1c21";
const full = (h) => (h.length === 4 ? "#" + [...h.slice(1)].map((c) => c + c).join("") : h).toLowerCase();
const lum = (h) => {
  const f = (v) => { const c = v / 255; return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
  return 0.2126 * f(parseInt(h.slice(1, 3), 16)) + 0.7152 * f(parseInt(h.slice(3, 5), 16)) + 0.0722 * f(parseInt(h.slice(5, 7), 16));
};
const ratio = (a, b) => { const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x); return (hi + 0.05) / (lo + 0.05); };
const NAMED = { white: "#ffffff", black: "#000000" };
// The colours an SVG paints with. A gradient counts as a mid-tone colourful paint; `currentColor` and no paint at all are black in an <img>.
const paints = (svg) => {
  const p = [];
  for (const m of svg.matchAll(/\b(?:fill|stroke)\s*[=:]\s*["']?\s*(#[0-9a-fA-F]{6}|#[0-9a-fA-F]{3}|currentColor|white|black|url\(#[^)]*\))/g)) {
    const v = m[1];
    p.push(v === "currentColor" ? "#000000" : NAMED[v] ?? (v.startsWith("url(") ? "#5a7fe0" : full(v)));
  }
  if (!p.length) p.push("#000000");
  return p;
};
const best = (p, tile) => Math.max(...p.map((c) => ratio(c, tile)));
const tiles = {};
for (const f of readdirSync(dir).filter((n) => n.endsWith(".svg")).sort()) {
  const id = f.slice(0, -4);
  const p = paints(readFileSync(join(dir, f), "utf8"));
  if (!p.length) continue;
  const b = brand[id] && /^#[0-9a-f]{6}$/i.test(brand[id]) ? full(brand[id]) : null;
  tiles[id] = b && best(p, b) >= 3 ? b : best(p, LIGHT) >= 1.8 ? LIGHT : DARK;
}
const text = JSON.stringify(tiles, null, 2) + "\n";
if (process.argv.includes("--check")) {
  let cur = ""; try { cur = readFileSync(out, "utf8").replace(/\r\n/g, "\n"); } catch {}
  if (cur !== text) { console.error("activityIconTiles.json is out of date: run node scripts/gen-icon-tiles.mjs"); process.exit(1); }
  console.log("icon tiles up to date:", Object.keys(tiles).length);
} else { writeFileSync(out, text); console.log("wrote", Object.keys(tiles).length, "tiles"); }
