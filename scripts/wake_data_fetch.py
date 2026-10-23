#!/usr/bin/env python3
"""Fetch reusable, permissively-licensed public audio for wake-word training (Phase E2).

Nobody needs to record hours of TV, room noise and everyday speech: real, labelled audio of all of that is
public. Only "NEXUS" / "hey NEXUS" need our own voices (plus synthetic TTS voices, see wake_data_synth.py).

    python scripts/wake_data_fetch.py list                       # what exists, size, license
    python scripts/wake_data_fetch.py get librispeech-dev-clean musan --max-gb 20
    python scripts/wake_data_fetch.py prepare --max-hours 8      # cut into 16 kHz training/held-out clips
    python scripts/wake_data_fetch.py sources                    # (re)write DATA_SOURCES.md

Downloads are resumable (HTTP Range), size-capped, and verified trust-on-first-use: the first complete
download of a file records its SHA-256 in public/SHA256.lock.json and every later run must match it (this
catches truncated or swapped files; it is NOT a substitute for the publisher's checksum - compare it with
the one on the dataset page if you need provenance). Archives are extracted with a path-traversal guard.

Layout (all git-ignored, under wake_word_data/public/):
    raw/<id>/...                downloads + extracted archives
    negative/public_*.wav       real speech (LibriSpeech, Speech Commands) cut to <= 2 s
    background/public_*.wav     MUSAN noise/music, and synthetic "TV in a room" mixes
    heldout/*.wav + manifest.jsonl   LONG recordings never trained on -> false alarms per hour
                                     (wake_eval --kinds room); LibriSpeech dev/test + held-out MUSAN files

Licenses: only datasets whose published license permits redistribution of derived models are fetched by
default. A trained model is a derivative of the data, not a redistribution of it, but attribution
requirements (CC BY) are listed in DATA_SOURCES.md - keep that file with the model.
"""

import argparse
import hashlib
import json
import os
import sys
import tarfile
import time
import urllib.request
import zipfile
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))

BASE = Path(__file__).resolve().parent.parent / "wake_word_data" / "public"
SR = 16000
LOCK = "SHA256.lock.json"

# id -> download description. `approx_gb` is only for the size cap / `list`; the real size comes from the server.
DATASETS = {
    "librispeech-dev-clean": {
        "url": "https://www.openslr.org/resources/12/dev-clean.tar.gz", "approx_gb": 0.34, "kind": "tar",
        "license": "CC BY 4.0", "role": "held-out real speech (false alarms per hour)",
        "attribution": "LibriSpeech, Panayotov et al. 2015, https://www.openslr.org/12", "permissive": True},
    "librispeech-test-clean": {
        "url": "https://www.openslr.org/resources/12/test-clean.tar.gz", "approx_gb": 0.35, "kind": "tar",
        "license": "CC BY 4.0", "role": "held-out real speech (false alarms per hour)",
        "attribution": "LibriSpeech, Panayotov et al. 2015, https://www.openslr.org/12", "permissive": True},
    "librispeech-train-clean-100": {
        "url": "https://www.openslr.org/resources/12/train-clean-100.tar.gz", "approx_gb": 6.3, "kind": "tar",
        "license": "CC BY 4.0", "role": "training: real read-speech negatives (251 speakers)",
        "attribution": "LibriSpeech, Panayotov et al. 2015, https://www.openslr.org/12", "permissive": True},
    "speech-commands": {
        "url": "http://download.tensorflow.org/data/speech_commands_v0.02.tar.gz", "approx_gb": 2.3, "kind": "tar",
        "license": "CC BY 4.0", "role": "training: 35 real one-word negatives from thousands of speakers",
        "attribution": "Speech Commands v0.02, Warden 2018, https://arxiv.org/abs/1804.03209", "permissive": True},
    "musan": {
        "url": "https://www.openslr.org/resources/17/musan.tar.gz", "approx_gb": 10.3, "kind": "tar",
        "license": "CC BY 4.0 (OpenSLR); per-file notes in musan/*/LICENSE",
        "role": "training + held-out: real speech, music and noise (the TV / room / street background)",
        "attribution": "MUSAN, Snyder, Chen, Povey 2015, https://www.openslr.org/17", "permissive": True},
    "rirs-noises": {
        "url": "https://www.openslr.org/resources/28/rirs_noises.zip", "approx_gb": 1.3, "kind": "zip",
        "license": "Apache 2.0", "role": "room impulse responses (reverb) and point-source noises",
        "attribution": "RIRS_NOISES, Ko et al. 2017, https://www.openslr.org/28", "permissive": True},
}

