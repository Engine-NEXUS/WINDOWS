#!/usr/bin/env python3
"""One command that runs the whole wake-word improvement loop and tells you, in numbers, whether the new
model is better - so you can review it yourself on your own laptop:  `nexus wake improve`

    seal      dry-run the trainer: decides which of YOUR takes are withheld from training (the test set)
    before    score the SHIPPED model on audio it cannot have trained on        -> before.json
    fetch     real public speech / noise / TV / held-out hours (skip: --skip-fetch)
    synth     thousands of synthetic speakers + clear soundalikes (skip: --skip-synth)
    audit     find mislabeled clips from the audio; STOPS (exit 3) until you have listened to the flagged ones
    train     candidate model (nexus.candidate.onnx); the shipped model is untouched
    after     score the candidate on the same audio, with deltas vs `before`    -> after.json + scorecard
    verdict   IMPROVED | SAME | REGRESSED | NO_DATA + an honest report          -> improvement-report.md

Every stage is resumable (state in wake_word_data/improve_state.json); re-run the command after you finish
the by-ear review and it continues. `--promote` installs the candidate ONLY when the verdict is IMPROVED.

What it will not do: invent a number. With no recordings / no held-out audio the verdict is NO_DATA and the
report says what is missing. Rows both models trained on are marked optimistic and never count as proof.
"""

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
DATA = ROOT / "wake_word_data"
OWW = ROOT / "src-tauri" / "resources" / "oww"
REPORT = ROOT / "docs" / "research" / "voice-accuracy" / "improvement-report.md"
STATE = DATA / "improve_state.json"

# Rows both models may have trained on, or that are informational: shown, never used to claim improvement.
OPTIMISTIC_KEYS = {"wake_owner_sealed"}
MIN_RATE_N = 20            # fewer clips than this is "inconclusive", not evidence
MIN_HOURS = 1.0            # less held-out audio than this cannot support a false-alarms-per-hour comparison
FETCH_IDS = ["librispeech-dev-clean", "librispeech-test-clean", "librispeech-train-clean-100",
             "speech-commands", "musan", "rirs-noises"]

EXIT_NEEDS_REVIEW = 3


# ─── verdict (pure, unit-tested) ─────────────────────────────────────────

def _by_key(metrics):
    return {m["key"] + "|" + m["label"]: m for m in metrics}


def compare(before, after):
    """[{label, key, before, after, n_before, n_after, status}] with status in
    improved | same | regressed | inconclusive | optimistic | new. `before`/`after` = metric lists from
    voice_scorecard --json."""
    b, a = _by_key(before), _by_key(after)
    out = []
    for k, ma in a.items():
        mb = b.get(k)
        row = {"key": ma["key"], "label": ma["label"], "kind": ma.get("kind"), "after": ma.get("point"),
               "n_after": ma.get("n"), "before": mb.get("point") if mb else None, "n_before": mb.get("n") if mb else None,
               "direction": ma.get("direction", "high"), "target": ma.get("target")}
        pa, pb = ma.get("point"), (mb or {}).get("point")
        n = ma.get("n") or 0
        if ma["key"] in OPTIMISTIC_KEYS:
            row["status"] = "optimistic"
        elif pa is None or pb is None:
            row["status"] = "new" if pa is not None and pb is None else "inconclusive"
        elif ma.get("kind") == "rate" and (n < MIN_RATE_N or (mb.get("n") or 0) < MIN_RATE_N):
            row["status"] = "inconclusive"
        elif ma.get("kind") == "perhour" and (n < MIN_HOURS or (mb.get("n") or 0) < MIN_HOURS):
            row["status"] = "inconclusive"
        else:
            # one clip of slack on rates: a single flipped take is noise, not a regression
            slack = (1.0 / n) if ma.get("kind") == "rate" and n else 1e-9
            if ma.get("direction", "high") == "high":
                row["status"] = "improved" if pa > pb + slack else ("regressed" if pa < pb - slack else "same")
            else:
                row["status"] = "improved" if pa < pb - slack else ("regressed" if pa > pb + slack else "same")
        out.append(row)
    return out


def verdict(rows):
    """IMPROVED needs >=1 conclusive improvement and zero regressions; any regression is REGRESSED."""
    conclusive = [r for r in rows if r["status"] in ("improved", "same", "regressed")]
    if not conclusive:
        return "NO_DATA"
    if any(r["status"] == "regressed" for r in conclusive):
        return "REGRESSED"
    return "IMPROVED" if any(r["status"] == "improved" for r in conclusive) else "SAME"


