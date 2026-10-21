"""Torch-free building blocks for the wake-word trainer (testable without a GPU or recordings).

* `Frontend`   - the high-pass / gate / AGC chain from `dsp_spec.json` ('aligned', identical to what
                 `wakeDspProfile=aligned` serves at runtime) + the mel/embedding models.
* `assign_splits` - group-level train/val/test assignment: every window of one recording (or one
                 manifest source group) lands in the SAME split, so validation numbers are not inflated by
                 near-duplicate windows from the same clip.
* augmentations - seeded and deterministic.
* `rates_at`   - recall / false-alarm at the deployed threshold.

numpy, scipy, onnxruntime only.
"""

import hashlib
import json
import random
from pathlib import Path

import numpy as np
from scipy.io import wavfile
from scipy.signal import fftconvolve, lfilter, resample_poly

import wake_dsp

SR = 16000
CHUNK = 1280
LOOKBACK = 480
MEL_PER_CHUNK = 8
MEL_CIRC = 10
EMB_FRAMES = 16

ROOT = Path(__file__).resolve().parent.parent
OWW_DIR = ROOT / "src-tauri" / "resources" / "oww"


# ─── audio i/o ───────────────────────────────────────────────────────────

def to_mono_16k(data, sr):
    """int16/float array at `sr` -> float32 mono at 16 kHz with a proper anti-alias filter."""
    data = np.asarray(data)
    if data.ndim > 1:
        data = data.mean(axis=1)
    if np.issubdtype(data.dtype, np.integer):
        x = data.astype(np.float32) / 32768.0
    else:
        x = data.astype(np.float32)
    if sr != SR:
        g = int(np.gcd(int(sr), SR))
        x = resample_poly(x, SR // g, int(sr) // g).astype(np.float32)
    return x


def load_wav_16k(path):
    sr, data = wavfile.read(str(path))
    return to_mono_16k(data, sr)


# ─── front end ───────────────────────────────────────────────────────────

class Frontend:
    """Feature extraction exactly as the runtime's 'aligned' chain does it."""

    def __init__(self, mel_sess, emb_sess, params=None, oww_dir=OWW_DIR):
        self.mel, self.emb = mel_sess, emb_sess
        self.names = (mel_sess.get_inputs()[0].name, emb_sess.get_inputs()[0].name)
        self.p = params or wake_dsp.aligned_params()
        comfort = Path(oww_dir) / "comfort_embedding.npy"
        self.comfort = np.load(str(comfort)).astype(np.float32) if comfort.exists() else np.zeros(96, np.float32)

    def highpass(self, audio, cutoff_hz=None):
        """One-pole high-pass, same recurrence as the Rust HighPassFilter."""
        fc = cutoff_hz or self.p["highpass_hz"]
        dt = 1.0 / SR
        rc = 1.0 / (2.0 * np.pi * fc)
        alpha = rc / (rc + dt)
        # y[n] = alpha * (y[n-1] + x[n] - x[n-1])  ==  IIR with b=[alpha,-alpha], a=[1,-alpha]
        return lfilter([alpha, -alpha], [1.0, -alpha], audio.astype(np.float64)).astype(np.float32)

    def windows(self, audio, include_transitions=False, highpass_hz=None):
        """List of [16, 96] embedding windows, one per voiced 80 ms chunk (+2 transition windows)."""
        audio = self.highpass(audio.astype(np.float32), highpass_hz)
        lookback = np.zeros(LOOKBACK, dtype=np.float32)
        silent_mel = np.zeros((MEL_PER_CHUNK, 32), dtype=np.float32)
        mel_buf = [silent_mel] * MEL_CIRC
        emb_buf = [self.comfort.copy() for _ in range(EMB_FRAMES)]
        out, trailing, had_speech = [], 0, False
        for i in range(0, len(audio) - CHUNK + 1, CHUNK):
            chunk = audio[i:i + CHUNK].copy()
            rms = float(np.sqrt(np.mean(chunk ** 2)))
            if rms < self.p["silence_rms"]:
                emb_buf = (emb_buf + [self.comfort])[-EMB_FRAMES:]
                mel_buf = (mel_buf + [silent_mel])[-MEL_CIRC:]
                if include_transitions and had_speech and trailing < 2:
                    trailing += 1
                    out.append(np.stack(emb_buf).astype(np.float32))
                continue
            had_speech, trailing = True, 0
            gain = wake_dsp.agc_gain(rms, self.p)
            if abs(gain - 1.0) > 1e-9:
                chunk = np.clip(chunk * gain, -1.0, 1.0)
            framed = np.concatenate([lookback, chunk]) * 32768.0
            lookback = chunk[-LOOKBACK:]
            m = self.mel.run(None, {self.names[0]: framed[None, :]})[0]
            m = m.reshape(MEL_PER_CHUNK, 32) / 10.0 + 2.0
            mel_buf = (mel_buf + [m])[-MEL_CIRC:]
            window = np.stack(mel_buf).reshape(80, 32)[4:80]
            e = self.emb.run(None, {self.names[1]: window[None, :, :, None].astype(np.float32)})[0]
            emb_buf = (emb_buf + [e.reshape(-1)])[-EMB_FRAMES:]
            out.append(np.stack(emb_buf).astype(np.float32))
        return out


# ─── splits ──────────────────────────────────────────────────────────────

def group_key(path, manifest_rows=None):
    """The unit that must never straddle train/val/test.

    With a manifest (wake_data_foundation schema): `source_group_id`, else `speaker_id`+`session_id`.
    Without one: the file itself (each recording is its own group) - already enough to stop
    near-duplicate windows of one clip leaking across the split."""
    name = Path(path).name
    if manifest_rows:
        row = manifest_rows.get(name) or manifest_rows.get(str(path))
        if row:
            return str(row.get("source_group_id") or f'{row.get("speaker_id", "")}|{row.get("session_id", "")}')
    return name


def assign_splits(groups, seed=1337, ratios=(0.70, 0.15, 0.15)):
    """Deterministic group -> 'train' | 'val' | 'test'. Stable under reordering and adding groups
    (hash of seed+group), so a new recording never reshuffles old assignments."""
    assert abs(sum(ratios) - 1.0) < 1e-6
    out = {}
    for g in groups:
        h = int(hashlib.sha256(f"{seed}:{g}".encode()).hexdigest()[:8], 16) / 0xFFFFFFFF
        out[g] = "train" if h < ratios[0] else ("val" if h < ratios[0] + ratios[1] else "test")
    return out


def load_manifest_rows(path):
    """{audio basename or relative path: row} from a manifest.jsonl; {} if absent."""
    p = Path(path)
    rows = {}
    if not p.exists():
        return rows
    for line in p.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            r = json.loads(line)
            rows[Path(r.get("audio", "")).name] = r
            rows[r.get("audio", "")] = r
    return rows


def is_sealed(group, frac, seed=1337):
    """True for a stable `frac` of groups, chosen by a hash independent of the train/val/test one. Sealed
    recordings are never read for training, validation or the trainer's own test split - they are the
    real-world test set that `wake_eval` scores afterwards."""
    if frac <= 0:
        return False
    h = int(hashlib.sha256(f"sealed:{seed}:{group}".encode()).hexdigest()[:8], 16) / 0xFFFFFFFF
    return h < frac


def cap_list(items, cap, rng):
    """At most `cap` items, a uniform random subset when over (order of the survivors preserved)."""
    if cap is None or len(items) <= cap:
        return list(items)
    keep = np.sort(rng.choice(len(items), size=cap, replace=False))
    return [items[i] for i in keep]


def hard_negative_indices(probs, floor, cap):
    """Indices of the most confusing negatives: probability >= floor, highest first, at most `cap`."""
    probs = np.asarray(probs, dtype=np.float64)
    idx = np.nonzero(probs >= floor)[0]
    idx = idx[np.argsort(-probs[idx], kind="stable")]
    return idx[:cap]


# ─── augmentations (all take an explicit numpy Generator) ────────────────

def aug_gain(x, rng, lo_db=-12.0, hi_db=6.0):
    return np.clip(x * 10 ** (rng.uniform(lo_db, hi_db) / 20.0), -1.0, 1.0).astype(np.float32)


def aug_shift(x, rng, max_ms=400):
    """Random start offset: pad with room silence or trim the head."""
    n = int(rng.integers(-max_ms, max_ms + 1)) * SR // 1000
    if n >= 0:
        return np.concatenate([np.zeros(n, np.float32), x])
    return x[-n:]


def aug_speed(x, rng, pct=0.08):
    """Resample by +-pct (changes pitch and tempo together, like a different speaker rate)."""
    f = 1.0 + float(rng.uniform(-pct, pct))
    num, den = int(round(f * 1000)), 1000
    g = int(np.gcd(num, den))
    return resample_poly(x, den // g, num // g).astype(np.float32)


def aug_reverb(x, rng, rt60_s=None):
    """Convolve with a synthetic exponentially decaying noise impulse response."""
    rt = rt60_s or float(rng.uniform(0.15, 0.6))
    n = int(rt * SR)
    t = np.arange(n) / SR
    ir = rng.standard_normal(n) * np.exp(-6.9 * t / rt)
    ir[0] = 1.0
    ir = (ir / np.sqrt(np.sum(ir ** 2))).astype(np.float32)
    y = fftconvolve(x, ir)[:len(x)]
    peak = float(np.max(np.abs(y))) or 1.0
    return (y * (float(np.max(np.abs(x))) / peak)).astype(np.float32)


def mix_noise(x, noise, snr_db, rng):
    """Add a random slice of `noise` at the requested signal-to-noise ratio."""
    if len(noise) < len(x):
        noise = np.tile(noise, int(np.ceil(len(x) / max(len(noise), 1))))
    start = int(rng.integers(0, len(noise) - len(x) + 1))
    n = noise[start:start + len(x)]
    ps = float(np.mean(x ** 2)) or 1e-12
    pn = float(np.mean(n ** 2)) or 1e-12
    scale = np.sqrt(ps / (pn * 10 ** (snr_db / 10.0)))
    return np.clip(x + n * scale, -1.0, 1.0).astype(np.float32)


# ─── metrics ─────────────────────────────────────────────────────────────

def rates_at(pos_probs, neg_probs, threshold):
    """(recall, false_alarm_rate) at `threshold`; NaN where a class is empty."""
    pos, neg = np.asarray(pos_probs), np.asarray(neg_probs)
    rec = float(np.mean(pos >= threshold)) if len(pos) else float("nan")
    fa = float(np.mean(neg >= threshold)) if len(neg) else float("nan")
    return rec, fa


def deployed_threshold():
    """The threshold the app actually uses (acoustic_profile.json kws_threshold)."""
    try:
        return float(json.loads((OWW_DIR / "acoustic_profile.json").read_text(encoding="utf-8"))["kws_threshold"])
    except Exception:  # noqa: BLE001
        return 0.68


def seeded(seed):
    random.seed(seed)
    np.random.seed(seed)
    return np.random.default_rng(seed)
