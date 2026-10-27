#!/usr/bin/env python3
"""Find mislabeled wake-word training clips WITHOUT asking Whisper (Phase E1).

Why this exists: every earlier label gate transcribed the clip with Whisper and trusted the text. Whisper
hears short "NEXUS" as "next / texas / access / nixes", so genuine positives were moved into negative/ (345
`soundalike_from_pos_*`, 136 `soundalike_from_rec_*`) - the model was taught that the owner's real voice is a
negative. This tool uses the *audio itself*:

  inventory   what is in positive/ negative/ background/, by origin (owner mic / TTS / DSP-synth / suspect
              relabel), duplicates, too-short, near-silent                        -> audit_inventory.json
  score       out-of-fold probabilities (cleanlab-style confident learning): a clip is scored by a model that
              NEVER saw it or its near-duplicates, so a wrong label cannot hide behind memorisation. Flags
              negatives that sound like NEXUS and positives that do not                -> audit_flags.jsonl
  status      how many flagged clips still need a human decision; exit 1 while any do (the trainer gate)
  review      (in wake_audition.py)  `review --queue audit_flags.jsonl` plays the worst clips first

Decisions are recorded by file CONTENT hash in audit_ledger.jsonl, so moving or renaming a file never loses
a review: p = positive, n = negative (a real soundalike that scores high is the BEST hard negative - keep it),
k = label confirmed, x = exclude from training (moved to quarantine/).

The scoring classifier is a deliberately simple screening model (regularised logistic regression on
clip-level embedding statistics): it only has to rank suspicious clips for a human, not be the product.
"""

import argparse
import hashlib
import json
import re
import sys
import time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))

BASE = Path(__file__).resolve().parent.parent / "wake_word_data"
LEDGER_NAME = "audit_ledger.jsonl"
FLAGS_NAME = "audit_flags.jsonl"
INVENTORY_NAME = "audit_inventory.json"

NEG_HI = 0.5     # a negative this likely to be NEXUS gets a human look
POS_LO = 0.2     # a positive this unlikely to be NEXUS gets a human look
MIN_SECONDS = 0.4
SILENT_RMS = 0.002

# (filename regex, provenance tag). First match wins. `mandatory` tags are always reviewed by ear.
PROVENANCE_RULES = [
    (r"^soundalike_from_pos_", "relabelled_from_positive"),
    (r"^soundalike_from_rec_", "relabelled_from_recording"),
    (r"^(audition_|audit_)", "by_ear"),
    (r"^piper_", "tts_piper"),
    (r"^public_", "public_dataset"),
    (r"^(synth_|neural_pos_|sapi_|edge_)", "tts_other"),
    (r"^(bg_|gen_bg_|bg_orig_|keyboard_burst_|clap_|thump_|sneeze_|cough_|gargle_|throat_clear_|vocal_fry_|fry_|"
     r"agc_stress_|silent_|unintelligible_|shout_|mic_testing_|long_sentence_|non_nexus_)", "dsp_or_tts_negative"),
    (r"^(nexus_|free_|seg_|false_alarm_|missed_|focus_|neg_)", "owner_mic"),
]
MANDATORY_PROVENANCE = {"relabelled_from_positive", "relabelled_from_recording"}
TRUSTED_PROVENANCE = {"by_ear"}


# ─── pure helpers (unit-tested) ──────────────────────────────────────────

def provenance(name):
    for pat, tag in PROVENANCE_RULES:
        if re.match(pat, Path(name).name):
            return tag
    return "unknown"


def file_sha(path):
    return hashlib.sha1(Path(path).read_bytes()).hexdigest()


def cluster_near_duplicates(X, cos=0.995):
    """Cluster id per row: rows whose cosine similarity >= `cos` share an id (union-find). Used as the
    fold group so a clip's duplicates are always held out WITH it."""
    n = len(X)
    if n == 0:
        return np.zeros(0, dtype=int)
    Z = np.asarray(X, dtype=np.float64)
    Z = Z - Z.mean(axis=0, keepdims=True)
    norms = np.linalg.norm(Z, axis=1, keepdims=True)
    Z = Z / np.maximum(norms, 1e-12)
    parent = list(range(n))

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for start in range(0, n, 512):
        sims = Z[start:start + 512] @ Z.T
        for r in range(sims.shape[0]):
            i = start + r
            for j in np.nonzero(sims[r] >= cos)[0]:
                if j > i:
                    a, b = find(i), find(int(j))
                    if a != b:
                        parent[b] = a
    roots = {}
    return np.array([roots.setdefault(find(i), len(roots)) for i in range(n)], dtype=int)


