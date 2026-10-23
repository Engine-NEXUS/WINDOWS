#!/usr/bin/env python3
"""
NEXUS wake-word classifier trainer (focal loss, hard-negative oversampling).

What changed in the voice-accuracy program (B7 / C1) - numbers from the old trainer were optimistic
because validation shared recordings with training and "best epoch" was just the last one:

  * Front end = the SHARED spec (`dsp_spec.json`, 'aligned'): the same high-pass / gate / AGC the runtime
    uses with wakeDspProfile=aligned, so serving matches training (see wake_dsp.py, wake_train_lib.py).
  * Splits are by recording / manifest source group (train 70 / val 15 / test 15, deterministic by hash),
    never by window. Augmentation is applied to TRAIN only; val and test are clean recordings.
  * `best_weights` is a real deep copy of the best epoch (it used to alias the live weights).
  * Everything is seeded (`--seed`).
  * Recall / false-alarm are reported at the DEPLOYED threshold (acoustic_profile.json kws_threshold) and at
    0.50, on val every epoch and on the untouched test split once at the end.
  * New augmentations: speed +-8%, reverb, random start shift, random gain, real background noise at
    0-20 dB SNR (plus the previous telecom / fan-hum / quiet-speech ones).
  * The result is written to `nexus.candidate.onnx` + `train_report.json`; the shipped `nexus.onnx` is only
    replaced with --promote (the old backup is kept as nexus.previous.onnx).

Phase E (data clean-up + reusable data):
  * LABEL GATE: training refuses to run until `wake_label_audit.py` has scored the data and every flagged clip
    (negatives that sound like NEXUS, positives that do not, the old Whisper "soundalike_from_*" relabels)
    has a by-ear decision. --skip-audit-gate overrides and is recorded in train_report.json. --dry-run only warns.
  * PUBLIC DATA: wake_word_data/public/{negative,background} (wake_data_fetch.py prepare) joins the pools;
    synthetic TTS positives (wake_data_synth.py) live in positive/ like any other positive. Public long
    recordings in public/heldout/ are NEVER read here - they measure false alarms per hour.
  * OWNER DATA: owner recordings get full augmentation and --owner-repeat weight; a stable --seal-owner
    fraction of them is withheld from every split and listed in sealed/manifest.jsonl for `nexus wake score`.
    Synthetic / public clips get lighter augmentation and per-file window caps.
  * HARD-NEGATIVE MINING: after the first fit, the unused negative pool (and extra background) is STREAMED
    through the model; the windows it fires on are added (repeated) and the model is fine-tuned, --mine-rounds times.

Usage:
    python scripts/train_local_wakeword.py --dry-run            # counts + split check, no torch needed
    python scripts/train_local_wakeword.py                      # train, write candidate
    python scripts/train_local_wakeword.py --promote            # ...and replace the shipped model

Data layout (wake_word_data/, git-ignored): positive/ negative/ background/ [manifest.jsonl].
Held-out evaluation recordings (eval_set/) are NEVER read here.
"""

import argparse
import copy
import glob
import json
import shutil
import sys
from pathlib import Path

import numpy as np

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", line_buffering=True)
    sys.stderr.reconfigure(encoding="utf-8", errors="replace", line_buffering=True)

sys.path.insert(0, str(Path(__file__).resolve().parent))
import wake_label_audit as A  # noqa: E402
import wake_train_lib as L  # noqa: E402

ROOT = L.ROOT
OWW_DIR = L.OWW_DIR
DATA_DIR = ROOT / "wake_word_data"


