#!/usr/bin/env python3
"""Merge wake_eval + stt_eval results into the NEXUS voice scorecard.

    cargo run --release --example wake_eval -- --manifest eval_set/manifest.jsonl --root eval_set --out wake.jsonl
    cargo run --release --example stt_eval  -- --manifest eval_set/manifest.jsonl --root eval_set --out stt.jsonl
    python scripts/voice_scorecard.py --wake wake.jsonl --stt stt.jsonl --out docs/research/voice-accuracy/scorecard.md

Every number is MEASURED on the held-out set; a row with no data shows
"not measured" and zero stars - it is never filled with a guess. Stars come
from the point estimate AND the 95% confidence bound, so a lucky 30 clips
cannot earn five stars. Standard library only.

Stars:  higher-is-better   5 = lower CI bound >= target   4 = point >= target
                           3 = within 5 pts of target     2 = within 15 pts     1 = worse
        lower-is-better    5 = upper CI bound <= target   4 = point <= target
                           3 = <= 2x target               2 = <= 5x target      1 = worse
"""

import argparse
import json
import math
import sys
from pathlib import Path

WAKE_PHRASES = ("nexus", "hey nexus", "okay nexus")

# name -> (target, direction). Rates are fractions; latency in ms; FA in events/hour.
TARGETS = {
    "wake_recall": (0.95, "high"),
    "wake_other_people": (0.90, "high"),
    "wake_owner_sealed": (0.95, "high"),
    "wake_hard_conditions": (0.90, "high"),
    "wake_false_alarm_per_hour": (1 / 8, "low"),
    "wake_negative_false_alarm": (0.01, "low"),
    "wake_latency_p95_ms": (400.0, "low"),
    "stt_word_accuracy": (0.95, "high"),
    "stt_command_match": (0.95, "high"),
    "stt_hallucination_leak": (0.005, "low"),
    "stt_wrongly_dropped": (0.005, "low"),
    "stt_one_breath": (0.95, "high"),
    "stt_endpoint_cutoff": (0.01, "low"),
}


# ─── statistics ──────────────────────────────────────────────────────────

def wilson(k, n, z=1.96):
    """Wilson score interval for k successes in n trials -> (point, lo, hi)."""
    if n <= 0:
        return (float("nan"), float("nan"), float("nan"))
    p = k / n
    denom = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return (p, max(0.0, centre - half), min(1.0, centre + half))


def poisson_upper(k, z=1.96):
    """Byar's approximation of the upper 95% bound on a Poisson count."""
    kk = k + 1
    return kk * (1 - 1 / (9 * kk) + z / (3 * math.sqrt(kk))) ** 3


def percentile(values, q):
    if not values:
        return None
    s = sorted(values)
    i = (len(s) - 1) * q
    lo, hi = math.floor(i), math.ceil(i)
    return s[lo] + (s[hi] - s[lo]) * (i - lo)


def stars(point, lo, hi, target, direction):
    """Star count 0-5; 0 means not measured."""
    if point is None or (isinstance(point, float) and math.isnan(point)):
        return 0
    if direction == "high":
        if lo >= target:
            return 5
        if point >= target:
            return 4
        if point >= target - 0.05:
            return 3
        if point >= target - 0.15:
            return 2
        return 1
    if hi <= target:
        return 5
    if point <= target:
        return 4
    if point <= 2 * target:
        return 3
    if point <= 5 * target:
        return 2
    return 1


def star_bar(n):
    return "★" * n + "☆" * (5 - n)


# ─── metric extraction ───────────────────────────────────────────────────

def load_jsonl(path):
    rows = []
    if not path:
        return rows
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def rate(name, label, k, n, note=""):
    point, lo, hi = wilson(k, n)
    target, direction = TARGETS[name]
    return {"key": name, "label": label, "kind": "rate", "k": k, "n": n, "point": point if n else None,
            "lo": lo if n else None, "hi": hi if n else None, "target": target,
            "direction": direction, "stars": stars(point if n else None, lo, hi, target, direction), "note": note}