def _fmt(row, which):
    v = row[which]
    if v is None:
        return "not measured"
    if row.get("kind") == "rate":
        return f"{v * 100:.1f}%"
    if row.get("kind") == "perhour":
        return f"{v:.2f}/h"
    if row.get("kind") == "ms":
        return f"{v:.0f} ms"
    return f"{v:.3g}"


ICON = {"improved": "better", "same": "same", "regressed": "WORSE", "inconclusive": "too little data",
        "optimistic": "optimistic*", "new": "new"}


def render_report(rows, v, ctx):
    lines = [f"# Wake-word improvement report - {time.strftime('%Y-%m-%d %H:%M')}", "",
             f"**Verdict: {v}**", ""]
    lines += {
        "IMPROVED": ["The candidate is better on audio neither model trained on, with no conclusive regression."],
        "SAME": ["No conclusive difference. Keep the shipped model unless a different reason justifies the swap."],
        "REGRESSED": ["At least one conclusive row got WORSE. Do not promote this candidate."],
        "NO_DATA": ["Nothing conclusive could be measured. See 'Missing' below - add that audio and re-run."],
    }[v]
    lines += ["", "| Metric | Before (shipped) | After (candidate) | Result |", "|---|---|---|---|"]
    for r in rows:
        lines.append(f"| {r['label']} | {_fmt(r, 'before')} (n={r['n_before']}) | {_fmt(r, 'after')} (n={r['n_after']}) | {ICON[r['status']]} |")
    lines += ["", "\\* *optimistic*: the shipped model was trained before these takes were withheld, so it has probably "
                  "heard them. These rows are informational and never count towards the verdict.",
              f"Rows with fewer than {MIN_RATE_N} clips (or less than {MIN_HOURS:g} h of held-out audio for false alarms per hour) are "
              "'too little data', not evidence. One flipped clip is treated as noise.", ""]
    if ctx.get("missing"):
        lines += ["## Missing", ""] + [f"- {m}" for m in ctx["missing"]] + [""]
    if ctx.get("notes"):
        lines += ["## Run notes", ""] + [f"- {m}" for m in ctx["notes"]] + [""]
    lines += ["## What this does NOT tell you", "",
              "- Accuracy for people whose voices are not in the held-out set, and for microphones you did not test.",
              "- Command-recognition (STT) accuracy: run `nexus wake score --stage2` and the stt_eval step of the runbook.",
              "- False alarms per hour are only certified when the room/held-out audio is long enough "
              "(about 30 h for 1 per 8 h); the scorecard prints the exact bound.", ""]
    return "\n".join(lines)


# ─── stages ──────────────────────────────────────────────────────────────

def load_state(path=STATE):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        return {}


def save_state(state, path=STATE):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(state, indent=2), encoding="utf-8")


def py(script, *args):
    return [sys.executable, str(SCRIPTS / script), *map(str, args)]


def default_run(argv):
    print("  $ " + " ".join(argv), flush=True)
    return subprocess.run(argv, cwd=str(ROOT)).returncode


def has_wavs(folder):
    return any(Path(folder).glob("*.wav")) if Path(folder).exists() else False