MANUAL = {
    "common-voice": "CC0, needs a Mozilla account: https://commonvoice.mozilla.org/datasets (optional extra negatives)",
    "but-reverb-db": "CC BY 4.0, manual download: https://speech.fit.vutbr.cz/software/but-speech-fit-reverb-database",
    "fsd50k": "per-clip licenses - keep CC0 / CC BY ONLY (read each clip's JSON): https://zenodo.org/records/4060432",
}
EXCLUDED = ("ACAV100M embeddings (license unclear)", "ESC-50 (CC BY-NC)", "WHAM! noise (CC BY-NC)",
            "DEMAND (share-alike - viral)", "MIT IR survey (license unclear)")


# ─── pure helpers (unit-tested) ──────────────────────────────────────────

def safe_member(dest, name):
    """True when extracting `name` under `dest` stays inside `dest` (blocks ../ and absolute paths)."""
    target = (Path(dest) / name).resolve()
    return str(target).startswith(str(Path(dest).resolve()) + os.sep) or target == Path(dest).resolve()


def extract(archive, dest, kind):
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    if kind == "tar":
        with tarfile.open(archive) as t:
            members = t.getmembers()
            bad = [m.name for m in members if not safe_member(dest, m.name) or m.issym() or m.islnk()]
            if bad:
                raise ValueError(f"unsafe archive member(s): {bad[:3]}")
            t.extractall(dest, members=members)
    else:
        with zipfile.ZipFile(archive) as z:
            bad = [n for n in z.namelist() if not safe_member(dest, n)]
            if bad:
                raise ValueError(f"unsafe archive member(s): {bad[:3]}")
            z.extractall(dest)


def sha256_file(path, chunk=1 << 20):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        while True:
            b = fh.read(chunk)
            if not b:
                return h.hexdigest()
            h.update(b)


def check_lock(lock_path, key, digest):
    """Trust-on-first-use. Returns 'new' (recorded), 'ok' or 'MISMATCH'."""
    lock_path = Path(lock_path)
    lock = json.loads(lock_path.read_text()) if lock_path.exists() else {}
    if key not in lock:
        lock[key] = digest
        lock_path.write_text(json.dumps(lock, indent=2))
        return "new"
    return "ok" if lock[key] == digest else "MISMATCH"


def download(url, dest, max_bytes=None, opener=urllib.request.urlopen, progress=True):
    """Resumable download to `dest` (via dest.part + Range). Refuses when the server announces more than
    `max_bytes`. Returns the final path."""
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists():
        return dest
    part = dest.with_name(dest.name + ".part")
    have = part.stat().st_size if part.exists() else 0
    req = urllib.request.Request(url, headers={"Range": f"bytes={have}-"} if have else {})
    with opener(req) as resp:
        status = getattr(resp, "status", 200)
        length = int(resp.headers.get("Content-Length") or 0)
        total = length + (have if status == 206 else 0)
        if max_bytes is not None and total and total > max_bytes:
            raise ValueError(f"{url}: {total / 1e9:.2f} GB exceeds the cap of {max_bytes / 1e9:.2f} GB")
        mode = "ab" if status == 206 and have else "wb"
        if mode == "wb":
            have = 0
        got, t0 = have, time.time()
        with open(part, mode) as fh:
            while True:
                b = resp.read(1 << 20)
                if not b:
                    break
                fh.write(b)
                got += len(b)
                if progress and total and int((got - len(b)) / total * 20) != int(got / total * 20):
                    print(f"    {got / 1e6:,.0f}/{total / 1e6:,.0f} MB ({time.time() - t0:.0f}s)", flush=True)
    if total and got != total:
        raise IOError(f"{url}: got {got} of {total} bytes - run the command again to resume")
    part.replace(dest)
    return dest


def chunk_audio(x, sr=SR, seconds=2.0, hop_seconds=2.0, min_rms=1e-3):
    """Cut a waveform into fixed windows, dropping near-silent ones."""
    n, hop = int(sr * seconds), int(sr * hop_seconds)
    out = []
    for i in range(0, max(0, len(x) - n + 1), hop):
        w = x[i:i + n]
        if float(np.sqrt(np.mean(w.astype(np.float64) ** 2))) >= min_rms:
            out.append(w)
    return out


def is_heldout(name, fraction=0.2):
    """Stable file-name hash -> held-out (never trained on)."""
    h = int(hashlib.sha256(name.encode()).hexdigest()[:8], 16) / 0xFFFFFFFF
    return h < fraction