def torch_parts():
    """Import torch lazily so --dry-run and the unit tests do not need it."""
    import torch
    import torch.nn as nn

    class OwwClassifier(nn.Module):
        def __init__(self, input_dim=16 * 96, hidden_dim=128):
            super().__init__()
            self.layer1 = nn.Linear(input_dim, hidden_dim)
            self.layernorm1 = nn.LayerNorm(hidden_dim)
            self.relu1 = nn.ReLU()
            self.layer2 = nn.Linear(hidden_dim, hidden_dim)
            self.layernorm2 = nn.LayerNorm(hidden_dim)
            self.relu2 = nn.ReLU()
            self.last_layer = nn.Linear(hidden_dim, 1)
            nn.init.constant_(self.last_layer.bias, -4.0)

        def forward(self, x):
            v = x.view(x.shape[0], -1)
            h = self.relu1(self.layernorm1(self.layer1(v)))
            h = self.relu2(self.layernorm2(self.layer2(h)))
            return self.last_layer(h)

    class BinaryFocalLoss(nn.Module):
        def __init__(self, gamma=2.0, pos_weight=1.5):
            super().__init__()
            self.gamma, self.pos_weight = gamma, pos_weight

        def forward(self, logits, targets):
            p = torch.clamp(torch.sigmoid(logits), 1e-7, 1.0 - 1e-7)
            loss_pos = -self.pos_weight * ((1.0 - p) ** self.gamma) * torch.log(p)
            loss_neg = -(p ** self.gamma) * torch.log(1.0 - p)
            return (targets * loss_pos + (1.0 - targets) * loss_neg).mean()

    class SigmoidExportWrapper(nn.Module):
        def __init__(self, base):
            super().__init__()
            self.base = base

        def forward(self, x):
            return torch.sigmoid(self.base(x))

    return torch, nn, OwwClassifier, BinaryFocalLoss, SigmoidExportWrapper


def butter_bandpass(lowcut, highcut, fs=L.SR, order=3):
    from scipy.signal import butter
    nyq = 0.5 * fs
    return butter(order, [max(lowcut / nyq, 0.01), min(highcut / nyq, 0.99)], btype="band")


def positives_from(fe, audio, rng, noise_pool, train, level="full"):
    """Windows for one positive recording. Val/test get the clean recording only.

    level 'full' = the owner's real takes (every augmentation); 'lite' = synthetic / public clips, which are
    already varied by construction (base + two random augmentations) so thousands of them stay affordable."""
    take = lambda w, k: w[-min(len(w), k):]  # noqa: E731 - last k windows = word end + transitions
    out = []
    w = fe.windows(audio, include_transitions=True)
    out += take(w, 5)
    if not train:
        return out
    if level == "lite":
        fns = [L.aug_speed, L.aug_reverb, L.aug_shift, L.aug_gain]
        for i in rng.permutation(len(fns))[:2]:
            out += take(fe.windows(fns[int(i)](audio, rng), include_transitions=True), 3)
        if len(noise_pool):
            noise = noise_pool[int(rng.integers(0, len(noise_pool)))]
            out += take(fe.windows(L.mix_noise(audio, noise, float(rng.uniform(0.0, 20.0)), rng), include_transitions=True), 3)
        return out
    from scipy.signal import lfilter
    b, a = butter_bandpass(300, 3400)
    out += take(fe.windows(lfilter(b, a, audio).astype(np.float32), include_transitions=True), 4)   # telecom
    t = np.arange(len(audio)) / L.SR
    for fund in (113.3, 72.0):                                                                     # fan hum
        hum = (0.020 * np.sin(2 * np.pi * fund * t)).astype(np.float32)
        out += take(fe.windows(np.clip(audio * 0.70 + hum, -1, 1), True, highpass_hz=128.0), 3)
    out += take(fe.windows(audio * 0.35, include_transitions=True), 3)                              # quiet speech
    for fn in (L.aug_speed, L.aug_reverb, L.aug_shift, L.aug_gain):
        out += take(fe.windows(fn(audio, rng), include_transitions=True), 3)
    if len(noise_pool):
        for _ in range(2):                                                                         # real noise, 0-20 dB
            noise = noise_pool[int(rng.integers(0, len(noise_pool)))]
            snr = float(rng.uniform(0.0, 20.0))
            out += take(fe.windows(L.mix_noise(audio, noise, snr, rng), include_transitions=True), 3)
    return out


OWNER_ORIGINS = {"owner_mic", "by_ear"}