def run_all(args, run=default_run, data=DATA, report_path=REPORT, oww=OWW):
    data = Path(data)
    state = load_state(data / "improve_state.json")
    notes, missing = [], []
    before_json, after_json = data / "before.json", data / "after.json"
    cand = oww / "nexus.candidate.onnx"

    def stage(name, argv, allow_fail=False, persist=True):
        if persist and name in state.get("done", []) and not args.redo:
            print(f"== {name}: already done (use --redo to repeat)")
            return 0
        print(f"== {name}", flush=True)
        rc = run(argv)
        if rc == 0:
            if persist:
                state.setdefault("done", []).append(name)
                save_state(state, data / "improve_state.json")
        elif not allow_fail:
            print(f"stage '{name}' failed (exit {rc}); fix it and re-run `nexus wake improve` to continue.")
        return rc

    if not has_wavs(data / "positive"):
        missing.append("No positive recordings in wake_word_data/positive (record with `nexus wake record`; "
                       "synthetic voices alone cannot show how the model hears YOU).")

    # 1. seal (decides the held-out owner takes) - only meaningful when there is owner data
    if has_wavs(data / "positive"):
        rc = stage("seal", py("train_local_wakeword.py", "--dry-run", "--skip-audit-gate", "--data-dir", data,
                                "--out", data / "seal_dry_run.onnx"), allow_fail=True, persist=False)
        if rc != 0:
            notes.append("seal/dry-run failed - owner takes were not sealed (see output above)")

    # 2. before
    if before_json.exists() and "before" in state.get("done", []) and not args.redo:
        print("== before: already done")
    else:
        rc = stage("before", py("wake_score.py", "--json", before_json, "--out", report_path.with_name("scorecard-before.md")),
                   allow_fail=True)
        if rc != 0:
            missing.append("Could not score the shipped model (no test audio: eval_set/, sealed takes, public held-out hours; "
                           "or cargo/GROQ issue).")

    # 3. data
    if not args.skip_fetch:
        if stage("fetch", py("wake_data_fetch.py", "get", *FETCH_IDS, "--max-gb", args.max_gb), allow_fail=True) == 0:
            stage("prepare", py("wake_data_fetch.py", "prepare", "--max-hours", args.max_hours), allow_fail=True)
        else:
            notes.append("public data fetch failed or was blocked - training continues without it")
    if not args.skip_synth:
        if stage("synth-voices", py("wake_data_synth.py", "voices"), allow_fail=True) == 0:
            stage("synth-positives", py("wake_data_synth.py", "positives", "--per-phrase", args.per_phrase), allow_fail=True)
            stage("synth-negatives", py("wake_data_synth.py", "negatives"), allow_fail=True)
        else:
            notes.append("synthetic voices unavailable (Piper / network) - training continues without them")

    # 4. audit gate (human, by ear)
    state["done"] = [s for s in state.get("done", []) if s not in ("audit",)]      # always re-score labels
    if run(py("wake_label_audit.py", "inventory")) != 0 or run(py("wake_label_audit.py", "score")) != 0:
        print("audit failed - see output above")
        return 1
    if run(py("wake_label_audit.py", "status")) != 0 and not args.allow_unreviewed:
        print("\nSTOP: listen to the flagged clips (this is the one step only a person can do):\n"
              "    nexus wake audit review\nthen run `nexus wake improve` again - it continues from here.")
        return EXIT_NEEDS_REVIEW
    gate = ["--skip-audit-gate"] if args.allow_unreviewed else []
    if args.allow_unreviewed:
        notes.append("trained with UNREVIEWED flagged clips (--allow-unreviewed) - treat the result as unreliable")

    # 5. train
    tr = ["--data-dir", data, *gate]
    if args.epochs:
        tr += ["--epochs", args.epochs]
    if stage("train", py("train_local_wakeword.py", *tr), persist=False) != 0:
        return 1

    # 6. after
    rc = stage("after", py("wake_score.py", "--model", cand, "--baseline", before_json, "--json", after_json,
                           "--out", report_path.with_name("scorecard-after.md")), allow_fail=True, persist=False)
    if rc != 0 or not (before_json.exists() and after_json.exists()):
        missing.append("Could not score the candidate against the shipped model (need the same test audio for both).")

    # 7. verdict
    rows = []
    if before_json.exists() and after_json.exists():
        rows = compare(json.loads(before_json.read_text())["metrics"], json.loads(after_json.read_text())["metrics"])
    v = verdict(rows)
    text = render_report(rows, v, {"missing": missing, "notes": notes})
    Path(report_path).parent.mkdir(parents=True, exist_ok=True)
    Path(report_path).write_text(text, encoding="utf-8")
    print("\n" + text)
    print(f"report written to {report_path}")

    if args.promote:
        if v == "IMPROVED" and cand.exists():
            shutil.copy2(oww / "nexus.onnx", oww / "nexus.previous.onnx")
            shutil.copy2(cand, oww / "nexus.onnx")
            run(py("wake_data_foundation.py", "fingerprint-models"))
            print("PROMOTED: candidate is now the shipped model (old one kept as nexus.previous.onnx). Commit it.")
        else:
            print(f"NOT promoted: verdict is {v}.")
    return 0 if v != "REGRESSED" else 2


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--skip-fetch", action="store_true", help="no public data download (offline / already fetched)")
    ap.add_argument("--skip-synth", action="store_true", help="no synthetic voices")
    ap.add_argument("--max-gb", default="12")
    ap.add_argument("--max-hours", default="8")
    ap.add_argument("--per-phrase", default="800")
    ap.add_argument("--epochs", default=None)
    ap.add_argument("--allow-unreviewed", action="store_true", help="train even though flagged clips are unreviewed (not recommended)")
    ap.add_argument("--promote", action="store_true", help="install the candidate only if the verdict is IMPROVED")
    ap.add_argument("--redo", action="store_true", help="repeat stages already marked done")
    args = ap.parse_args(argv)
    return run_all(args)


if __name__ == "__main__":
    sys.exit(main())
