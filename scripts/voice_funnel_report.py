#!/usr/bin/env python3
"""Voice funnel report: where do spoken turns get lost? Reads the NEXUS log (real usage), prints measured counts only.

    python scripts/voice_funnel_report.py [--log PATH ...] [--since 2026-11-13T00:00:00] [--json]

Stages, in the order a turn travels:
  1. microphone      LIVE vs SILENT readings, share of time the driver delivered digital silence
  2. wake candidate  stage-1 neural detections (the model alone), by score
  3. speech check    the second stage: accepted ("firing wake") vs rejected, with what the recogniser heard
  4. capture         command captures started, delivered / cancelled / no speech
  5. recognition     transcripts handed on, hallucinations filtered, empty results

Nothing here is a guess: a stage with no lines in the log shows 0, and rates need a denominator that is printed next to them.
It never reads audio. Standard library only.
"""

import argparse
import json
import os
import re
import sys
from collections import Counter

PATS = {
    "mic_live": re.compile(r"audio: mic .*LIVE"),
    "mic_silent": re.compile(r"audio: mic .*SILENT"),
    "mic_silent_warn": re.compile(r"mic has been silent for"),
    "candidate": re.compile(r"stage-1 candidate \(prob ([0-9.]+)\)"),
    "rejected": re.compile(r"transcript '(.*?)' rejected \(stage-1 prob ([0-9.]+)\)"),
    "confirmed": re.compile(r"STT confirmed '(.*?)' \(stage-1 prob ([0-9.]+)"),
    "fired": re.compile(r"wake: firing orb \(source=(\w+)\)"),
    "capture_started": re.compile(r"stt-capture: started"),
    "capture_terminal": re.compile(r'"event":"terminal","outcome":"(\w+)"'),
    "capture_aborted": re.compile(r"stt-capture: aborted \(was_capturing=(\w+), had_speech=(\w+), elapsed=(\d+)ms\)"),
    "transcript": re.compile(r"stt: groq transcript: '(.*)'"),
    "halluc": re.compile(r"stt: filtered (?:repetition loop )?hallucination '(.*?)' ->"),
    "turn": re.compile(r"turn session=(\d+) rms_max=([0-9.]+) voiced=(\d+)/(\d+) endpoint=(\w+) path=(\w+) filter=(\w+)"),
    "hotkey_cancel": re.compile(r"hotkey .* CANCELING"),
    "model_prob": re.compile(r"wake: model probability=([0-9.]+)"),
}

# The wake threshold the neural stage fires at (settings wakeSensitivity medium = 0.68) and the lowest score counted as a near miss.
FIRE_AT = 0.68
NEAR_MISS_FROM = 0.35
CLUSTER_GAP_S = 1.5


def _stamp(line):
    m = re.match(r"(\d{4}-\d\d-\d\dT[\d:.]+Z)", line)
    return m.group(1) if m else None


def _seconds(ts):
    """Seconds since midnight from 2026-11-13T10:00:00.123456Z (the date is irrelevant inside a 1.5 s gap)."""
    m = re.match(r"\d{4}-\d\d-\d\dT(\d\d):(\d\d):([\d.]+)Z", ts or "")
    return None if not m else int(m.group(1)) * 3600 + int(m.group(2)) * 60 + float(m.group(3))


def near_misses(scored):
    """Speech-like moments the neural stage scored but did not fire on. `scored` = [(seconds, probability)] in log order.
    A burst of scores less than 1.5 s apart is one moment; it is a near miss when its peak is in [0.35, 0.68)."""
    clusters, cur = [], []
    for t, p in scored:
        if cur and t - cur[-1][0] > CLUSTER_GAP_S:
            clusters.append(cur)
            cur = []
        cur.append((t, p))
    if cur:
        clusters.append(cur)
    peaks = [max(p for _, p in c) for c in clusters]
    return [p for p in peaks if NEAR_MISS_FROM <= p < FIRE_AT]


def analyse(lines, since=None):
    """Counts per stage. `since` is an ISO timestamp prefix; older lines are skipped."""
    out = {k: 0 for k in ("mic_live", "mic_silent", "mic_silent_warn", "candidates", "rejected", "confirmed", "capture_started",
                          "capture_no_speech", "hotkey_cancel", "transcripts", "hallucinations", "turns")}
    scored = []
    out.update(reject_empty=0, reject_text=Counter(), confirmed_text=Counter(), fires=Counter(), capture_outcomes=Counter(),
               cand_prob=[], reject_prob=[], confirm_prob=[], halluc_text=Counter(), turn_rows=[])
    for line in lines:
        ts = _stamp(line)
        if since and ts and ts < since:
            continue
        if PATS["mic_live"].search(line):
            out["mic_live"] += 1
        elif PATS["mic_silent"].search(line):
            out["mic_silent"] += 1
        if PATS["mic_silent_warn"].search(line):
            out["mic_silent_warn"] += 1
        m = PATS["candidate"].search(line)
        if m:
            out["candidates"] += 1
            out["cand_prob"].append(float(m.group(1)))
        m = PATS["rejected"].search(line)
        if m:
            out["rejected"] += 1
            out["reject_prob"].append(float(m.group(2)))
            if m.group(1).strip() == "":
                out["reject_empty"] += 1
            else:
                out["reject_text"][m.group(1)] += 1
        m = PATS["confirmed"].search(line)
        if m:
            out["confirmed"] += 1
            out["confirm_prob"].append(float(m.group(2)))
            out["confirmed_text"][m.group(1)] += 1
        m = PATS["fired"].search(line)
        if m:
            out["fires"][m.group(1)] += 1
        if PATS["capture_started"].search(line):
            out["capture_started"] += 1
        m = PATS["capture_terminal"].search(line)
        if m:
            out["capture_outcomes"][m.group(1)] += 1
        m = PATS["capture_aborted"].search(line)
        if m and m.group(2) == "false":
            out["capture_no_speech"] += 1
        if PATS["hotkey_cancel"].search(line):
            out["hotkey_cancel"] += 1
        m = PATS["transcript"].search(line)
        if m:
            out["transcripts"] += 1
        m = PATS["halluc"].search(line)
        if m:
            out["hallucinations"] += 1
            out["halluc_text"][m.group(1)] += 1
        m = PATS["model_prob"].search(line)
        if m and _seconds(ts) is not None:
            scored.append((_seconds(ts), float(m.group(1))))
        m = PATS["turn"].search(line)
        if m:
            out["turns"] += 1
            out["turn_rows"].append(dict(session=int(m.group(1)), rms_max=float(m.group(2)), voiced=int(m.group(3)), frames=int(m.group(4)),
                                         endpoint=m.group(5), path=m.group(6), filter=m.group(7)))
    out["near_miss_peaks"] = near_misses(scored)
    out["model_moments"] = len([1 for _ in scored])
    return out


