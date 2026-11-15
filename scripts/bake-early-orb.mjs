#!/usr/bin/env node
// Bakes the real frames of the NEW dotted orb for the early hello (src-tauri/src/early_orb.rs).
//
// The greeting at logon is played and drawn natively, before the app's own window (which owns the dotted orb) exists.
// Instead of re-implementing the orb engine in Rust, this runs the real engine (frontend/src/avatar/dotted) once and stores
// what it draws: the idle circle morphing (650 ms, exactly as the stage does it) into the speaking design (composing).
//
//   node scripts/bake-early-orb.mjs           write src-tauri/resources/early_orb_frames.bin
//   node scripts/bake-early-orb.mjs --check   fail (exit 1) when the committed file differs from a fresh bake
//
// File layout (little endian), see `BakedOrb::parse` in early_orb.rs:
//   "EOB1" | fps u16 | frames u16 | size u16 (the pixel size the engine was asked for)
//   per frame: count u16, then per dot: x u16, y u16, r u8, white u8, alpha u8
//   x, y are (value / size + 0.25) * 32767 ; r is r / size * 1000 (the engine clamps r far below 0.255 of the size).
import { createRequire } from "node:module";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outPath = join(root, "src-tauri", "resources", "early_orb_frames.bin");
const require = createRequire(join(root, "frontend", "package.json"));
const esbuild = require("esbuild");

const SIZE = 105; // the pebble capsule is 105 px high: the orb fills its cradle (Avatar.tsx measures min(width, height))
const FPS = 30;
const SECONDS = 6;
const FRAMES = FPS * SECONDS;

const dottedDir = join(root, "frontend", "src", "avatar", "dotted").replace(/\\/g, "/");

const entry = `
import { resolveOrb } from "${dottedDir}/resolveOrb";
import { blendFrames, MORPH_MS } from "${dottedDir}/morph";
export function bake(size, fps, frames) {
  const circle = resolveOrb("circle", size);
  const speak = resolveOrb("composing", size);
  const from = circle.frame(size, 1.0, circle.opts); // the idle circle the morph starts from (a still, as in DottedOrb)
  const out = [];
  for (let i = 0; i < frames; i++) {
    const tSec = i / fps;
    let frame = speak.frame(size, tSec * speak.speed, speak.opts);
    const k = (tSec * 1000) / MORPH_MS;
    if (k < 1) frame = blendFrames(from, frame, k, size / 2, size / 2);
    out.push(frame.dots.map((d) => ({ x: d.x, y: d.y, r: d.r, white: d.white, a: d.a ?? 1 })));
  }
  return out;
}
`;

const built = await esbuild.build({
  stdin: { contents: entry, resolveDir: root, loader: "ts" },
  bundle: true,
  format: "esm",
  platform: "node",
  write: false,
  logLevel: "silent",
});
const code = built.outputFiles[0].text;
const mod = await import("data:text/javascript;base64," + Buffer.from(code).toString("base64"));
const frames = mod.bake(SIZE, FPS, FRAMES);

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const chunks = [];
const head = Buffer.alloc(4 + 2 + 2 + 2);
head.write("EOB1", 0, "ascii");
head.writeUInt16LE(FPS, 4);
head.writeUInt16LE(FRAMES, 6);
head.writeUInt16LE(SIZE, 8);
chunks.push(head);
for (const dots of frames) {
  const b = Buffer.alloc(2 + dots.length * 7);
  b.writeUInt16LE(dots.length, 0);
  let o = 2;
  for (const d of dots) {
    b.writeUInt16LE(clamp(Math.round((d.x / SIZE + 0.25) * 32767), 0, 65535), o);
    b.writeUInt16LE(clamp(Math.round((d.y / SIZE + 0.25) * 32767), 0, 65535), o + 2);
    b.writeUInt8(clamp(Math.round((d.r / SIZE) * 1000), 0, 255), o + 4);
    b.writeUInt8(clamp(Math.round(d.white * 255), 0, 255), o + 5);
    b.writeUInt8(clamp(Math.round(d.a * 255), 0, 255), o + 6);
    o += 7;
  }
  chunks.push(b);
}
const bytes = Buffer.concat(chunks);

if (process.argv.includes("--check")) {
  if (!existsSync(outPath) || !readFileSync(outPath).equals(bytes)) {
    console.error("early orb frames are out of date: run `node scripts/bake-early-orb.mjs` and commit src-tauri/resources/early_orb_frames.bin");
    process.exit(1);
  }
  console.log(`early orb frames OK (${FRAMES} frames, ${bytes.length} bytes)`);
} else {
  writeFileSync(outPath, bytes);
  const counts = frames.map((f) => f.length);
  console.log(`baked ${FRAMES} frames (${Math.min(...counts)}-${Math.max(...counts)} dots) -> ${outPath} (${bytes.length} bytes)`);
}