def _fit_logreg(X, y, l2, w_pos):
    from scipy.optimize import minimize
    n, d = X.shape
    sw = np.where(y == 1, w_pos, 1.0)

    def fun(theta):
        w, b = theta[:d], theta[d]
        z = X @ w + b
        # stable log(1+exp(z)) - y*z
        loss = np.sum(sw * (np.logaddexp(0.0, z) - y * z)) / n + 0.5 * l2 * np.dot(w, w)
        p = 1.0 / (1.0 + np.exp(-np.clip(z, -30, 30)))
        g = sw * (p - y)
        return loss, np.concatenate([X.T @ g / n + l2 * w, [g.sum() / n]])

    res = minimize(fun, np.zeros(d + 1), jac=True, method="L-BFGS-B", options={"maxiter": 200})
    return res.x[:d], res.x[d]


def oof_scores(X, y, groups, k=5, seed=0, l2=1e-2):
    """Out-of-fold P(positive) per row. Folds are by `groups` (never splits a group); each fold's rows are
    scored by a model fit on the other folds. If a fold's training side lacks a class the row gets NaN."""
    X = np.asarray(X, dtype=np.float64)
    y = np.asarray(y, dtype=np.float64)
    groups = np.asarray(groups)
    uniq = np.unique(groups)
    rng = np.random.default_rng(seed)
    order = rng.permutation(len(uniq))
    fold_of_group = {int(uniq[g]): i % k for i, g in enumerate(order)}
    folds = np.array([fold_of_group[int(g)] for g in groups])
    out = np.full(len(y), np.nan)
    for f in range(k):
        te = folds == f
        tr = ~te
        if te.sum() == 0 or len(np.unique(y[tr])) < 2:
            continue
        mu, sd = X[tr].mean(axis=0), X[tr].std(axis=0) + 1e-6
        w_pos = float(np.sum(y[tr] == 0)) / max(1.0, float(np.sum(y[tr] == 1)))     # balance the classes
        w, b = _fit_logreg((X[tr] - mu) / sd, y[tr], l2, w_pos)
        z = ((X[te] - mu) / sd) @ w + b
        out[te] = 1.0 / (1.0 + np.exp(-np.clip(z, -30, 30)))
    return out


def make_flags(rows, neg_hi=NEG_HI, pos_lo=POS_LO):
    """rows: dicts with sha, path, label ('positive'|'negative'|'background'), provenance, oof (float|None),
    and optionally duration_s, rms. Returns flags, worst first."""
    flags = []
    for r in rows:
        reasons = []
        sev = 0.0
        oof = r.get("oof")
        has = oof is not None and not (isinstance(oof, float) and np.isnan(oof))
        if r["provenance"] in TRUSTED_PROVENANCE:
            continue
        if r["provenance"] in MANDATORY_PROVENANCE:
            reasons.append("suspect_origin")
            sev = max(sev, 0.5 + (oof if has else 0.0) / 2)
        if has and r["label"] != "positive" and oof >= neg_hi:
            reasons.append("negative_sounds_like_nexus")
            sev = max(sev, oof)
        if has and r["label"] == "positive" and oof <= pos_lo:
            reasons.append("positive_does_not_sound_like_nexus")
            sev = max(sev, 1.0 - oof)
        if r.get("duration_s") is not None and r["duration_s"] < MIN_SECONDS and r["label"] == "positive":
            reasons.append("too_short")
            sev = max(sev, 0.9)
        if r.get("rms") is not None and r["rms"] < SILENT_RMS and r["label"] == "positive":
            reasons.append("near_silent")
            sev = max(sev, 0.95)
        if reasons:
            flags.append({"sha": r["sha"], "path": r["path"], "label": r["label"], "provenance": r["provenance"],
                          "oof": None if not has else round(float(oof), 4), "severity": round(sev, 4),
                          "reasons": reasons})
    flags.sort(key=lambda f: (-f["severity"], f["path"]))
    return flags


