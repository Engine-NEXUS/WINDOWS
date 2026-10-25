#!/usr/bin/env python3
"""Label wake-word training audio BY EAR (Phase C2). Whisper is not allowed to judge: it re-labelled
136 of the owner's real "NEXUS" recordings as negatives on 2026-10-01 and poisoned the dataset.

    # cut the long free-talk recordings into single utterances to review
    python scripts/wake_audition.py segment --src wake_word_data/free --out wake_word_data/review

    # listen and decide: [p]ositive (said NEXUS)  [n]egative  [r]eplay  [s]kip  [q]uit
    python scripts/wake_audition.py review --dir wake_word_data/review
    python scripts/wake_audition.py review --dir wake_word_data/negative --pattern "soundalike_from_*"

    # audit queue (Phase E1): the clips scripts/wake_label_audit.py flagged, worst first, with the
    # model's score and the current label shown. [k]eep = the label is right; [x] = exclude from training
    python scripts/wake_audition.py review --queue wake_word_data/audit_flags.jsonl

    python scripts/wake_audition.py stats

Decisions move the file to wake_word_data/positive or negative (named audition_<n>.wav so source and
recorder indices never collide) and append to audition_log.jsonl. Nothing is ever deleted. Playback needs
`sounddevice`; `segment` and `stats` need only the standard library.
"""

import argparse
import array
import json
import shutil
import sys
import time
import wave
from fnmatch import fnmatch
from pathlib import Path

BASE = Path(__file__).resolve().parent.parent / "wake_word_data"
SR = 16000

DECISIONS = {"p": "positive", "n": "negative"}


# ─── pure helpers (unit-tested) ──────────────────────────────────────────

def read_wav(path):
    with wave.open(str(path), "rb") as w:
        sr, ch, width = w.getframerate(), w.getnchannels(), w.getsampwidth()
        raw = w.readframes(w.getnframes())
    if width != 2:
        raise ValueError(f"{path}: expected 16-bit PCM")
    a = array.array("h")
    a.frombytes(raw)
    if sys.byteorder == "big":
        a.byteswap()
    if ch > 1:
        a = array.array("h", a[::ch])
    return sr, a


def write_wav(path, samples, sr=SR):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    a = array.array("h", samples)
    if sys.byteorder == "big":
        a.byteswap()
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(a.tobytes())


def split_utterances(samples, sr=SR, frame_ms=20, rms_gate=0.012, min_ms=250, gap_ms=500, pad_ms=200, max_ms=4000):
    """Energy-based utterance finder. Returns [(start_sample, end_sample), ...].

    A voiced run ends after `gap_ms` of quiet; runs shorter than `min_ms` are dropped (clicks); each run
    is padded by `pad_ms` on both sides and long runs are cut into `max_ms` pieces so a clip holds
    one or two words, not a paragraph."""
    n = sr * frame_ms // 1000
    frames = []
    for i in range(0, len(samples) - n + 1, n):
        seg = samples[i:i + n]
        rms = (sum(float(x) * x for x in seg) / n) ** 0.5 / 32768.0
        frames.append(rms > rms_gate)
    runs, start, quiet = [], None, 0
    gap_frames = max(1, gap_ms // frame_ms)
    for i, voiced in enumerate(frames):
        if voiced:
            if start is None:
                start = i
            quiet = 0
        elif start is not None:
            quiet += 1
            if quiet >= gap_frames:
                runs.append((start, i - quiet + 1))
                start, quiet = None, 0
    if start is not None:
        runs.append((start, len(frames) - quiet))
    out = []
    pad = pad_ms // frame_ms
    for a, b in runs:
        if (b - a) * frame_ms < min_ms:
            continue
        a, b = max(0, a - pad), min(len(frames), b + pad)
        piece = max_ms // frame_ms
        for s in range(a, b, piece):
            e = min(b, s + piece)
            if (e - s) * frame_ms >= min_ms:
                out.append((s * n, min(len(samples), e * n)))
    return out


def next_index(directory, prefix="audition_"):
    """Next free integer for <prefix>NNNN.wav in `directory`."""
    best = 0
    d = Path(directory)
    if d.exists():
        for f in d.glob(f"{prefix}*.wav"):
            try:
                best = max(best, int(f.stem[len(prefix):]))
            except ValueError:
                pass
    return best + 1


def plan_move(src, decision, base=BASE):
    """Destination path for `decision` ('p'|'n'); None for anything else (skip/replay)."""
    kind = DECISIONS.get(decision)
    if kind is None:
        return None
    dest_dir = Path(base) / kind
    return dest_dir / f"audition_{next_index(dest_dir):04d}.wav"


def apply_decision(src, decision, base=BASE, log_path=None, now=None):
    """Move `src` per the decision and log it. Returns the destination or None."""
    dest = plan_move(src, decision, base)
    if dest is None:
        return None
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(src), str(dest))
    log = Path(log_path) if log_path else Path(base) / "audition_log.jsonl"
    with open(log, "a", encoding="utf-8") as fh:
        fh.write(json.dumps({"from": str(src), "to": str(dest), "decision": DECISIONS[decision],
                             "ts": int(now if now is not None else time.time())}) + "\n")
    return dest


def stats(base=BASE):
    base = Path(base)
    counts = {k: len(list((base / k).glob("*.wav"))) for k in ("positive", "negative", "background", "review", "free")
              if (base / k).exists()}
    audited = 0
    log = base / "audition_log.jsonl"
    if log.exists():
        audited = sum(1 for l in log.read_text(encoding="utf-8").splitlines() if l.strip())
    return counts, audited


