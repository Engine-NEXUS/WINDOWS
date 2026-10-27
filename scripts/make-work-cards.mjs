#!/usr/bin/env node
// Generates one production work card per in-scope feature from docs/testing/scorecard-data.json,
// in queue order (ascending raw score), plus docs/testing/work-cards/README.md and
// docs/testing/feature-process-map.md. Run: node scripts/make-work-cards.mjs [--check]
// --check exits 1 if the committed files differ from what would be generated.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dataPath = path.join(root, "docs/testing/scorecard-data.json");
const outDir = path.join(root, "docs/testing/work-cards");
const mapPath = path.join(root, "docs/testing/feature-process-map.md");
const check = process.argv.includes("--check");

const WEIGHTS = { B: 0.2, T: 0.2, L: 0.3, S: 0.15, I: 0.15 };
const GATES = { B: 90, T: 70, L: 70, S: 80, I: 60 };
const RAW_GATE = 90;
const OUT_OF_TARGET = /Streaming partial captions|Admin brain|Administrator control|macOS \/ Linux parity/i;

// What each axis gap means in practice and how to close it. Wording is deliberately generic;
// the per-feature note from the scorecard carries the specifics.
const CLOSE = {
  B: "Wire it end to end in the shipping path (no dead code, reachable by the user, IPC registered, setting kept in `NexusSettings`).",
  T: "Add focused tests for the unproven paths; mutation-check each safety rule (break it, a test must fail); keep fixtures independent of the author's own phrasing where the feature is voice-facing.",
  L: "Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.",
  S: "List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.",
  I: "Measure against a stated reference (published number or the previous build) on a held-out set; record the measurement and its uncertainty, not an estimate.",
};

// Runtime processes per feature (keyword -> processes). `focus` says what Focus Mode
// (feature 161/163) does to the feature's background work while Hand Mode is on.
const PROCESS_MAP = [
  [/wake word/i, "cpal audio stream thread, silence-recovery thread, openWakeWord classifier (muted by solo Hand Mode)", "keeps running"],
  [/speaker verification/i, "wake receiver thread, enrollment pipeline (Setup/Hub), keychain key", "keeps running"],
  [/cloud stt|local stt/i, "stt-capture-rx thread, Groq HTTP client; Moonshine Python sidecar (port 39217, lazy, killed after 60 s idle)", "sidecar is lazy; diagnostics sweep paused"],
  [/tts online|tts offline|voice personas|captions|tts volume/i, "Edge TTS connection, Kokoro ONNX (lazy), rodio playback, volume lease watchdog", "cache refresh, voice sync and swap downloads deferred"],
  [/hand mode/i, "hand WebView2 renderer (camera + MediaPipe), hand-out thread, 80 ms watchdog, GPU shared with the stage WebView2", "owner of Focus Mode"],
  [/orb|stage|window theming/i, "stage WebView2 (fullscreen overlay), hitbox thread (30-250 ms), stage watchdog (2 s), WebGL orb loop", "never paused (a starved heartbeat destroys the stage)"],
  [/ghost/i, "ghost session state, hot-mic loop, drill runner, Esc shortcut", "keeps running"],
  [/dictation|shortcut|open \/ close|window tiling|keep-awake|ask-first shutdown/i, "foreground/UI Automation calls, SendInput, keep-awake poller (3 s)", "keep-awake never paused"],
  [/mail|calendar|timetable|leave-by|whatsapp|briefing|overnight|snapshot|resume|activity manager|memory core|profile, corrections/i, "memcore pollers (30-60 s ticks), session snapshot (60 s), resume sampler (30 s), activity sampler (2 s), sealed SQLite store", "snapshot, resume, activity, curator, review, routines paused; mail/WhatsApp/leave-by keep polling and their speech is held"],
  [/proactive/i, "proactive_policy engine + 2 s ticker", "Medium alerts held, not dropped"],
  [/agent|watch one|gated commit|track all/i, "agent hooks webhook (port 39220), agent_watch Node sidecar (39222, lazy), watch_runner (3 s)", "keeps running"],
  [/github|architecture mapper|reverse engineer|screen analysis|vision/i, "GitHub REST/Worker calls, repo index cache (10 min), Gemini/Groq vision calls", "on demand"],
  [/google|oauth|mcp|device links|security|worker/i, "OAuth loopback listener, vault monitor (90 s), Worker HTTPS", "vault monitor paused"],
  [/nlu|intent|compound|centre|9router|mishearing/i, "regex tier in-process; BERT-Mini NLU sidecar (39218, lazy, 60 s idle); 9Router HTTP", "on demand"],
  [/directed|endpointing/i, "capture loop, VAD, turn-detect ONNX", "keeps running"],
  [/resource footprint|ci gates|installer|command hub/i, "build scripts / Hub WebView2", "n/a"],
  [/music|maps/i, "media keys / deep links / Routes API", "on demand"],
  [/wake-up sound/i, "on-demand rodio stream (closed 20 s after use), hotkey/wake callbacks", "keeps running"],
  [/action kernel/i, "action_kernel (paths, policy, journal, audit JSONL), quarantine folders on disk", "on demand"],
  [/global hotkey/i, "global shortcut handler, wake/cancel decision on the capture thread", "keeps running"],
];