def make_tv_mix(speech, music, rir, snr_db, rng):
    """Speech over music at `snr_db` (speech relative to music), then through a room impulse response -
    the sound of a TV or a video call in the next room."""
    from scipy.signal import fftconvolve
    n = max(len(speech), len(music))
    s = np.resize(speech, n).astype(np.float64)
    m = np.resize(music, n).astype(np.float64)
    ps, pm = float(np.mean(s ** 2)) + 1e-12, float(np.mean(m ** 2)) + 1e-12
    m *= np.sqrt(ps / pm / (10 ** (snr_db / 10.0)))
    mix = s + m
    if rir is not None and len(rir):
        r = rir / (np.max(np.abs(rir)) + 1e-9)
        mix = fftconvolve(mix, r)[:n]
    peak = np.max(np.abs(mix)) + 1e-9
    return (mix / peak * float(rng.uniform(0.15, 0.6))).astype(np.float32)


def write_wav16(path, x):
    from scipy.io import wavfile
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    wavfile.write(str(path), SR, (np.clip(x, -1, 1) * 32767).astype(np.int16))


# ─── commands ────────────────────────────────────────────────────────────

def cmd_list(args):
    print(f"{'id':<30}{'~GB':>6}  license / role")
    for k, d in DATASETS.items():
        print(f"{k:<30}{d['approx_gb']:>6}  {d['license']} | {d['role']}")
    print("\nmanual (not auto-downloaded):")
    for k, v in MANUAL.items():
        print(f"  {k}: {v}")
    print("\nexcluded on purpose: " + "; ".join(EXCLUDED))
    return 0


def cmd_get(args):
    base = Path(args.base)
    ids = args.ids or []
    unknown = [i for i in ids if i not in DATASETS]
    if unknown:
        print(f"unknown dataset(s): {unknown}. See: wake_data_fetch.py list")
        return 2
    cap = int(args.max_gb * 1e9)
    for i in ids:
        d = DATASETS[i]
        if not d["permissive"] and not args.allow_nonpermissive:
            print(f"skip {i}: not permissively licensed")
            continue
        raw = base / "raw"
        arc = raw / (i + (".tar.gz" if d["kind"] == "tar" else ".zip"))
        print(f"== {i} ({d['license']})", flush=True)
        try:
            download(d["url"], arc, max_bytes=cap)
        except (ValueError, IOError, OSError) as e:
            print(f"   FAILED: {e}")
            return 1
        verdict = check_lock(base / LOCK, arc.name, sha256_file(arc))
        if verdict == "MISMATCH":
            print(f"   {arc.name} changed since it was first downloaded - delete it to re-fetch. Not extracting.")
            return 1
        print(f"   checksum {verdict}; extracting...", flush=True)
        marker = raw / i / ".extracted"
        if not marker.exists():
            extract(arc, raw / i, d["kind"])
            marker.write_text("ok")
        if args.delete_archives:
            arc.unlink(missing_ok=True)
    write_sources(base)
    return 0


def write_sources(base):
    base = Path(base)
    base.mkdir(parents=True, exist_ok=True)
    lines = ["# Wake-word training data sources", "",
             "Generated by `scripts/wake_data_fetch.py`. Keep this file with any model trained on this data "
             "(attribution requirements).", "", "| dataset | license | role | attribution |", "|---|---|---|---|"]
    for k, d in DATASETS.items():
        got = (base / "raw" / k / ".extracted").exists()
        lines.append(f"| {k}{'' if got else ' (not fetched)'} | {d['license']} | {d['role']} | {d['attribution']} |")
    lines += ["", "Synthetic positives/negatives: Piper TTS voices, see `wake_data_synth.py` (voice model cards are "
                  "recorded in `wake_word_data/synth/voices.json`).", "",
              "Excluded on purpose: " + "; ".join(EXCLUDED) + ".", ""]
    (base / "DATA_SOURCES.md").write_text("\n".join(lines), encoding="utf-8")


def _read_audio(path):
    """float32 mono 16 kHz from wav or flac (soundfile for flac)."""
    import wake_train_lib as L
    p = str(path)
    if p.lower().endswith(".wav"):
        return L.load_wav_16k(p)
    import soundfile as sf
    data, sr = sf.read(p, dtype="float32", always_2d=False)
    return L.to_mono_16k(data, sr)


