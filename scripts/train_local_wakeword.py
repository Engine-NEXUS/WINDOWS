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


def positives_from(fe, audio, rng, noise_pool, train):
    """Windows for one positive recording. Val/test get the clean recording only."""
    take = lambda w, k: w[-min(len(w), k):]  # noqa: E731 - last k windows = word end + transitions
    out = []
    w = fe.windows(audio, include_transitions=True)
    out += take(w, 5)
    if not train:
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


def collect(fe, pattern, label_kind, rng, manifest, noise_pool, args):
    """{split: [windows]} for one class, assigned by recording/group."""
    files = sorted(glob.glob(pattern))
    groups = {f: L.group_key(f, manifest) for f in files}
    splits = L.assign_splits(set(groups.values()), seed=args.seed)
    buckets = {"train": [], "val": [], "test": []}
    counts = {"train": 0, "val": 0, "test": 0}
    for f in files:
        sp = splits[groups[f]]
        counts[sp] += 1
        audio = L.load_wav_16k(f)
        if label_kind == "positive":
            buckets[sp] += positives_from(fe, audio, rng, noise_pool, train=(sp == "train"))
        else:
            w = fe.windows(audio, include_transitions=True)
            buckets[sp] += w
            if sp == "train" and label_kind == "speech_neg" and len(w):
                # one gain/noise-perturbed copy so soundalikes are not memorised at one level
                buckets[sp] += fe.windows(L.aug_gain(audio, rng), include_transitions=True)
    return buckets, counts


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--data-dir", default=str(DATA_DIR))
    ap.add_argument("--manifest", default=None, help="manifest.jsonl (default <data-dir>/manifest.jsonl)")
    ap.add_argument("--epochs", type=int, default=45)
    ap.add_argument("--seed", type=int, default=1337)
    ap.add_argument("--threshold", type=float, default=None, help="deployed threshold (default: acoustic_profile.json)")
    ap.add_argument("--max-background", type=int, default=300)
    ap.add_argument("--out", default=str(OWW_DIR / "nexus.candidate.onnx"))
    ap.add_argument("--promote", action="store_true", help="replace the shipped nexus.onnx with the candidate")
    ap.add_argument("--dry-run", action="store_true", help="extract + split + report counts; no training, no torch")
    args = ap.parse_args()

    data = Path(args.data_dir)
    rng = L.seeded(args.seed)
    thr = args.threshold if args.threshold is not None else L.deployed_threshold()
    print("=" * 70)
    print("  NEXUS WAKE-WORD TRAINER  (shared front end: dsp_spec.json 'aligned')")
    print(f"  seed={args.seed}  deployed threshold={thr:.2f}  data={data}")
    print("=" * 70, flush=True)

    import onnxruntime as ort
    mel = ort.InferenceSession(str(OWW_DIR / "melspectrogram.onnx"))
    emb = ort.InferenceSession(str(OWW_DIR / "embedding_model.onnx"))
    fe = L.Frontend(mel, emb)
    manifest = L.load_manifest_rows(args.manifest or data / "manifest.jsonl")
    print(f"manifest rows: {len(manifest) // 2 if manifest else 0}", flush=True)

    bg_files = sorted(glob.glob(str(data / "background" / "*.wav")))
    bg_sel = [bg_files[i] for i in rng.permutation(len(bg_files))[: args.max_background]] if bg_files else []
    noise_pool = [L.load_wav_16k(p) for p in bg_sel[:40]]

    pos, pc = collect(fe, str(data / "positive" / "*.wav"), "positive", rng, manifest, noise_pool, args)
    sneg, sc = collect(fe, str(data / "negative" / "*.wav"), "speech_neg", rng, manifest, noise_pool, args)
    noise_buckets = {"train": [], "val": [], "test": []}
    bsplits = L.assign_splits({Path(p).name for p in bg_sel}, seed=args.seed)
    for p in bg_sel:
        noise_buckets[bsplits[Path(p).name]] += fe.windows(L.load_wav_16k(p), include_transitions=True)

    comfort = np.stack([fe.comfort.copy() for _ in range(L.EMB_FRAMES)])
    for sp, n in (("train", 1000), ("val", 150), ("test", 150)):
        noise_buckets[sp] += [comfort + rng.normal(0.0, 0.005, comfort.shape).astype(np.float32) for _ in range(n)]

    report = {"seed": args.seed, "threshold": thr, "recordings": {"positive": pc, "speech_negative": sc},
              "windows": {s: {"pos": len(pos[s]), "speech_neg": len(sneg[s]), "noise_neg": len(noise_buckets[s])}
                          for s in ("train", "val", "test")}}
    print(json.dumps(report["windows"], indent=2), flush=True)
    for sp in ("train", "val", "test"):
        if not pos[sp]:
            sys.exit(f"ERROR: no positive windows in the {sp} split ({pc}). Record more NEXUS clips "
                     f"(the split is by recording, so each split needs several).")
    if args.dry_run:
        Path(args.out).with_name("train_report.dry.json").write_text(json.dumps(report, indent=2))
        print("dry run complete (nothing trained).")
        return

    def assemble(sp):
        neg_sp, neg_no = sneg[sp], noise_buckets[sp]
        if sp == "train" and neg_sp and neg_no:       # speech negatives must not be swamped by noise
            neg_sp = neg_sp * int(np.ceil(len(neg_no) / len(neg_sp)))
        neg = neg_sp + neg_no
        X = np.array(pos[sp] + neg, dtype=np.float32)
        y = np.concatenate([np.ones((len(pos[sp]), 1)), np.zeros((len(neg), 1))]).astype(np.float32)
        return X, y, len(pos[sp])

    torch, nn, OwwClassifier, BinaryFocalLoss, SigmoidExportWrapper = torch_parts()
    from torch.utils.data import DataLoader, TensorDataset
    torch.manual_seed(args.seed)
    Xtr, ytr, _ = assemble("train")
    Xva, yva, nva = assemble("val")
    Xte, yte, nte = assemble("test")
    loader = DataLoader(TensorDataset(torch.from_numpy(Xtr), torch.from_numpy(ytr)), batch_size=64, shuffle=True,
                        generator=torch.Generator().manual_seed(args.seed))
    model = OwwClassifier()
    crit = BinaryFocalLoss(gamma=2.0, pos_weight=2.2)
    opt = torch.optim.AdamW(model.parameters(), lr=0.001, weight_decay=1e-4)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=args.epochs)

    def evaluate(X, y, n_pos):
        model.eval()
        with torch.no_grad():
            logits = model(torch.from_numpy(X))
            loss = crit(logits, torch.from_numpy(y)).item()
            probs = torch.sigmoid(logits).numpy().reshape(-1)
        return loss, L.rates_at(probs[:n_pos], probs[n_pos:], thr), L.rates_at(probs[:n_pos], probs[n_pos:], 0.5)

    best_loss, best_state, best_epoch = float("inf"), None, 0
    for epoch in range(1, args.epochs + 1):
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
            print(f"  epoch {epoch:2d}/{args.epochs} loss {total/len(Xtr):.4f} | val loss {vloss:.4f} | "
                  f"@{thr:.2f}: recall {rec_d:5.1%} FA {fa_d:5.2%} | @0.50: recall {rec_5:5.1%} FA {fa_5:5.2%}", flush=True)

    model.load_state_dict(best_state)
    _, (rec_d, fa_d), (rec_5, fa_5) = evaluate(Xte, yte, nte)
    report["best_epoch"] = best_epoch
    report["best_val_loss"] = best_loss
    report["test"] = {"recall_at_deployed": rec_d, "false_alarm_at_deployed": fa_d,
                      "recall_at_0.5": rec_5, "false_alarm_at_0.5": fa_5,
                      "n_pos": nte, "n_neg": len(Xte) - nte}
    print(f"\nTEST split (untouched until now), best epoch {best_epoch}: recall {rec_d:.1%} / FA {fa_d:.2%} "
          f"at {thr:.2f}; recall {rec_5:.1%} / FA {fa_5:.2%} at 0.50", flush=True)
    print("NOTE: these are window-level rates on your own recordings. The real accuracy numbers come from the "
          "held-out eval_set via wake_eval + voice_scorecard.", flush=True)

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
        print("not promoted: score it first (wake_eval --resources with this model, voice_scorecard), "
              "then re-run with --promote.", flush=True)


if __name__ == "__main__":
    main()