function slug(s) {
  return s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60);
}

function raw(f) {
  return Object.entries(WEIGHTS).reduce((a, [k, w]) => a + w * f[k], 0);
}

const features = JSON.parse(fs.readFileSync(dataPath, "utf8"))
  .filter((f) => !OUT_OF_TARGET.test(f.feature))
  .map((f) => ({ ...f, raw: raw(f) }))
  .sort((a, b) => a.raw - b.raw || a.feature.localeCompare(b.feature));

const procFor = (name) => PROCESS_MAP.find(([re]) => re.test(name)) ?? [null, "not mapped yet — add it to PROCESS_MAP in scripts/make-work-cards.mjs", "unknown"];

function card(f, rank) {
  const gaps = Object.keys(GATES).filter((k) => f[k] < GATES[k]);
  const [, procs, focus] = procFor(f.feature);
  const lines = [];
  lines.push(`# Work card ${String(rank).padStart(2, "0")} — ${f.feature}`);
  lines.push("");
  lines.push(`Generated from \`docs/testing/scorecard-data.json\` by \`scripts/make-work-cards.mjs\`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.`);
  lines.push("");
  lines.push(`Area: ${f.area}. Queue position ${rank} of ${features.length}. Raw score **${f.raw.toFixed(1)}%** (production needs ≥ ${RAW_GATE}% and every axis gate).`);
  lines.push("");
  lines.push("| Axis | Now | Gate | Status |");
  lines.push("|---|---:|---:|---|");
  for (const k of Object.keys(GATES)) {
    lines.push(`| ${k} | ${f[k]} | ${GATES[k]} | ${f[k] >= GATES[k] ? "met" : `**gap ${GATES[k] - f[k]}**`} |`);
  }
  lines.push("");
  lines.push(`Evidence today: ${f.tests || "none recorded"}.`);
  lines.push("");
  lines.push("## Known gap (from the scorecard)");
  lines.push("");
  lines.push(f.note || "No note recorded.");
  lines.push("");
  lines.push("## Runs as (processes, for contention and Focus Mode)");
  lines.push("");
  lines.push(`- Processes/threads: ${procs}`);
  lines.push(`- Under Hand Mode Focus Mode: ${focus}`);
  lines.push("");
  lines.push("## How to close each gap");
  lines.push("");
  if (gaps.length === 0) {
    lines.push("All axis gates are met; remaining work is the shared release gates and a fresh live confirmation.");
  } else {
    for (const k of gaps) lines.push(`- **${k}** (${f[k]} → ${GATES[k]}): ${CLOSE[k]}`);
  }
  lines.push("");
  lines.push("## Shared gates (must pass on the assessed revision)");
  lines.push("");
  lines.push("- `cargo check --lib` clean and `cargo test --lib -- --test-threads=1`; frontend `tsc --noEmit`, `vitest run`, `npm run build`; Worker tests if touched.");
  lines.push("- `node scripts/check-turn-ends.mjs`, `check-repo-hygiene.mjs`, `check-ipc-contract.mjs`, `dead-code-audit.mjs`, `check-window-opacity.mjs`.");
  lines.push("- Fresh release with provenance (`node scripts/build-provenance.mjs verify`) before any live claim.");
  lines.push("");
  lines.push("## Live script (owner)");
  lines.push("");
  lines.push("1. Write the 5–10 exact spoken or clicked steps that prove the feature, including one failure case (offline, wrong window, denied permission) and the expected honest message.");
  lines.push("2. Run them on the fresh release; save the console log to `docs/testing/runs/<date>-<slug>.log`.");
  lines.push("3. Record pass/fail per step, sample sizes where it is a rate, and the build identity in `docs/testing/live-proof-ledger.md`.");
  lines.push("");
  lines.push("## Done when");
  lines.push("");
  lines.push("- [ ] Every axis gate above is met with evidence, not estimates.");
  lines.push("- [ ] Safety rules each have a mutation that fails a test.");
  lines.push("- [ ] A live-proof-ledger line exists for the assessed build.");
  lines.push("- [ ] `scorecard-data.json` updated, cards regenerated, queue order rechecked.");
  lines.push("");
  return lines.join("\n");
}

