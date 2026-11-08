// Cross-check of the agent logos against the files that actually ship: for each agent, the colours its SVG paints
// with must show up on the tile the island puts it on. (A white mark on a white tile, or a near-black mark on a dark
// tile, is invisible - the Antigravity and Cursor bug.)
import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { AGENTS_WITH_LOGOS, agentLook } from "./agentIslandModel";

const PUBLIC = resolve(__dirname, "../../public");

type Rgb = [number, number, number];

function parseHex(h: string): Rgb {
  let x = h.replace("#", "");
  if (x.length === 3) x = x.split("").map((c) => c + c).join("");
  return [parseInt(x.slice(0, 2), 16), parseInt(x.slice(2, 4), 16), parseInt(x.slice(4, 6), 16)];
}

function luminance([r, g, b]: Rgb): number {
  const f = (v: number) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

function saturation([r, g, b]: Rgb): number {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  return max === 0 ? 0 : (max - min) / max;
}

/** The solid colours an SVG paints with. `currentColor` inside an <img> is black. Gradients are skipped. */
function paintColours(svg: string): Rgb[] {
  const out: Rgb[] = [];
  for (const m of svg.matchAll(/\bfill\s*[=:]\s*["']?\s*(#[0-9a-fA-F]{3,6}|currentColor)\b/g)) {
    out.push(m[1] === "currentColor" ? [0, 0, 0] : parseHex(m[1]));
  }
  if (out.length === 0 && /\bfill\s*=\s*["']currentColor["']/.test(svg)) out.push([0, 0, 0]);
  return out;
}

/** Secondary details that are meant to be subtle (OpenCode's inner square). */
const SUBTLE_OK: Record<string, string[]> = { opencode: ["#cfcecd"] };

describe("agent logos", () => {
  it("covers every agent NEXUS can hook up", () => {
    for (const a of ["claude", "codex", "antigravity", "opencode", "gemini", "cursor", "aider", "kilo", "copilot", "windsurf", "devin", "trae"]) {
      expect(agentLook(a), a).not.toBeNull();
    }
    expect(agentLook("Claude")?.src).toBe("/activity-logos/claude.svg");
    expect(agentLook("nobody")).toBeNull();
  });

  for (const agent of AGENTS_WITH_LOGOS) {
    it(`${agent}: the file exists and its mark is visible on its tile`, () => {
      const look = agentLook(agent)!;
      const file = resolve(PUBLIC, look.src.replace(/^\//, ""));
      expect(existsSync(file), `${look.src} is missing`).toBe(true);
      const svg = readFileSync(file, "utf8");
      expect(svg.includes("<svg")).toBe(true);
      expect(/<script|onload=|onclick=|javascript:/i.test(svg)).toBe(false);
      const tile = parseHex(look.tile);
      const colours = paintColours(svg);
      const hasGradient = /fill\s*=\s*["']url\(/.test(svg);
      expect(colours.length > 0 || hasGradient, `${agent}: no fill colours found`).toBe(true);
      if (agent === "aider") return; // it paints its own tile
      // at least one colour of the mark stands clear of the tile...
      if (colours.length > 0 && !hasGradient) {
        expect(Math.max(...colours.map((c) => contrast(c, tile))), `${agent} is invisible on its tile`).toBeGreaterThanOrEqual(3);
      }
      // ...and no neutral (white / black / grey) part of it vanishes into the tile
      const allowed = (SUBTLE_OK[agent] ?? []).map(parseHex);
      for (const c of colours) {
        if (saturation(c) > 0.25) continue;
        if (allowed.some((a) => a.join() === c.join())) continue;
        expect(contrast(c, tile), `${agent}: rgb(${c.join(",")}) disappears on ${look.tile}`).toBeGreaterThanOrEqual(3);
      }
    });
  }

  it("would have caught the original bug (white mark on a white tile, dark on dark)", () => {
    const white: Rgb = [255, 255, 255];
    const nearBlack: Rgb = parseHex("#26251e");
    expect(contrast(white, parseHex("#ffffff"))).toBeLessThan(3);
    expect(contrast(nearBlack, parseHex("#262626"))).toBeLessThan(3);
  });
});