def collect_positives(fe, files, rng, manifest, noise_pool, args):
    """({split: [windows]}, {split: n_files}, owner_windows {split: [..]}, sealed [files])."""
    groups = {f: L.group_key(f, manifest) for f in files}
    splits = L.assign_splits(set(groups.values()), seed=args.seed)
    buckets = {"train": [], "val": [], "test": []}
    owner = {"train": [], "val": [], "test": []}
    counts = {"train": 0, "val": 0, "test": 0}
    sealed = []
    for f in files:
        prov = A.provenance(Path(f).name)
        is_owner = prov in OWNER_ORIGINS
        if is_owner and L.is_sealed(groups[f], args.seal_owner, args.seed):
            sealed.append(f)
            continue
        sp = splits[groups[f]]
        counts[sp] += 1
        audio = L.load_wav_16k(f)
        w = positives_from(fe, audio, rng, noise_pool, train=(sp == "train"), level="full" if is_owner else "lite")
        (owner if is_owner else buckets)[sp] += w
    return buckets, counts, owner, sealed


def collect_negatives(fe, files, rng, manifest, args, per_file_cap=None):
    """{split: [windows]}, {split: n_files} for negative speech/sound files, assigned by recording/group."""
    groups = {f: L.group_key(f, manifest) for f in files}
    splits = L.assign_splits(set(groups.values()), seed=args.seed)
    buckets = {"train": [], "val": [], "test": []}
    counts = {"train": 0, "val": 0, "test": 0}
    for f in files:
        sp = splits[groups[f]]
        counts[sp] += 1
        audio = L.load_wav_16k(f)
        w = fe.windows(audio, include_transitions=True)
        if per_file_cap and len(w) > per_file_cap:
            w = L.cap_list(w, per_file_cap, rng)
        buckets[sp] += w
        if sp == "train" and len(w) and A.provenance(Path(f).name) != "public_dataset":
            # one gain-perturbed copy so soundalikes are not memorised at one level
            buckets[sp] += fe.windows(L.aug_gain(audio, rng), include_transitions=True)
    return buckets, counts