def _gate_fired(r, full_gate):
    if not r.get("fired"):
        return False
    if full_gate:
        return bool(r.get("stage2_pass", False))
    return True


def wake_metrics(rows, full_gate=False):
    """Metrics from wake_eval rows. `full_gate` also requires the stage-2 STT check to pass."""
    out = []
    wake = [r for r in rows if r.get("kind") == "wake" and "error" not in r]
    suffix = " (with speech check)" if full_gate else " (neural only)"
    for ph in WAKE_PHRASES:
        sub = [r for r in wake if r.get("phrase") == ph]
        k = sum(_gate_fired(r, full_gate) for r in sub)
        out.append(rate("wake_recall", f'"{ph.upper()}" recall{suffix}', k, len(sub)))
    owner = {"owner"}
    others = [r for r in wake if r.get("speaker_id") not in owner]
    out.append(rate("wake_other_people", "Other people (non-owner speakers)" + suffix,
                    sum(_gate_fired(r, full_gate) for r in others), len(others)))
    sealed = [r for r in wake if r.get("source") == "owner_sealed"]
    out.append(rate("wake_owner_sealed", "Owner's own takes withheld from training (all phrases)" + suffix,
                    sum(_gate_fired(r, full_gate) for r in sealed), len(sealed)))
    hard = [r for r in wake if r.get("environment", "quiet") != "quiet"
            or float(r.get("distance_cm", 0) or 0) >= 100 or r.get("volume") == "whisper"]
    out.append(rate("wake_hard_conditions", "Noisy room / far away / whisper" + suffix,
                    sum(_gate_fired(r, full_gate) for r in hard), len(hard)))

    # False alarms: long room recordings (events/hour) and short soundalike clips.
    room = [r for r in rows if r.get("kind") == "room" and "error" not in r]
    secs = sum(float(r.get("duration_ms", 0)) / 1000.0 for r in room)
    fires = sum(int(r.get("n_fires", 0)) for r in room)
    hours = secs / 3600.0
    target, direction = TARGETS["wake_false_alarm_per_hour"]
    if hours > 0:
        per_h = fires / hours
        hi = poisson_upper(fires) / hours
        out.append({"key": "wake_false_alarm_per_hour", "label": "False alarms per hour (room audio, neural only)",
                    "kind": "perhour", "k": fires, "n": round(hours, 2), "point": per_h, "lo": None, "hi": hi,
                    "target": target, "direction": direction, "stars": stars(per_h, 0, hi, target, direction),
                    "note": f"{hours:.2f} h of audio"})
    else:
        out.append({"key": "wake_false_alarm_per_hour", "label": "False alarms per hour (room audio, neural only)",
                    "kind": "perhour", "k": 0, "n": 0, "point": None, "lo": None, "hi": None, "target": target,
                    "direction": direction, "stars": 0, "note": "no room audio"})
    neg = [r for r in rows if r.get("kind") == "negative" and "error" not in r]
    out.append(rate("wake_negative_false_alarm", "Soundalike / everyday-word false alarms" + suffix,
                    sum(_gate_fired(r, full_gate) for r in neg), len(neg)))

    # Speed: from first loud sound to the wake firing (+ the speech-check round trip when measured).
    lat = []
    for r in wake:
        if not _gate_fired(r, full_gate):
            continue
        d = r.get("fire_after_onset_ms")
        if d is None:
            continue
        lat.append(float(d) + (float(r.get("stage2_ms", 0)) if full_gate else 0.0))
    target, direction = TARGETS["wake_latency_p95_ms"]
    p95 = percentile(lat, 0.95)
    out.append({"key": "wake_latency_p95_ms", "label": "Wake speed p95 (ms after you start speaking)" + suffix,
                "kind": "ms", "k": len(lat), "n": len(lat), "point": p95, "lo": None, "hi": p95,
                "target": target, "direction": direction,
                "stars": 0 if p95 is None else stars(p95, 0, p95, target, direction),
                "note": f"median {percentile(lat, 0.5):.0f} ms" if lat else "no fired clips"})
    return out