# ─── commands ────────────────────────────────────────────────────────────

def cmd_segment(args):
    src, out = Path(args.src), Path(args.out)
    files = sorted(src.glob("*.wav"))
    if not files:
        print(f"no .wav files in {src}")
        return 1
    n_out = next_index(out, "seg_")
    total = 0
    for f in files:
        sr, a = read_wav(f)
        for s, e in split_utterances(a, sr):
            write_wav(out / f"seg_{n_out:04d}.wav", a[s:e], sr)
            with open(out / "index.jsonl", "a", encoding="utf-8") as fh:
                fh.write(json.dumps({"seg": f"seg_{n_out:04d}.wav", "source": f.name,
                                     "start_s": round(s / sr, 2), "end_s": round(e / sr, 2)}) + "\n")
            n_out += 1
            total += 1
    print(f"wrote {total} segments from {len(files)} files to {out}. Now: wake_audition.py review --dir {out}")
    return 0


def _play(path):
    import numpy as np
    import sounddevice as sd  # lazy: only the reviewer needs it
    sr, a = read_wav(path)
    sd.play(np.frombuffer(a, dtype=np.int16), sr)  # a list becomes int64, which sounddevice rejects
    sd.wait()


def cmd_review(args):
    if args.queue:
        return cmd_review_queue(args)
    d = Path(args.dir)
    files = sorted(f for f in d.glob("*.wav") if fnmatch(f.name, args.pattern))
    if not files:
        print(f"nothing to review in {d} matching {args.pattern!r}")
        return 1
    print(f"{len(files)} clips. [p]ositive = the speaker clearly says NEXUS  [n]egative = anything else  "
          "[r]eplay  [s]kip  [q]uit")
    for i, f in enumerate(files, 1):
        while True:
            print(f"[{i}/{len(files)}] {f.name}")
            _play(f)
            ans = (input("  p / n / r / s / q > ").strip().lower() or "r")[:1]
            if ans == "r":
                continue
            if ans == "q":
                return 0
            if ans == "s":
                break
            dest = apply_decision(f, ans, BASE)
            if dest:
                print(f"  -> {dest.parent.name}/{dest.name}")
                break
    return 0


QUEUE_KEYS = {"p": "positive", "n": "negative", "k": "keep", "x": "exclude"}


def queue_decision_dest(label, decision, base=BASE):
    """Where a queue decision moves the file: None = stay put (keep), quarantine/ for exclude, else the
    class folder. A decision equal to the current label also stays put."""
    if decision == "keep" or decision == label:
        return None
    if decision == "exclude":
        return Path(base) / "quarantine"
    return Path(base) / decision


def cmd_review_queue(args):
    import wake_label_audit as A
    base = BASE
    qpath = Path(args.queue)
    flags = A.load_flags(qpath)
    ledger_path = qpath.parent / A.LEDGER_NAME
    todo = A.unreviewed(flags, A.load_ledger(ledger_path))
    if not todo:
        print("nothing left to review in the queue.")
        return 0
    print(f"{len(todo)} flagged clips, worst first.  [p]ositive (clearly says NEXUS)  [n]egative (anything else - a real "
          "soundalike is a GOOD negative)  [k]eep current label  [x] exclude from training  [r]eplay  [s]kip  [q]uit")
    for i, fl in enumerate(todo, 1):
        f = qpath.parent / fl["path"]
        if not f.exists():
            continue
        while True:
            score = "n/a" if fl["oof"] is None else f'{fl["oof"]:.2f}'
            print(f"[{i}/{len(todo)}] {fl['path']}  label={fl['label']}  model says P(NEXUS)={score}  "
                  f"origin={fl['provenance']}  why={','.join(fl['reasons'])}")
            _play(f)
            ans = (input("  p / n / k / x / r / s / q > ").strip().lower() or "r")[:1]
            if ans == "r":
                continue
            if ans == "q":
                return 0
            if ans == "s":
                break
            decision = QUEUE_KEYS.get(ans)
            if decision is None:
                continue
            dest_dir = queue_decision_dest(fl["label"], decision, qpath.parent)
            if dest_dir is not None:
                dest_dir.mkdir(parents=True, exist_ok=True)
                dest = dest_dir / f"audit_{next_index(dest_dir, 'audit_'):04d}.wav"
                shutil.move(str(f), str(dest))
                # the content hash is unchanged by the move, so the ledger entry still matches
            A.record_decision(ledger_path, fl["sha"], decision, fl["path"])
            print(f"  -> {decision}")
            break
    return 0


def cmd_stats(args):
    counts, audited = stats()
    for k, v in counts.items():
        print(f"  {k:<11} {v}")
    print(f"  audited by ear: {audited}")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("segment")
    p.add_argument("--src", default=str(BASE / "free"))
    p.add_argument("--out", default=str(BASE / "review"))
    p.set_defaults(fn=cmd_segment)
    p = sub.add_parser("review")
    p.add_argument("--dir", default=str(BASE / "review"))
    p.add_argument("--pattern", default="*")
    p.add_argument("--queue", default=None, help="audit_flags.jsonl from wake_label_audit.py score")
    p.set_defaults(fn=cmd_review)
    sub.add_parser("stats").set_defaults(fn=cmd_stats)
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
