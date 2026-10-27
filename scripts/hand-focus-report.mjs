#!/usr/bin/env node
// Compares Hand Mode metrics with and without Focus Mode from a NEXUS console log.
// Usage: node scripts/hand-focus-report.mjs <log-file> [more logs]   |   --selftest
// A [HAND] line counts as "focus on" between "[FOCUS] on" and "[FOCUS] off", else "focus off".
// Record one session with handFocusMode off and one with it on (same load), then run this.
import fs from "node:fs";

const NUM = (s, re) => {
  const m = re.exec(s);
  return m ? Number(m[1]) : null;
};

export function parse(text) {
  const rows = { on: [], off: [] };
  let focus = false;
  for (const line of text.split(/\r?\n/)) {
    if (line.includes("[FOCUS] on")) focus = true;
    else if (line.includes("[FOCUS] off")) focus = false;
    else if (line.includes("[HAND] fps=")) {
      rows[focus ? "on" : "off"].push({
        fps: NUM(line, /fps=([\d.]+)/),
        rustP50: NUM(line, /capture_to_rust_p50=([\d.]+)ms/),
        inferP50: NUM(line, /infer_p50=([\d.]+)ms/),
        gpuBusy: /gpu_busy=true/.test(line) ? 1 : 0,
      });
    } else if (line.includes("[HAND] cam:")) {
      const tail = focus ? rows.on : rows.off;
      tail.push({
        busySkips: NUM(line, /busy_skips=(\d+)/),
        landmarksP95: NUM(line, /capture_to_landmarks p50=[\d.]+ms p95=([\d.]+)ms/),
        camFps: NUM(line, /cam_fps\(track\)=([\d.]+)/),
      });
    }
  }
  return rows;
}

const median = (xs) => {
  const v = xs.filter((x) => x != null).sort((a, b) => a - b);
  if (!v.length) return null;
  const m = v.length >> 1;
  return v.length % 2 ? v[m] : (v[m - 1] + v[m]) / 2;
};

export function summarize(rows) {
  const pick = (k) => median(rows.map((r) => r[k]));
  const gpu = rows.filter((r) => r.gpuBusy != null);
  return {
    samples: rows.length,
    fps: pick("fps"),
    captureToRustP50: pick("rustP50"),
    inferP50: pick("inferP50"),
    busySkips: pick("busySkips"),
    captureToLandmarksP95: pick("landmarksP95"),
    camFps: pick("camFps"),
    gpuBusyShare: gpu.length ? gpu.reduce((a, r) => a + r.gpuBusy, 0) / gpu.length : null,
  };
}

const fmt = (x) => (x == null ? "-" : Number.isInteger(x) ? String(x) : x.toFixed(1));

export function report(text) {
  const { on, off } = parse(text);
  const a = summarize(off);
  const b = summarize(on);
  const lowerIsBetter = new Set(["captureToRustP50", "inferP50", "busySkips", "captureToLandmarksP95", "gpuBusyShare"]);
  const lines = ["metric                  focus off   focus on   verdict"];
  for (const k of Object.keys(a)) {
    if (k === "samples") {
      lines.push(`${k.padEnd(22)}  ${fmt(a[k]).padEnd(10)}  ${fmt(b[k])}`);
      continue;
    }
    let verdict = "";
    if (a[k] != null && b[k] != null && a[k] !== b[k]) {
      const better = lowerIsBetter.has(k) ? b[k] < a[k] : b[k] > a[k];
      verdict = better ? "better" : "WORSE";
    }
    lines.push(`${k.padEnd(22)}  ${fmt(a[k]).padEnd(10)}  ${fmt(b[k]).padEnd(9)}  ${verdict}`);
  }
  if (!a.samples || !b.samples) lines.push("", "Need [HAND] lines both with and without Focus Mode: record two sessions.");
  else lines.push("", "Keep Focus Mode only if captureToLandmarksP95 and busySkips improve under the same load.");
  return lines.join("\n");
}

function selftest() {
  const log = [
    "[HAND] fps=14.0 hand=90% capture_to_rust_p50=60ms infer_p50=40ms lead=20ms armed=true live=true clicks=0 drags=0 scrolls=0 gpu_busy=true",
    "[HAND] cam: source=track cam_fps(track)=15.0 cam_fps(rvfc)=15.0 proc_fps=14.0 driver_fps=30 busy_skips=9 dropped=0 capture_to_landmarks p50=50ms p95=120ms (x) page=1 exposure=1 power=AC covered=false",
    "[FOCUS] on",
    "[HAND] fps=22.0 hand=90% capture_to_rust_p50=30ms infer_p50=25ms lead=20ms armed=true live=true clicks=0 drags=0 scrolls=0 gpu_busy=false",
    "[HAND] cam: source=track cam_fps(track)=15.0 cam_fps(rvfc)=15.0 proc_fps=22.0 driver_fps=30 busy_skips=2 dropped=0 capture_to_landmarks p50=30ms p95=60ms (x) page=1 exposure=1 power=AC covered=false",
    "[FOCUS] off",
  ].join("\n");
  const { on, off } = parse(log);
  const a = summarize(off);
  const b = summarize(on);
  const ok =
    off.length === 2 && on.length === 2 && a.fps === 14 && b.fps === 22 &&
    a.busySkips === 9 && b.busySkips === 2 && a.captureToLandmarksP95 === 120 &&
    b.captureToLandmarksP95 === 60 && a.gpuBusyShare === 1 && b.gpuBusyShare === 0 &&
    report(log).includes("better");
  if (!ok) {
    console.error("selftest FAILED", { a, b });
    process.exit(1);
  }
  console.log("hand-focus-report selftest OK");
}

if (process.argv.includes("--selftest")) selftest();
else if (import.meta.url === `file:///${process.argv[1].replace(/\\/g, "/")}` || process.argv[1]?.endsWith("hand-focus-report.mjs")) {
  const files = process.argv.slice(2).filter((a) => !a.startsWith("--"));
  if (!files.length) {
    console.error("usage: node scripts/hand-focus-report.mjs <log> [more logs] | --selftest");
    process.exit(2);
  }
  console.log(report(files.map((f) => fs.readFileSync(f, "utf8")).join("\n")));
}
