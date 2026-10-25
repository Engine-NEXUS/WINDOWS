#!/usr/bin/env python3
"""Pick the wake threshold from held-out data (Phase C4).

    cargo run --release --example wake_eval -- --manifest M --root R --sweep 0.50,0.55,0.60,0.65,0.68,0.72,0.76,0.80 --out wake_sweep.jsonl
    python scripts/wake_calibrate.py wake_sweep.jsonl [--max-fa-per-hour 0.125] [--max-soundalike-fa 0.01] [--apply]

For every swept threshold it reports recall on the wake clips, false alarms per hour on the room recordings
and the false-alarm rate on soundalike / everyday-word clips, then recommends the threshold with the HIGHEST
recall that stays inside both false-alarm budgets. `--apply` writes it to
src-tauri/resources/oww/acoustic_profile.json (kws_threshold) - after which you must re-run
`wake_data_foundation.py fingerprint-models`.

Speech-check policy (Phase E4):  `--verify` reads a `wake_eval --stage2` file (clips AND the long room
recordings) and chooses `wakeVerifyHigh` for `wakeVerifyMode: tiered`: candidates the classifier scores at or
above it wake WITHOUT the Groq speech check, the rest are checked. It compares the three policies
(off / always / tiered) on recall and false alarms and prints the one to use.

Honest limit: 0 false alarms in H hours of audio only shows the rate is <= 3.7/H per hour (95%).
Certifying 1 per 8 h needs ~30 h of room audio; with less, the tool still recommends from the point estimates
but says so. Standard library only.
"""

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from voice_scorecard import poisson_upper, wilson  # noqa: E402

PROFILE = Path(__file__).resolve().parent.parent / "src-tauri" / "resources" / "oww" / "acoustic_profile.json"


def load(path):
    with open(path, encoding="utf-8") as fh:
        return [json.loads(l) for l in fh if l.strip()]


def table(rows):
    """One dict per threshold, sorted ascending."""
    thresholds = sorted({t for r in rows for t in (r.get("sweep") or {})}, key=float)
    wake = [r for r in rows if r.get("kind") == "wake" and r.get("sweep")]
    neg = [r for r in rows if r.get("kind") == "negative" and r.get("sweep")]
    room = [r for r in rows if r.get("kind") == "room" and r.get("sweep")]
    hours = sum(float(r.get("duration_ms", 0)) for r in room) / 3.6e6
    out = []
    for t in thresholds:
        k = sum(bool(r["sweep"][t].get("fired")) for r in wake)
        nk = sum(bool(r["sweep"][t].get("fired")) for r in neg)
        fires = sum(int(r["sweep"][t].get("n_fires", 0)) for r in room)
        rec, rlo, _ = wilson(k, len(wake)) if wake else (float("nan"),) * 3
        out.append({
            "threshold": float(t), "recall": rec, "recall_lo": rlo, "k": k, "n_wake": len(wake),
            "soundalike_fa": (nk / len(neg)) if neg else float("nan"), "soundalike_k": nk, "n_neg": len(neg),
            "fa_per_hour": (fires / hours) if hours else float("nan"), "fires": fires,
            "fa_upper_per_hour": (poisson_upper(fires) / hours) if hours else float("nan"),
            "hours": hours,
        })
    return out


def recommend(tab, max_fa_per_hour=0.125, max_soundalike_fa=0.01):
    """Highest-recall row inside both budgets (ties -> the higher, i.e. safer, threshold)."""
    ok = [r for r in tab
          if not math.isnan(r["recall"])
          and (math.isnan(r["fa_per_hour"]) or r["fa_per_hour"] <= max_fa_per_hour)
          and (math.isnan(r["soundalike_fa"]) or r["soundalike_fa"] <= max_soundalike_fa)]
    if not ok:
        return None
    return max(ok, key=lambda r: (round(r["recall"], 6), r["threshold"]))