def write_sealed_manifest(data, sealed):
    """Owner recordings withheld from training, as a wake_eval manifest (kind 'wake')."""
    out = Path(data) / "sealed"
    out.mkdir(parents=True, exist_ok=True)
    rows = [{"audio": f"positive/{Path(f).name}", "kind": "wake", "phrase": "owner_sealed", "speaker_id": "owner",
             "source": "owner_sealed"} for f in sorted(sealed)]
    (out / "manifest.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
    return len(rows)


def discover(data, public):
    """File lists for the three classes (public pools appended when present)."""
    data = Path(data)
    pos = sorted(glob.glob(str(data / "positive" / "*.wav")))
    neg = sorted(glob.glob(str(data / "negative" / "*.wav")))
    bg = sorted(glob.glob(str(data / "background" / "*.wav")))
    if public:
        pub = Path(public)
        neg += sorted(glob.glob(str(pub / "negative" / "*.wav")))
        bg += sorted(glob.glob(str(pub / "background" / "*.wav")))
    return pos, neg, bg


def load_manifests(args, data):
    rows = {}
    for p in (data / "manifest.jsonl", data / "synth" / "manifest.jsonl"):
        rows.update(L.load_manifest_rows(args.manifest if (args.manifest and p == data / "manifest.jsonl") else p))
    return rows


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--data-dir", default=str(DATA_DIR))
    ap.add_argument("--public-dir", default=None, help="public pools (default <data-dir>/public if it exists)")
    ap.add_argument("--no-public", action="store_true")
    ap.add_argument("--manifest", default=None, help="manifest.jsonl (default <data-dir>/manifest.jsonl)")
    ap.add_argument("--epochs", type=int, default=45)
    ap.add_argument("--seed", type=int, default=1337)
    ap.add_argument("--threshold", type=float, default=None, help="deployed threshold (default: acoustic_profile.json)")
    ap.add_argument("--max-background", type=int, default=600)
    ap.add_argument("--max-neg-files", type=int, default=6000, help="negative files in the first fit; the rest is the mining pool")
    ap.add_argument("--public-windows-per-file", type=int, default=4)
    ap.add_argument("--owner-repeat", type=int, default=3, help="owner positives are repeated this many times in training")
    ap.add_argument("--seal-owner", type=float, default=0.3, help="fraction of owner positives withheld for scoring")
    ap.add_argument("--mine-rounds", type=int, default=2, help="hard-negative mining rounds (0 = off)")
    ap.add_argument("--mine-epochs", type=int, default=12)
    ap.add_argument("--mine-floor", type=float, default=0.2, help="a pool window above this score counts as hard")
    ap.add_argument("--mine-cap", type=int, default=20000)
    ap.add_argument("--hard-neg-repeat", type=int, default=3)
    ap.add_argument("--out", default=str(OWW_DIR / "nexus.candidate.onnx"))
    ap.add_argument("--promote", action="store_true", help="replace the shipped nexus.onnx with the candidate")
    ap.add_argument("--dry-run", action="store_true", help="extract + split + report counts; no training, no torch")
    ap.add_argument("--skip-audit-gate", action="store_true", help="train even though flagged clips are unreviewed (recorded)")
    args = ap.parse_args(argv)

    data = Path(args.data_dir)
    public = None if args.no_public else (Path(args.public_dir) if args.public_dir else (data / "public" if (data / "public").exists() else None))
    rng = L.seeded(args.seed)
    thr = args.threshold if args.threshold is not None else L.deployed_threshold()
    print("=" * 70)
    print("  NEXUS WAKE-WORD TRAINER  (shared front end: dsp_spec.json 'aligned')")
    print(f"  seed={args.seed}  deployed threshold={thr:.2f}  data={data}  public={'yes' if public else 'no'}")
    print("=" * 70, flush=True)

    gate_ok, gate_msg = A.gate(data)
    print(("AUDIT GATE: " if gate_ok else "AUDIT GATE FAILED: ") + gate_msg, flush=True)
    if not gate_ok and not args.dry_run and not args.skip_audit_gate:
        sys.exit("Refusing to train on unaudited labels (a soundalike filed as a negative, or a real NEXUS filed "
                 "as a negative, poisons the model). Fix: nexus wake audit  -> review the queue by ear.  "
                 "Override: --skip-audit-gate")

    import onnxruntime as ort
    mel = ort.InferenceSession(str(OWW_DIR / "melspectrogram.onnx"))
    emb = ort.InferenceSession(str(OWW_DIR / "embedding_model.onnx"))
    fe = L.Frontend(mel, emb)
    manifest = load_manifests(args, data)
    print(f"manifest rows: {len(manifest) // 2 if manifest else 0}", flush=True)

    pos_files, neg_files, bg_files = discover(data, public)
    bg_sel = [bg_files[i] for i in rng.permutation(len(bg_files))[: args.max_background]] if bg_files else []
    noise_pool = [L.load_wav_16k(p) for p in bg_sel[:40]]

    pos, pc, owner_pos, sealed = collect_positives(fe, pos_files, rng, manifest, noise_pool, args)
    n_sealed = write_sealed_manifest(data, sealed)
    order = rng.permutation(len(neg_files))
    first = [neg_files[i] for i in order[: args.max_neg_files]]
    pool_files = [neg_files[i] for i in order[args.max_neg_files:]]
    sneg, sc = collect_negatives(fe, first, rng, manifest, args, per_file_cap=args.public_windows_per_file)
    noise_buckets = {"train": [], "val": [], "test": []}
    bsplits = L.assign_splits({Path(p).name for p in bg_sel}, seed=args.seed)
    for p in bg_sel:
        w = fe.windows(L.load_wav_16k(p), include_transitions=True)
        noise_buckets[bsplits[Path(p).name]] += L.cap_list(w, 12, rng)

    comfort = np.stack([fe.comfort.copy() for _ in range(L.EMB_FRAMES)])
    for sp, n in (("train", 1000), ("val", 150), ("test", 150)):
        noise_buckets[sp] += [comfort + rng.normal(0.0, 0.005, comfort.shape).astype(np.float32) for _ in range(n)]

    origins = {}
    for f in pos_files:
        origins[A.provenance(Path(f).name)] = origins.get(A.provenance(Path(f).name), 0) + 1
    report = {"seed": args.seed, "threshold": thr, "recordings": {"positive": pc, "speech_negative": sc},
              "positive_origins": origins, "sealed_owner_recordings": n_sealed,
              "negative_pool_files_for_mining": len(pool_files),
              "audit_gate": {"ok": gate_ok, "message": gate_msg, "skipped": bool(not gate_ok and args.skip_audit_gate)},
              "windows": {s: {"pos": len(pos[s]), "owner_pos": len(owner_pos[s]), "speech_neg": len(sneg[s]),
                              "noise_neg": len(noise_buckets[s])} for s in ("train", "val", "test")}}
    print(json.dumps(report["windows"], indent=2), flush=True)
    print(f"sealed owner recordings (never trained on): {n_sealed}  -> {data / 'sealed' / 'manifest.jsonl'}", flush=True)
    for sp in ("train", "val", "test"):
        if not (pos[sp] or owner_pos[sp]):
            sys.exit(f"ERROR: no positive windows in the {sp} split ({pc}). Add more NEXUS clips "
                     f"(the split is by recording/speaker, so each split needs several).")
    if args.dry_run:
        Path(args.out).with_name("train_report.dry.json").write_text(json.dumps(report, indent=2))
        print("dry run complete (nothing trained).")
        return

    def assemble(sp, repeat_owner=1):
        neg_sp, neg_no = sneg[sp], noise_buckets[sp]
        if sp == "train" and neg_sp and neg_no:       # speech negatives must not be swamped by noise
            neg_sp = neg_sp * int(np.ceil(len(neg_no) / len(neg_sp)))
        neg = neg_sp + neg_no
        pw = pos[sp] + owner_pos[sp] * (repeat_owner if sp == "train" else 1)
        X = np.array(pw + neg, dtype=np.float32)
        y = np.concatenate([np.ones((len(pw), 1)), np.zeros((len(neg), 1))]).astype(np.float32)
        return X, y, len(pw)

    torch, nn, OwwClassifier, BinaryFocalLoss, SigmoidExportWrapper = torch_parts()
    from torch.utils.data import DataLoader, TensorDataset
    torch.manual_seed(args.seed)
    Xtr, ytr, _ = assemble("train", args.owner_repeat)
    Xva, yva, nva = assemble("val")
    Xte, yte, nte = assemble("test")
    model = OwwClassifier()
    crit = BinaryFocalLoss(gamma=2.0, pos_weight=2.2)

    def evaluate(X, y, n_pos):
        model.eval()
        with torch.no_grad():
            logits = model(torch.from_numpy(X))
            loss = crit(logits, torch.from_numpy(y)).item()
            probs = torch.sigmoid(logits).numpy().reshape(-1)
        return loss, L.rates_at(probs[:n_pos], probs[n_pos:], thr), L.rates_at(probs[:n_pos], probs[n_pos:], 0.5)

    def fit(X, y, epochs, lr, tag):
        loader = DataLoader(TensorDataset(torch.from_numpy(X), torch.from_numpy(y)), batch_size=64, shuffle=True,
                            generator=torch.Generator().manual_seed(args.seed))
        opt = torch.optim.AdamW(model.parameters(), lr=lr, weight_decay=1e-4)
        sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=epochs)
        best_loss, best_state, best_epoch = float("inf"), None, 0
        for epoch in range(1, epochs + 1):
            model.train()
            total = 0.0
            for bx, by in loader:
                opt.zero_grad()
                loss = crit(model(bx), by)
                loss.backward()
                opt.step()
                total += loss.item() * len(bx)
            sched.step()
            vloss, (rec_d, fa_d), (rec_5, fa_5) = evaluate(Xva, yva, nva)
            if vloss < best_loss:
                best_loss, best_epoch = vloss, epoch
                best_state = copy.deepcopy(model.state_dict())      # a real copy, not an alias
            if epoch % 5 == 0 or epoch == 1:
                print(f"  [{tag}] epoch {epoch:2d}/{epochs} loss {total/len(X):.4f} | val loss {vloss:.4f} | "
                      f"@{thr:.2f}: recall {rec_d:5.1%} FA {fa_d:5.2%} | @0.50: recall {rec_5:5.1%} FA {fa_5:5.2%}", flush=True)
        model.load_state_dict(best_state)
        report.setdefault("fits", []).append({"stage": tag, "epochs": epochs, "best_epoch": best_epoch,
                                              "best_val_loss": round(best_loss, 5)})
        return best_loss, best_epoch

    best_loss, best_epoch = fit(Xtr, ytr, args.epochs, 0.001, "base")
    report["mining"] = []

    train_groups = {f: L.group_key(f, manifest) for f in pool_files}
    train_split = L.assign_splits(set(train_groups.values()), seed=args.seed)
    pool_train = [f for f in pool_files if train_split[train_groups[f]] == "train"]
    bg_pool = [f for f in bg_files if f not in set(bg_sel) and L.assign_splits({Path(f).name}, seed=args.seed)[Path(f).name] == "train"]
    for rnd in range(1, args.mine_rounds + 1):
        stream = pool_train + bg_pool
        if not stream:
            print("no unused negative pool - mining skipped (everything fit in the first set)")
            break
        hard = []
        model.eval()
        scored = 0
        for f in stream:
            wins = L.cap_list(fe.windows(L.load_wav_16k(f), include_transitions=True), args.public_windows_per_file * 3, rng)
            if not wins:
                continue
            with torch.no_grad():
                p = torch.sigmoid(model(torch.from_numpy(np.array(wins, dtype=np.float32)))).numpy().reshape(-1)
            scored += len(wins)
            for i in L.hard_negative_indices(p, args.mine_floor, 8):
                hard.append((float(p[i]), wins[int(i)]))
        hard.sort(key=lambda t: -t[0])
        hard = hard[: args.mine_cap]
        stats = {"round": rnd, "windows_scored": scored, "hard_found": len(hard),
                 "max_score": hard[0][0] if hard else 0.0, "files": len(stream)}
        report["mining"].append(stats)
        print(f"  mining round {rnd}: scored {scored} windows, {len(hard)} above {args.mine_floor:.2f} "
              f"(worst {stats['max_score']:.2f})", flush=True)
        if not hard:
            break
        Hx = np.array([w for _, w in hard] * args.hard_neg_repeat, dtype=np.float32)
        Xtr = np.concatenate([Xtr, Hx])
        ytr = np.concatenate([ytr, np.zeros((len(Hx), 1), np.float32)])
        best_loss, best_epoch = fit(Xtr, ytr, args.mine_epochs, 0.0003, f"mine{rnd}")

    _, (rec_d, fa_d), (rec_5, fa_5) = evaluate(Xte, yte, nte)
    report["best_epoch"] = best_epoch
    report["best_val_loss"] = best_loss
    report["test"] = {"recall_at_deployed": rec_d, "false_alarm_at_deployed": fa_d,
                      "recall_at_0.5": rec_5, "false_alarm_at_0.5": fa_5,
                      "n_pos": nte, "n_neg": len(Xte) - nte}
    print(f"\nTEST split (untouched until now), final stage best epoch {best_epoch}: recall {rec_d:.1%} / FA {fa_d:.2%} "
          f"at {thr:.2f}; recall {rec_5:.1%} / FA {fa_5:.2%} at 0.50", flush=True)
    print("NOTE: these are window-level rates on your own recordings. The real accuracy numbers come from "
          "`nexus wake score` (sealed owner clips, held-out eval_set, hours of public held-out audio).", flush=True)

    model.eval()
    out = Path(args.out)
    export = SigmoidExportWrapper(model).eval()
    torch.onnx.export(export, torch.randn(1, 16, 96), str(out), input_names=["input"], output_names=["output"],
                      dynamic_axes={"input": {0: "batch_size"}, "output": {0: "batch_size"}},
                      opset_version=14, dynamo=False)
    out.with_name("nexus.candidate.onnx.data").unlink(missing_ok=True)
    out.with_name("train_report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"saved candidate: {out}", flush=True)
    if args.promote:
        shipped = OWW_DIR / "nexus.onnx"
        if shipped.exists():
            shutil.copy2(shipped, OWW_DIR / "nexus.previous.onnx")
        shutil.copy2(out, shipped)
        print(f"PROMOTED to {shipped} (old model kept as nexus.previous.onnx). Now run: "
              f"python scripts/wake_data_foundation.py fingerprint-models", flush=True)
    else:
        print("not promoted: score it first (nexus wake score --model <candidate>), then re-run with --promote.", flush=True)


if __name__ == "__main__":
    main()