const outputs = new Map();
features.forEach((f, i) => {
  const rank = i + 1;
  outputs.set(path.join(outDir, `${String(rank).padStart(2, "0")}-${slug(f.feature)}.md`), card(f, rank));
});

const index = [
  "# Feature work cards",
  "",
  "One card per in-scope feature, in queue order (lowest raw score first). Generated by `node scripts/make-work-cards.mjs` from `docs/testing/scorecard-data.json`; the programme rules (active card first, 90% gate, real-device proof) are in [the production queue](../windows1-production-queue.md). See also the [feature process map](../feature-process-map.md).",
  "",
  "| # | Feature | Area | Raw | Gaps |",
  "|---:|---|---|---:|---|",
  ...features.map((f, i) => {
    const gaps = Object.keys(GATES).filter((k) => f[k] < GATES[k]).join(" ") || "none";
    const file = `${String(i + 1).padStart(2, "0")}-${slug(f.feature)}.md`;
    return `| ${i + 1} | [${f.feature.replace(/\|/g, "/")}](${file}) | ${f.area} | ${f.raw.toFixed(1)}% | ${gaps} |`;
  }),
  "",
].join("\n");
outputs.set(path.join(outDir, "README.md"), index);

const map = [
  "# Feature process map",
  "",
  "Which runtime processes and threads each feature depends on, and what Focus Mode (Hand Mode, records 161 and 163) does to them. Generated by `node scripts/make-work-cards.mjs`; extend `PROCESS_MAP` in that script when a feature is unmapped.",
  "",
  "| Feature | Processes / threads | Under Hand Mode Focus Mode |",
  "|---|---|---|",
  ...features.map((f) => {
    const [, p, focus] = procFor(f.feature);
    return `| ${f.feature.replace(/\|/g, "/")} | ${p} | ${focus} |`;
  }),
  "",
].join("\n");
outputs.set(mapPath, map);

let drift = false;
if (!check) fs.mkdirSync(outDir, { recursive: true });
for (const [file, text] of outputs) {
  const body = text.endsWith("\n") ? text : text + "\n";
  if (check) {
    if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== body) {
      console.error(`work cards out of date: ${path.relative(root, file)}`);
      drift = true;
    }
  } else {
    fs.writeFileSync(file, body);
  }
}
if (check) process.exit(drift ? 1 : 0);
console.log(`wrote ${outputs.size} files (${features.length} cards) under docs/testing`);