def stt_metrics(rows):
    out = []
    cmd = [r for r in rows if r.get("kind") == "command" and "error" not in r]
    plain = [r for r in cmd if not r.get("one_breath")]
    breath = [r for r in cmd if r.get("one_breath")]
    scored = [r for r in cmd if r.get("expected_text") and "ref_words" in r]
    ref = sum(int(r["ref_words"]) for r in scored)
    edits = sum(min(int(r["edits_raw"]), int(r["ref_words"])) for r in scored)
    target, direction = TARGETS["stt_word_accuracy"]
    if ref:
        acc = 1 - edits / ref
        # Word-level CI treating words as independent trials (optimistic; fine for ranking).
        _, lo, hi = wilson(ref - edits, ref)
        out.append({"key": "stt_word_accuracy", "label": "Word accuracy (1 - WER, raw transcript)", "kind": "rate",
                    "k": ref - edits, "n": ref, "point": acc, "lo": lo, "hi": hi, "target": target,
                    "direction": direction, "stars": stars(acc, lo, hi, target, direction), "note": "words"})
    else:
        out.append({"key": "stt_word_accuracy", "label": "Word accuracy (1 - WER, raw transcript)", "kind": "rate",
                    "k": 0, "n": 0, "point": None, "lo": None, "hi": None, "target": target,
                    "direction": direction, "stars": 0, "note": ""})
    ok = [r for r in plain if "intent_ok" in r]
    out.append(rate("stt_command_match", "Understands the command (audio -> intent)",
                    sum(bool(r["intent_ok"]) for r in ok), len(ok)))
    non_empty = [r for r in cmd if r.get("raw", "").strip()]
    out.append(rate("stt_wrongly_dropped", "Real commands wiped by the hallucination filter",
                    sum(bool(r.get("dropped_by_filter")) for r in non_empty), len(non_empty)))
    bg = [r for r in rows if r.get("kind") in ("silence", "noise") and "error" not in r]
    out.append(rate("stt_hallucination_leak", "Hallucinated text on silence / TV noise",
                    sum(bool(r.get("leaked")) for r in bg), len(bg)))
    ep = [r for r in cmd if "cut_early" in r]
    if ep:
        out.append(rate("stt_endpoint_cutoff", "Commands cut off mid-sentence by the endpointer",
                        sum(bool(r["cut_early"]) for r in ep), len(ep),
                        note="stt_eval --simulate-endpoint (sweep --endpoint-silence-chunks)"))
    bok = [r for r in breath if "intent_ok" in r]
    sim = [r for r in bok if "capture_offset_ms" in r]
    if sim:
        # stt_eval --capture-offset-ms: the real wake engine decided where the
        # capture starts, so capture loss and wake misses are INCLUDED.
        offs = sorted({r["capture_offset_ms"] for r in sim})
        missed = sum(bool(r.get("wake_missed")) for r in sim)
        m = rate("stt_one_breath",
                 f'"NEXUS open Chrome" in one breath (wake + capture simulated, offset {offs[0]} ms)',
                 sum(bool(r["intent_ok"]) for r in bok), len(bok),
                 note=f"{missed} wake miss(es) counted as failures")
    else:
        m = rate("stt_one_breath", '"NEXUS open Chrome" in one breath (STT + parser only)',
                 sum(bool(r["intent_ok"]) for r in bok), len(bok),
                 note="whole clip transcribed; capture loss NOT included - run stt_eval --capture-offset-ms")
    out.append(m)
    return out


# ─── confidence-gate calibration ─────────────────────────────────────────

LOGPROB_STEPS = (-0.6, -0.8, -1.0, -1.2, -1.5, -2.0)
RATIO_STEPS = (2.0, 2.4, 2.6, 3.0, 4.0)
MAX_WRONG_REJECT = 0.005  # same budget as TARGETS["stt_wrongly_dropped"]