def render(tab, rec, max_fa_per_hour, max_soundalike_fa):
    lines = ["| Threshold | Recall (95% low) | Soundalike FA | FA / hour (95% upper) |", "|---|---|---|---|"]
    for r in tab:
        mark = "  <== recommended" if rec and r["threshold"] == rec["threshold"] else ""
        sa = "n/a" if math.isnan(r["soundalike_fa"]) else f'{r["soundalike_fa"] * 100:.1f}% ({r["soundalike_k"]}/{r["n_neg"]})'
        fa = "n/a (no room audio)" if math.isnan(r["fa_per_hour"]) else \
            f'{r["fa_per_hour"]:.2f} ({r["fires"]} in {r["hours"]:.2f} h; <= {r["fa_upper_per_hour"]:.2f})'
        lines.append(f'| {r["threshold"]:.2f} | {r["recall"] * 100:.1f}% ({r["recall_lo"] * 100:.1f}%) | {sa} | {fa}{mark} |')
    lines.append("")
    if rec is None:
        lines.append(f"No threshold meets both budgets (<= {max_fa_per_hour} false alarms/h and "
                     f"<= {max_soundalike_fa * 100:.1f}% soundalike false alarms). Improve the model (Phase C data) "
                     "or relax a budget deliberately.")
    else:
        lines.append(f"Recommended threshold: **{rec['threshold']:.2f}** "
                     f"(recall {rec['recall'] * 100:.1f}%, {rec['fa_per_hour']:.2f} false alarms/h).")
        h = rec["hours"]
        if not math.isnan(rec["fa_upper_per_hour"]) and rec["fa_upper_per_hour"] > max_fa_per_hour:
            need = poisson_upper(rec["fires"]) / max_fa_per_hour
            lines.append(f"Not certified: with {h:.2f} h of room audio the 95% upper bound is "
                         f"{rec['fa_upper_per_hour']:.2f}/h; about {need:.0f} h would be needed to prove "
                         f"<= {max_fa_per_hour}/h. Record more room audio (voice_eval_set.py room) before trusting it.")
    return "\n".join(lines)


# ─── speech-check policy (tiered verification) ───────────────────────────

VERIFY_GRID = (0.80, 0.85, 0.90, 0.93, 0.95, 0.97, 0.98, 0.99, 0.995, 0.999)


def _passes(row, t_hi):
    """Would this fired clip wake under tiered verification at `t_hi`? Sure candidates skip the check."""
    prob = row.get("first_fire_prob")
    if not row.get("fired"):
        return False
    if prob is not None and prob >= t_hi:
        return True
    return bool(row.get("stage2_pass", True))


def verify_table(rows, grid=VERIFY_GRID):
    """One entry per policy: off (t_hi=0), tiered at each grid value, always (t_hi=inf)."""
    wake = [r for r in rows if r.get("kind") == "wake"]
    neg = [r for r in rows if r.get("kind") == "negative"]
    room = [r for r in rows if r.get("kind") == "room"]
    hours = sum(float(r.get("duration_ms", 0)) for r in room) / 3.6e6
    room_probs = [p for r in room for p in (r.get("fire_probs") or [])]
    out = []
    for name, t in [("off", 0.0)] + [(f"tiered {g:.3f}", g) for g in grid] + [("always", float("inf"))]:
        k = sum(_passes(r, t) for r in wake)
        nk = sum(_passes(r, t) for r in neg)
        unchecked = sum(p >= t for p in room_probs)           # room fires that would wake with NO check at all
        rec, rlo, _ = wilson(k, len(wake)) if wake else (float("nan"),) * 3
        out.append({"policy": name, "t_hi": t, "recall": rec, "recall_lo": rlo, "k": k, "n_wake": len(wake),
                    "soundalike_fa": (nk / len(neg)) if neg else float("nan"), "soundalike_k": nk, "n_neg": len(neg),
                    "unchecked_fires": unchecked, "hours": hours,
                    "unchecked_per_hour": (unchecked / hours) if hours else float("nan"),
                    "room_fires_total": len(room_probs)})
    return out


