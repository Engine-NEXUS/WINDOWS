#!/usr/bin/env python3
"""One command for "how accurate is the wake word?" (Phase E0).  `nexus data wake validate` only checks
files and hashes - THIS measures recall and false alarms on audio the model never trained on:

    sealed owner takes   wake_word_data/sealed/manifest.jsonl   (withheld by the trainer: --seal-owner)
    held-out eval set    eval_set/manifest.jsonl                (voice_eval_set.py --purpose eval; other speakers)
    public held-out      wake_word_data/public/heldout/         (hours of LibriSpeech/MUSAN -> false alarms per hour)

    python scripts/wake_score.py --json before.json                     # the SHIPPED model
    python scripts/wake_score.py --model src-tauri/resources/oww/nexus.candidate.onnx --baseline before.json

It runs the real engine (`cargo run --release --example wake_eval`) once per available manifest, merges the
rows and builds the scorecard (voice_scorecard.py). With --stage2 and GROQ_API_KEY it also scores the
speech check. Missing sources are skipped with a note; a row with no data stays "not measured".
"""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OWW = ROOT / "src-tauri" / "resources" / "oww"
sys.path.insert(0, str(Path(__file__).resolve().parent))


def find_sources(root=ROOT):
    """[(label, manifest path, root dir)] for every wake test source that exists and is non-empty."""
    root = Path(root)
    cands = [("held-out eval set", root / "eval_set" / "manifest.jsonl", root / "eval_set"),
             ("sealed owner takes", root / "wake_word_data" / "sealed" / "manifest.jsonl", root / "wake_word_data"),
             ("public held-out audio", root / "wake_word_data" / "public" / "heldout" / "manifest.jsonl",
              root / "wake_word_data" / "public" / "heldout")]
    return [(label, m, r) for label, m, r in cands if m.exists() and m.stat().st_size > 0]


def resources_with_model(model, tmp, oww=OWW):
    """A temporary `<tmp>/oww/` copy of the runtime resources whose nexus.onnx is `model`; returns <tmp>."""
    dest = Path(tmp) / "oww"
    shutil.copytree(oww, dest)
    shutil.copy2(model, dest / "nexus.onnx")
    return Path(tmp)


def merge_jsonl(paths, out):
    n = 0
    with open(out, "w", encoding="utf-8") as fh:
        for p in paths:
            for line in Path(p).read_text(encoding="utf-8").splitlines():
                if line.strip():
                    fh.write(line + "\n")
                    n += 1
    return n


def eval_cmd(manifest, root, out, resources=None, threshold=None, dsp=None, stage2=False):
    cmd = ["cargo", "run", "--release", "--manifest-path", str(ROOT / "src-tauri" / "Cargo.toml"),
           "--example", "wake_eval", "--", "--manifest", str(manifest), "--root", str(root), "--out", str(out)]
    if resources:
        cmd += ["--resources", str(resources)]
    if threshold is not None:
        cmd += ["--threshold", str(threshold)]
    if dsp:
        cmd += ["--dsp", dsp]
    if stage2:
        cmd += ["--stage2"]
    return cmd


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--model", help="score this .onnx instead of the shipped nexus.onnx (e.g. nexus.candidate.onnx)")
    ap.add_argument("--threshold", type=float)
    ap.add_argument("--dsp", choices=["legacy", "aligned"])
    ap.add_argument("--stage2", action="store_true", help="also run the Groq speech check (needs GROQ_API_KEY)")
    ap.add_argument("--out", help="markdown report path")
    ap.add_argument("--json", help="metrics json (use later as --baseline)")
    ap.add_argument("--baseline")
    ap.add_argument("--results", default=str(ROOT / "wake_results.jsonl"))
    ap.add_argument("--list", action="store_true", help="show which sources would be scored, then stop")
    args = ap.parse_args(argv)

    sources = find_sources()
    if not sources:
        print("No wake test audio found. Expected one of:\n"
              "  eval_set/manifest.jsonl                   record with: python scripts/voice_eval_set.py\n"
              "  wake_word_data/sealed/manifest.jsonl      written by: nexus wake train (--seal-owner)\n"
              "  wake_word_data/public/heldout/            written by: nexus wake fetch / prepare")
        return 1
    for label, m, r in sources:
        print(f"  source: {label}  ({m})")
    if not args.model and any(label == "sealed owner takes" for label, _, _ in sources):
        print("NOTE: the SHIPPED model was trained before these takes were sealed, so it has probably HEARD them: its\n"
              "      'sealed owner takes' row is optimistic (an upper bound). The candidate has not heard them. For a\n"
              "      fair before/after use the held-out eval_set (other speakers / a fresh owner session) and the public\n"
              "      held-out audio, which neither model trained on.")
    if args.list:
        return 0

    with tempfile.TemporaryDirectory() as tmp:
        resources = None
        if args.model:
            resources = resources_with_model(args.model, Path(tmp) / "res")
            print(f"scoring candidate model: {args.model}")
        else:
            print("scoring the SHIPPED model (src-tauri/resources/oww/nexus.onnx)")
        parts = []
        for i, (label, manifest, root) in enumerate(sources):
            out = Path(tmp) / f"part{i}.jsonl"
            print(f"== {label}", flush=True)
            rc = subprocess.run(eval_cmd(manifest, root, out, resources, args.threshold, args.dsp, args.stage2)).returncode
            if rc != 0:
                print(f"wake_eval failed on {label} (exit {rc}); not merging.")
                return rc
            parts.append(out)
        n = merge_jsonl(parts, args.results)
    print(f"{n} rows -> {args.results}")
    import voice_scorecard
    sc = ["--wake", args.results]
    for flag in ("out", "json", "baseline"):
        if getattr(args, flag):
            sc += [f"--{flag}", getattr(args, flag)]
    return voice_scorecard.main(sc)


if __name__ == "__main__":
    sys.exit(main())