# ─── ledger ──────────────────────────────────────────────────────────────

def load_ledger(path):
    """{sha: decision} - last entry for a sha wins."""
    p = Path(path)
    out = {}
    if p.exists():
        for line in p.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line:
                e = json.loads(line)
                out[e["sha"]] = e["decision"]
    return out


def record_decision(ledger_path, sha, decision, path="", now=None):
    assert decision in ("positive", "negative", "keep", "exclude"), decision
    with open(ledger_path, "a", encoding="utf-8") as fh:
        fh.write(json.dumps({"sha": sha, "decision": decision, "path": str(path),
                             "ts": int(now if now is not None else time.time())}) + "\n")


def load_flags(path):
    p = Path(path)
    if not p.exists():
        return []
    return [json.loads(l) for l in p.read_text(encoding="utf-8").splitlines() if l.strip()]


def unreviewed(flags, ledger):
    return [f for f in flags if f["sha"] not in ledger]


def gate(base=BASE):
    """(ok, message) - the trainer calls this. Missing flags file = the audit never ran."""
    base = Path(base)
    fp = base / FLAGS_NAME
    if not fp.exists():
        return False, f"{fp.name} not found - run `nexus wake audit` first (it finds mislabeled clips)."
    todo = unreviewed(load_flags(fp), load_ledger(base / LEDGER_NAME))
    if todo:
        return False, (f"{len(todo)} flagged clip(s) still need a decision by ear. "
                       f"Run: python scripts/wake_audition.py review --queue {fp}")
    return True, "audit clean: every flagged clip has been reviewed"


# ─── commands ────────────────────────────────────────────────────────────

LABEL_DIRS = (("positive", "positive"), ("negative", "negative"), ("background", "background"))


def _scan(base):
    for label, d in LABEL_DIRS:
        for f in sorted((Path(base) / d).glob("*.wav")):
            yield label, f


def _basic_stats(path):
    from wake_audition import read_wav  # stdlib reader
    sr, a = read_wav(path)
    n = len(a)
    if n == 0:
        return 0.0, 0.0
    arr = np.asarray(a, dtype=np.float64) / 32768.0
    return n / sr, float(np.sqrt(np.mean(arr ** 2)))


def cmd_inventory(args):
    base = Path(args.base)
    counts, items, by_sha = {}, [], {}
    for label, f in _scan(base):
        prov = provenance(f.name)
        dur, rms = _basic_stats(f)
        sha = file_sha(f)
        counts.setdefault(label, {}).setdefault(prov, 0)
        counts[label][prov] += 1
        items.append({"path": f.relative_to(base).as_posix(), "label": label, "provenance": prov, "sha": sha,
                      "duration_s": round(dur, 3), "rms": round(rms, 5)})
        by_sha.setdefault(sha, []).append(f.relative_to(base).as_posix())
    dup = {s: p for s, p in by_sha.items() if len(p) > 1}
    report = {"counts": counts, "exact_duplicates": dup,
              "too_short_positive": [i["path"] for i in items if i["label"] == "positive" and i["duration_s"] < MIN_SECONDS],
              "near_silent_positive": [i["path"] for i in items if i["label"] == "positive" and i["rms"] < SILENT_RMS],
              "suspect_negatives": [i["path"] for i in items if i["provenance"] in MANDATORY_PROVENANCE],
              "unknown_origin": [i["path"] for i in items if i["provenance"] == "unknown"]}
    (base / INVENTORY_NAME).write_text(json.dumps(report, indent=2), encoding="utf-8")
    print("clips by label / origin:")
    for label, tags in counts.items():
        print(f"  {label}: " + ", ".join(f"{t}={n}" for t, n in sorted(tags.items())))
    print(f"exact duplicates: {len(dup)} groups | too short positives: {len(report['too_short_positive'])} | "
          f"near-silent positives: {len(report['near_silent_positive'])}")
    print(f"MUST be reviewed by ear (relabelled by the old Whisper gate): {len(report['suspect_negatives'])}")
    print(f"wrote {base / INVENTORY_NAME}")
    return 0