def confidence_sweep(rows):
    """For each candidate threshold: hallucinations it would still catch vs real
    commands it would wrongly reject. Uses the stats stt_eval records from Groq's
    verbose_json. Returns None when the rows carry no such stats (local engine,
    or a run made before the stats existed).

    leaks   = silence/noise clips whose text SURVIVED the text filter (the only
              ones a confidence gate could still help with)
    commands = real command clips that produced text
    The shipped rule rejects when every segment is below the logprob floor
    (max_avg_logprob < t) or any segment's compression ratio exceeds the cap.
    """
    cmds = [r for r in rows if r.get("kind") == "command" and r.get("raw", "").strip()
            and "max_avg_logprob" in r]
    leaks = [r for r in rows if r.get("kind") in ("silence", "noise") and r.get("leaked")
             and "max_avg_logprob" in r]
    if not cmds:
        return None
    sweep = []
    for signal, steps in (("avg_logprob", LOGPROB_STEPS), ("compression_ratio", RATIO_STEPS)):
        for t in steps:
            if signal == "avg_logprob":
                hit = lambda r, t=t: float(r["max_avg_logprob"]) < t  # noqa: E731
            else:
                hit = lambda r, t=t: float(r.get("max_compression_ratio", 0)) > t  # noqa: E731
            wrong = sum(1 for r in cmds if hit(r))
            caught = sum(1 for r in leaks if hit(r))
            sweep.append({"signal": signal, "threshold": t, "wrong": wrong, "n_commands": len(cmds),
                          "caught": caught, "n_leaks": len(leaks),
                          "wrong_rate": wrong / len(cmds),
                          "caught_rate": (caught / len(leaks)) if leaks else None})
    return sweep


def recommend_gate(sweep):
    """Per signal, the setting that catches the most leaks while rejecting at most
    MAX_WRONG_REJECT of real commands; None if no setting helps (or no leaks)."""
    best = {}
    for row in sweep or []:
        if row["n_leaks"] == 0 or row["wrong_rate"] > MAX_WRONG_REJECT or not row["caught"]:
            continue
        cur = best.get(row["signal"])
        if cur is None or row["caught"] > cur["caught"]:
            best[row["signal"]] = row
    return best


def render_sweep(sweep):
    if sweep is None:
        return ""
    rec = recommend_gate(sweep)
    lines = ["### Confidence gate calibration (setting is OFF unless you enable it)", "",
             "| Signal | Threshold | Leaks caught | Real commands rejected |", "|---|---|---|---|"]
    for r in sweep:
        op = "best segment <" if r["signal"] == "avg_logprob" else "any segment >"
        caught = "n/a (no leaks)" if r["caught_rate"] is None else f'{r["caught"]}/{r["n_leaks"]}'
        lines.append(f'| {r["signal"]} | {op} {r["threshold"]} | {caught} | '
                     f'{r["wrong"]}/{r["n_commands"]} ({r["wrong_rate"] * 100:.1f}%) |')
    lines.append("")
    if rec:
        parts = [f'{k} at {v["threshold"]} (catches {v["caught"]}/{v["n_leaks"]}, rejects '
                 f'{v["wrong"]}/{v["n_commands"]} commands)' for k, v in rec.items()]
        lines.append("**Safe to enable:** " + "; ".join(parts) + ". Set the matching constant in "
                     "`stt_groq.rs`, then turn on *Reject low-confidence speech*.")
    else:
        lines.append("**Do not enable the gate:** no threshold catches leaks without rejecting more than "
                     f"{MAX_WRONG_REJECT * 100:.1f}% of real commands (or there were no leaks to catch).")
    return "\n".join(lines)


# ─── rendering ───────────────────────────────────────────────────────────

def fmt_value(m):
    if m["point"] is None:
        return "not measured"
    if m["kind"] == "rate":
        return f'{m["point"] * 100:.1f}% ({m["k"]}/{m["n"]})'
    if m["kind"] == "perhour":
        return f'{m["point"]:.2f}/h ({m["k"]} in {m["n"]} h)'
    return f'{m["point"]:.0f} ms'