def cmd_prepare(args):
    base = Path(args.base)
    rng = np.random.default_rng(args.seed)
    raw = base / "raw"
    budget = int(args.max_hours * 3600 * SR)
    written = {"negative": 0, "background": 0, "heldout": 0}

    # 1. real speech negatives
    spent = 0
    for ds in ("librispeech-train-clean-100",):
        files = sorted((raw / ds).rglob("*.flac"))
        rng.shuffle(files)
        for f in files:
            if spent >= budget:
                break
            for j, w in enumerate(chunk_audio(_read_audio(f))[:3]):
                write_wav16(base / "negative" / f"public_ls_{f.stem}_{j}.wav", w)
                spent += len(w)
                written["negative"] += 1
    sc = raw / "speech-commands"
    if sc.exists():
        per_word = {}
        for f in sorted(sc.rglob("*.wav")):
            word = f.parent.name
            if word.startswith("_") or per_word.get(word, 0) >= args.words_per_class:
                continue
            per_word[word] = per_word.get(word, 0) + 1
            write_wav16(base / "negative" / f"public_sc_{word}_{f.stem}.wav", _read_audio(f))
            written["negative"] += 1

    # 2. MUSAN: background for training, a stable 20 % kept as long held-out recordings
    musan = raw / "musan" / "musan"
    music_pool, speech_pool = [], []
    if musan.exists():
        for kind in ("noise", "music", "speech"):
            files = sorted((musan / kind).rglob("*.wav"))
            for f in files:
                if is_heldout(f.name):
                    continue
                for j, w in enumerate(chunk_audio(_read_audio(f), seconds=4.0, hop_seconds=4.0)[:2]):
                    write_wav16(base / "background" / f"public_musan_{kind}_{f.stem}_{j}.wav", w)
                    written["background"] += 1
                    if kind == "music" and len(music_pool) < 200:
                        music_pool.append(w)
                    if kind == "speech" and len(speech_pool) < 200:
                        speech_pool.append(w)

    # 3. synthetic "TV in a room": speech over music through a real room impulse response
    rirs = sorted((raw / "rirs-noises").rglob("*.wav"))[: args.max_rirs] if (raw / "rirs-noises").exists() else []
    if music_pool and speech_pool:
        for k in range(args.tv_mixes):
            rir = _read_audio(rirs[int(rng.integers(0, len(rirs)))])[: SR] if rirs else None
            x = make_tv_mix(speech_pool[int(rng.integers(0, len(speech_pool)))],
                            music_pool[int(rng.integers(0, len(music_pool)))], rir, float(rng.uniform(3, 15)), rng)
            write_wav16(base / "background" / f"public_tv_{k:04d}.wav", x)
            written["background"] += 1

    # 4. held-out LONG recordings (false alarms per hour). Never read by the trainer.
    rows = []
    sources = []
    for ds in ("librispeech-dev-clean", "librispeech-test-clean"):
        sources += [("librispeech", f) for f in sorted((raw / ds).rglob("*.flac"))]
    if musan.exists():
        for kind in ("noise", "music", "speech"):
            sources += [(f"musan_{kind}", f) for f in sorted((musan / kind).rglob("*.wav")) if is_heldout(f.name)]
    by_kind = {}
    for kind, f in sources:
        by_kind.setdefault(kind, []).append(f)
    for kind, files in by_kind.items():
        buf, idx = [], 0
        for f in files:
            buf.append(_read_audio(f))
            if sum(len(b) for b in buf) >= 10 * 60 * SR:
                name = f"heldout_{kind}_{idx:03d}.wav"
                write_wav16(base / "heldout" / name, np.concatenate(buf))
                rows.append({"audio": name, "kind": "room", "environment": kind, "source": "public"})
                written["heldout"] += 1
                buf, idx = [], idx + 1
    if rows:
        (base / "heldout").mkdir(parents=True, exist_ok=True)
        (base / "heldout" / "manifest.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
    write_sources(base)
    print(f"prepared: {written}  (held-out hours: {written['heldout'] * 10 / 60:.1f})")
    if not any(written.values()):
        print("nothing prepared - run `get` first.")
        return 1
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base", default=str(BASE))
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("list").set_defaults(fn=cmd_list)
    p = sub.add_parser("get")
    p.add_argument("ids", nargs="*")
    p.add_argument("--max-gb", type=float, default=12.0, help="refuse any single download larger than this")
    p.add_argument("--delete-archives", action="store_true")
    p.add_argument("--allow-nonpermissive", action="store_true")
    p.set_defaults(fn=cmd_get)
    p = sub.add_parser("prepare")
    p.add_argument("--max-hours", type=float, default=8.0, help="cap on LibriSpeech training negatives")
    p.add_argument("--words-per-class", type=int, default=150)
    p.add_argument("--tv-mixes", type=int, default=300)
    p.add_argument("--max-rirs", type=int, default=200)
    p.add_argument("--seed", type=int, default=1337)
    p.set_defaults(fn=cmd_prepare)
    sub.add_parser("sources").set_defaults(fn=lambda a: (write_sources(a.base), print(f"wrote {Path(a.base) / 'DATA_SOURCES.md'}"), 0)[-1])
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