def recommend_verify(tab, max_fa_per_hour=0.125, max_soundalike_fa=0.01):
    """Highest recall within budgets; among equals prefer the MORE checked policy (safer). `unchecked_per_hour`
    is a guaranteed floor on room false alarms (those fires skip the check); the checked ones may add more
    or be suppressed - the speech check on long room audio is not measured by wake_eval."""
    ok = [r for r in tab
          if not math.isnan(r["recall"])
          and (math.isnan(r["unchecked_per_hour"]) or r["unchecked_per_hour"] <= max_fa_per_hour)
          and (math.isnan(r["soundalike_fa"]) or r["soundalike_fa"] <= max_soundalike_fa)]
    if not ok:
        return None
    best = max(round(r["recall"], 6) for r in ok)
    return max((r for r in ok if round(r["recall"], 6) == best), key=lambda r: r["t_hi"])


def render_verify(tab, rec):
    lines = ["| Policy | Recall (95% low) | Soundalike FA | Room fires with no check /h |", "|---|---|---|---|"]
    for r in tab:
        mark = "  <== recommended" if rec and r["policy"] == rec["policy"] else ""
        sa = "n/a" if math.isnan(r["soundalike_fa"]) else f'{r["soundalike_fa"] * 100:.1f}% ({r["soundalike_k"]}/{r["n_neg"]})'
        un = "n/a (no room audio)" if math.isnan(r["unchecked_per_hour"]) else \
            f'{r["unchecked_per_hour"]:.2f} ({r["unchecked_fires"]}/{r["room_fires_total"]} fires, {r["hours"]:.1f} h)'
        lines.append(f'| {r["policy"]} | {r["recall"] * 100:.1f}% ({r["recall_lo"] * 100:.1f}%) | {sa} | {un}{mark} |')
    lines.append("")
    if rec is None:
        lines.append("No policy fits the budgets - improve the model before choosing a policy.")
    elif rec["policy"] == "always":
        lines.append("Recommended: wakeVerifyMode = always (the speech check is worth its cost here).")
    elif rec["policy"] == "off":
        lines.append("Recommended: wakeVerifyMode = off (no false-alarm budget is used up without the check).")
    else:
        lines.append(f'Recommended: wakeVerifyMode = tiered, wakeVerifyHigh = {rec["t_hi"]:.3f}.')
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("sweep_file")
    ap.add_argument("--max-fa-per-hour", type=float, default=0.125)
    ap.add_argument("--max-soundalike-fa", type=float, default=0.01)
    ap.add_argument("--apply", action="store_true", help="write the recommendation to acoustic_profile.json")
    ap.add_argument("--verify", action="store_true",
                    help="treat the file as `wake_eval --stage2` output and choose the speech-check policy")
    args = ap.parse_args(argv)
    if args.verify:
        rows = load(args.sweep_file)
        if not any("stage2_pass" in r for r in rows):
            print("no stage-2 data: run wake_eval with --stage2 (needs GROQ_API_KEY)")
            return 2
        tab = verify_table(rows)
        print(render_verify(tab, recommend_verify(tab, args.max_fa_per_hour, args.max_soundalike_fa)))
        return 0
    tab = table(load(args.sweep_file))
    if not tab:
        print("no sweep data: run wake_eval with --sweep")
        return 2
    rec = recommend(tab, args.max_fa_per_hour, args.max_soundalike_fa)
    print(render(tab, rec, args.max_fa_per_hour, args.max_soundalike_fa))
    if args.apply:
        if rec is None:
            print("not applied: no acceptable threshold")
            return 1
        prof = json.loads(PROFILE.read_text(encoding="utf-8"))
        prof["kws_threshold"] = round(rec["threshold"], 2)
        PROFILE.write_text(json.dumps(prof, indent=2) + "\n", encoding="utf-8")
        print(f"applied kws_threshold={prof['kws_threshold']} to {PROFILE}\n"
              "now run: python scripts/wake_data_foundation.py fingerprint-models")
    return 0


if __name__ == "__main__":
    sys.exit(main())