def clip_descriptor(windows):
    """96-d mean + 96-d newest-frame max over a clip's [16,96] windows; None if the clip has no voiced chunk."""
    if not windows:
        return None
    W = np.asarray(windows, dtype=np.float64)           # [n, 16, 96]
    return np.concatenate([W.mean(axis=(0, 1)), W[:, -1, :].max(axis=0)])


def cmd_score(args):
    import onnxruntime as ort
    import wake_train_lib as L
    base = Path(args.base)
    mel = ort.InferenceSession(str(L.OWW_DIR / "melspectrogram.onnx"))
    emb = ort.InferenceSession(str(L.OWW_DIR / "embedding_model.onnx"))
    fe = L.Frontend(mel, emb)
    ledger = load_ledger(base / LEDGER_NAME)
    meta, feats = [], []
    files = list(_scan(base))
    print(f"embedding {len(files)} clips (no Whisper, no labels used for features)...", flush=True)
    for i, (label, f) in enumerate(files, 1):
        audio = L.load_wav_16k(f)
        d = clip_descriptor(fe.windows(audio, include_transitions=True))
        dur, rms = len(audio) / L.SR, float(np.sqrt(np.mean(audio.astype(np.float64) ** 2))) if len(audio) else 0.0
        meta.append({"sha": file_sha(f), "path": f.relative_to(base).as_posix(), "label": label,
                     "provenance": provenance(f.name), "duration_s": dur, "rms": rms})
        feats.append(d)
        if i % 200 == 0:
            print(f"  {i}/{len(files)}", flush=True)
    ok = [i for i, d in enumerate(feats) if d is not None]
    X = np.stack([feats[i] for i in ok])
    y = np.array([1 if meta[i]["label"] == "positive" else 0 for i in ok])
    groups = cluster_near_duplicates(X, cos=args.dup_cos)
    print(f"{len(ok)} clips with speech; {len(set(groups.tolist()))} duplicate-groups; scoring out-of-fold...", flush=True)
    oof = oof_scores(X, y, groups, k=args.folds, seed=args.seed)
    for j, i in enumerate(ok):
        meta[i]["oof"] = float(oof[j]) if not np.isnan(oof[j]) else None
    for i, d in enumerate(feats):
        if d is None:
            meta[i]["oof"] = None
            meta[i]["no_speech"] = True
    flags = make_flags(meta, args.neg_hi, args.pos_lo)
    flags = [f for f in flags if f["sha"] not in ledger]          # reviewed clips do not come back
    (base / FLAGS_NAME).write_text("".join(json.dumps(f) + "\n" for f in flags), encoding="utf-8")
    (base / "audit_scores.jsonl").write_text("".join(json.dumps(m) + "\n" for m in meta), encoding="utf-8")
    n_sus = sum(1 for f in flags if "suspect_origin" in f["reasons"])
    print(f"flagged {len(flags)} clip(s) for review ({n_sus} from the old Whisper relabel, "
          f"{sum(1 for f in flags if 'negative_sounds_like_nexus' in f['reasons'])} negatives that sound like NEXUS, "
          f"{sum(1 for f in flags if 'positive_does_not_sound_like_nexus' in f['reasons'])} positives that do not).")
    print(f"review them (worst first): python scripts/wake_audition.py review --queue {base / FLAGS_NAME}")
    return 0


def cmd_status(args):
    ok, msg = gate(args.base)
    print(msg)
    return 0 if ok else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base", default=str(BASE))
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("inventory").set_defaults(fn=cmd_inventory)
    p = sub.add_parser("score")
    p.add_argument("--folds", type=int, default=5)
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--dup-cos", type=float, default=0.995)
    p.add_argument("--neg-hi", type=float, default=NEG_HI)
    p.add_argument("--pos-lo", type=float, default=POS_LO)
    p.set_defaults(fn=cmd_score)
    sub.add_parser("status").set_defaults(fn=cmd_status)
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