def pct(n, d):
    return "n/a" if d == 0 else f"{100.0 * n / d:.1f}% ({n}/{d})"


def render(a):
    mic_total = a["mic_live"] + a["mic_silent"]
    gate = a["candidates"]
    lines = ["# Voice funnel (measured from the log)", ""]
    lines += ["## 1. Microphone", f"- Readings that were LIVE: {pct(a['mic_live'], mic_total)}; SILENT: {pct(a['mic_silent'], mic_total)}",
              f"- 'Mic has been silent' warnings: {a['mic_silent_warn']}", ""]
    lines += ["## 2. Wake candidates (neural model alone)", f"- Stage-1 detections: {a['candidates']}"]
    if a["cand_prob"]:
        p = sorted(a["cand_prob"])
        lines.append(f"- Score range {p[0]:.2f} to {p[-1]:.2f}, median {p[len(p) // 2]:.2f}")
    nm = a["near_miss_peaks"]
    lines += [f"- Near misses (the model heard something wake-like, peak {NEAR_MISS_FROM:.2f} to {FIRE_AT:.2f}, no fire): {len(nm)}"]
    if nm:
        lines.append(f"    peaks: {', '.join(f'{p:.2f}' for p in sorted(nm, reverse=True)[:12])}")
    lines += ["", "## 3. Speech check (second stage)",
              f"- Accepted: {pct(a['confirmed'], gate)}; rejected: {pct(a['rejected'], gate)}",
              f"- Rejected because the recogniser heard nothing: {pct(a['reject_empty'], a['rejected'])}",
              f"- Wakes that fired, by source: {dict(a['fires'])}"]
    if a["confirmed_text"]:
        lines.append(f"- Accepted transcripts: {dict(a['confirmed_text'])}")
    if a["reject_text"]:
        lines.append("- What the rejected audio was (ordinary speech that fooled the detector):")
        lines += [f"    - {n}x {t!r}" for t, n in a["reject_text"].most_common(8)]
    lines += ["", "## 4. Command capture", f"- Captures started: {a['capture_started']}", f"- Outcomes: {dict(a['capture_outcomes'])}",
              f"- Aborted with no speech heard: {a['capture_no_speech']}", f"- Hotkey cancels: {a['hotkey_cancel']}", ""]
    lines += ["## 5. Recognition", f"- Transcripts received: {a['transcripts']}",
              f"- Hallucinations filtered: {a['hallucinations']} ({pct(a['hallucinations'], a['transcripts'] + a['hallucinations'])} of recogniser output)"]
    if a["halluc_text"]:
        lines += [f"    - {n}x {t!r}" for t, n in a["halluc_text"].most_common(5)]
    if a["turn_rows"]:
        lines.append("- Command turns:")
        lines += [f"    - session {r['session']}: loudest {r['rms_max']:.2f}, voiced {r['voiced']}/{r['frames']} frames, ended by {r['endpoint']}, path {r['path']}" for r in a["turn_rows"][:10]]
    lines.append("")
    return "\n".join(lines)


def default_logs():
    out = []
    appdata = os.environ.get("APPDATA", "")
    p = os.path.join(appdata, "com.nexus.assistant", "logs", "nexus.log")
    if os.path.exists(p):
        out.append(p)
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--log", action="append", help="NEXUS log file (default: the personal profile log)")
    ap.add_argument("--since", help="only lines at or after this ISO timestamp, e.g. 2026-11-13T00:00:00")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)
    logs = args.log or default_logs()
    if not logs:
        print("no log found; pass --log", file=sys.stderr)
        return 2
    lines = []
    for p in logs:
        with open(p, encoding="utf-8", errors="replace") as f:
            lines.extend(f.read().splitlines())
    a = analyse(lines, args.since)
    if args.json:
        print(json.dumps({k: (dict(v) if isinstance(v, Counter) else v) for k, v in a.items()}, indent=2))
    else:
        sys.stdout.buffer.write((render(a) + "\n").encode("utf-8", "replace"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