def fmt_ci(m):
    if m["point"] is None:
        return "-"
    if m["kind"] == "rate":
        return f'{m["lo"] * 100:.1f}-{m["hi"] * 100:.1f}%'
    if m["kind"] == "perhour":
        return f'<= {m["hi"]:.2f}/h'
    return "-"


def fmt_target(m):
    t = m["target"]
    if m["kind"] == "rate":
        sign = ">=" if m["direction"] == "high" else "<="
        return f"{sign} {t * 100:g}%"
    if m["kind"] == "perhour":
        return "<= 1 per 8 h"
    return f"<= {t:g} ms"


def render_table(title, metrics, baseline=None):
    lines = [f"### {title}", "", "| What | Measured | 95% range | Target | Stars |", "|---|---|---|---|---|"]
    for m in metrics:
        val = fmt_value(m)
        prev = (baseline or {}).get(m["label"])
        if prev is not None and m["point"] is not None and prev.get("point") is not None:
            val += f' (was {fmt_value(prev)})'
        note = f' - {m["note"]}' if m.get("note") else ""
        lines.append(f'| {m["label"]}{note} | {val} | {fmt_ci(m)} | {fmt_target(m)} | {star_bar(m["stars"])} |')
    return "\n".join(lines)


def overall(all_metrics):
    measured = [m for m in all_metrics if m["stars"] > 0]
    if not measured:
        return 0, "nothing measured yet"
    worst = min(m["stars"] for m in measured)
    unmeasured = len(all_metrics) - len(measured)
    note = f"weakest measured row is {worst} star(s)"
    if unmeasured:
        note += f"; {unmeasured} row(s) not measured yet"
    return worst, note


def build_report(wake_rows, stt_rows, baseline=None, title="NEXUS voice accuracy scorecard"):
    sections = []
    all_m = []
    if wake_rows:
        w1 = wake_metrics(wake_rows, full_gate=False)
        sections.append(render_table("Wake word - neural model only", w1, baseline))
        all_m += w1
        if any("stage2_pass" in r for r in wake_rows):
            w2 = [m for m in wake_metrics(wake_rows, full_gate=True)
                  if m["key"] in ("wake_recall", "wake_other_people", "wake_hard_conditions",
                                  "wake_negative_false_alarm", "wake_latency_p95_ms")]
            sections.append(render_table("Wake word - with the speech check (stage 2)", w2, baseline))
            all_m += w2
    if stt_rows:
        s = stt_metrics(stt_rows)
        sections.append(render_table("Speech-to-text", s, baseline))
        all_m += s
        sweep_md = render_sweep(confidence_sweep(stt_rows))
        if sweep_md:
            sections.append(sweep_md)
    n, note = overall(all_m)
    head = [f"# {title}", "", f"**Overall: {star_bar(n)}** - {note}.", "",
            "Stars follow the lower confidence bound against the target, not just the average. "
            "'not measured' means no held-out data was supplied, never a guess.", ""]
    return "\n\n".join(["\n".join(head)] + sections) + "\n", all_m


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--wake", help="wake_eval output (jsonl)")
    ap.add_argument("--stt", help="stt_eval output (jsonl)")
    ap.add_argument("--out", help="write the markdown report here")
    ap.add_argument("--json", help="write machine-readable metrics here (use as --baseline later)")
    ap.add_argument("--baseline", help="metrics json from an earlier run, shows 'was ...' deltas")
    ap.add_argument("--title", default="NEXUS voice accuracy scorecard")
    args = ap.parse_args(argv)
    if not args.wake and not args.stt:
        ap.error("give --wake and/or --stt")
    base = None
    if args.baseline:
        prev = json.loads(Path(args.baseline).read_text(encoding="utf-8"))
        base = {m["label"]: m for m in prev.get("metrics", [])}
    report, metrics = build_report(load_jsonl(args.wake), load_jsonl(args.stt), base, args.title)
    print(report)
    if args.out:
        Path(args.out).parent.mkdir(parents=True, exist_ok=True)
        Path(args.out).write_text(report, encoding="utf-8")
    if args.json:
        Path(args.json).write_text(json.dumps({"metrics": metrics}, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
