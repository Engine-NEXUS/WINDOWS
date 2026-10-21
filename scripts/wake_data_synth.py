#!/usr/bin/env python3
"""Synthesize wake-word training audio with many TTS voices (Phase E2).

This is how openWakeWord's own recipe gets its positives: thousands of different synthetic speakers saying
the phrase, with varied speed / pitch-like noise / loudness / silence padding. Only one real human voice (the
owner's) is needed on top, for adaptation and as the sealed test set.

    python scripts/wake_data_synth.py voices                    # download Piper voices whose licence passes
    python scripts/wake_data_synth.py positives --per-phrase 800
    python scripts/wake_data_synth.py negatives --per-phrase 300

Writes (all git-ignored under wake_word_data/):
    positive/piper_<voice>_<phrase>_<n>.wav          "nexus", "hey nexus", "okay nexus", "hi nexus", ...
    negative/piper_adv_<voice>_<phrase>_<n>.wav      clear non-wake words/phrases that share sounds
    borderline/piper_bl_<voice>_<phrase>_<n>.wav     phrases a TTS may pronounce AS "nexus" ("next us"):
                                                     listen to them with wake_audition.py review --dir
                                                     wake_word_data/borderline before they can train anything
    synth/manifest.jsonl                             speaker/session/source_group per clip (split by speaker)
    synth/voices.json                                every voice used + its licence text

Voices must be permissively licensed: the voice's MODEL_CARD is downloaded and its licence line checked
(CC0 / CC BY / public domain / MIT / Apache). Unknown or non-commercial licences are skipped unless you pass
--allow-unknown-license. Piper (pip install piper-tts) is the engine; TTS output is never judged by Whisper.
"""

import argparse
import json
import re
import sys
import urllib.request
import wave
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))

BASE = Path(__file__).resolve().parent.parent / "wake_word_data"
HF = "https://huggingface.co/rhasspy/piper-voices/resolve/main/"
SR = 16000

# name -> path in the rhasspy/piper-voices repo (language/voice/quality). Multi-speaker voices give many
# "people" from one download. The licence check below, not this list, decides what is actually used.
VOICES = {
    "en_US-libritts_r-medium": "en/en_US/libritts_r/medium/en_US-libritts_r-medium",
    "en_GB-vctk-medium": "en/en_GB/vctk/medium/en_GB-vctk-medium",
    "en_US-ljspeech-high": "en/en_US/ljspeech/high/en_US-ljspeech-high",
    "en_US-arctic-medium": "en/en_US/arctic/medium/en_US-arctic-medium",
    "en_GB-aru-medium": "en/en_GB/aru/medium/en_GB-aru-medium",
    "en_US-hfc_female-medium": "en/en_US/hfc_female/medium/en_US-hfc_female-medium",
    "en_US-hfc_male-medium": "en/en_US/hfc_male/medium/en_US-hfc_male-medium",
    "en_US-amy-medium": "en/en_US/amy/medium/en_US-amy-medium",
}

# Piper cards often just say "License: See URL". For voices trained on a dataset whose public licence is
# unambiguous we fall back to that - listed here so the choice is visible and reviewable, and recorded in
# voices.json. Anything not here AND not passing the card check is skipped.
KNOWN_DATASET_LICENSE = {
    "libritts_r": "CC BY 4.0 (LibriTTS-R)",
    "vctk": "CC BY 4.0 (VCTK corpus)",
    "ljspeech": "Public domain (LJ Speech)",
}

POSITIVE_PHRASES = {            # phrase -> relative weight. Two-word triggers are the primary ones.
    "hey nexus": 3, "okay nexus": 2, "hi nexus": 2, "nexus": 2, "hello nexus": 1,
}
# Unambiguous non-wake words that share consonants/vowels with the name.
ADVERSARIAL = ["next", "texas", "lexus", "access", "exits", "excess", "necessary", "next up", "next one",
               "next thing", "next year", "nectar", "plexus", "nick says", "hey next", "okay next", "hey texas",
               "hey lexus", "hey nick", "hey alexa", "okay google", "hey siri", "hey exit", "let's go next",
               "what's next", "see you next", "executives"]
# A TTS (or a person) can say these so that they ARE "nexus". Never auto-labelled; review by ear first.
BORDERLINE = ["next us", "next sis", "necks us", "nexis", "nixes", "nexa", "hey next us", "okay next us", "bexus",
              "nexxus", "nex us", "next is"]

LICENSE_OK = re.compile(r"(cc0|public\s*domain|cc[\s-]*by(?![\s-]*(nc|nd|sa))[\s-]*[\d.]*|"
                        r"\bmit\b|apache|bsd|unlicense|creative commons attribution(?!-?non))", re.I)
LICENSE_BAD = re.compile(r"(non[\s-]*commercial|\bnc\b|no[\s-]*derivatives?|\bnd\b|share[\s-]*alike|\bsa\b|"
                         r"research use only|all rights reserved)", re.I)


# ─── pure helpers (unit-tested) ──────────────────────────────────────────

def license_lines(card_text):
    return [l.strip() for l in card_text.splitlines() if "licen" in l.lower()]


def license_ok(card_text, allow_unknown=False):
    """(ok, reason). A voice passes only if its card has a licence line that matches an allowed licence and
    none that matches a restriction (non-commercial, no-derivatives, ...)."""
    lines = license_lines(card_text)
    if not lines:
        return (allow_unknown, "no licence line in MODEL_CARD")
    joined = " ".join(lines)
    if LICENSE_BAD.search(joined):
        return False, f"restricted licence: {lines[0][:80]}"
    if LICENSE_OK.search(joined):
        return True, lines[0][:80]
    return (allow_unknown, f"unrecognised licence: {lines[0][:80]}")


def effective_license(name, card_text, allow_unknown=False):
    """(ok, text, source). The model card wins when it names a licence; "See URL"-style cards fall back to the
    vetted dataset table. A card that names a RESTRICTED licence is never overridden."""
    ok, why = license_ok(card_text, allow_unknown=False)
    if ok:
        return True, why, "card"
    if LICENSE_BAD.search(" ".join(license_lines(card_text))):
        return False, why, "card"
    voice = name.split("-")[1] if "-" in name else name
    if voice in KNOWN_DATASET_LICENSE:
        return True, KNOWN_DATASET_LICENSE[voice], "dataset"
    return (allow_unknown, why, "forced" if allow_unknown else "card")


def slug(text):
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def plan_jobs(voices, phrases, per_phrase, seed, n_speakers=None):
    """Deterministic synthesis jobs. `voices` = {name: n_speakers}; `phrases` = {phrase: weight} or a list.
    Each job = dict(voice, speaker, phrase, length_scale, noise_scale, noise_w, gain_db, lead_s, tail_s, n).
    Speakers are cycled so every speaker of a voice is used about equally."""
    rng = np.random.default_rng(seed)
    weights = phrases if isinstance(phrases, dict) else {p: 1 for p in phrases}
    total_w = float(sum(weights.values()))
    jobs = []
    names = sorted(voices)
    if not names:
        return jobs
    counters = {}
    for phrase, w in sorted(weights.items()):
        count = max(1, int(round(per_phrase * len(weights) * w / total_w))) if isinstance(phrases, dict) else per_phrase
        for i in range(count):
            v = names[i % len(names)]
            ns = max(1, int(voices[v]))
            k = counters.get((v, phrase), 0)
            counters[(v, phrase)] = k + 1
            jobs.append({"voice": v, "speaker": (k * 7 + i) % ns if ns > 1 else 0, "phrase": phrase, "n": i,
                         "length_scale": round(float(rng.uniform(0.8, 1.35)), 3),
                         "noise_scale": round(float(rng.uniform(0.4, 0.95)), 3),
                         "noise_w": round(float(rng.uniform(0.4, 1.0)), 3),
                         "gain_db": round(float(rng.uniform(-14.0, -2.0)), 2),
                         "lead_s": round(float(rng.uniform(0.05, 0.5)), 3),
                         "tail_s": round(float(rng.uniform(0.2, 0.9)), 3)})
    return jobs


def postprocess(x, job, sr_in):
    """Resample to 16 kHz, set loudness, pad silence. Returns float32."""
    import wake_train_lib as L
    y = L.to_mono_16k(np.asarray(x), sr_in)
    peak = float(np.max(np.abs(y))) + 1e-9
    y = y / peak * (10 ** (job["gain_db"] / 20.0))
    pad = lambda s: np.zeros(int(SR * s), dtype=np.float32)       # noqa: E731
    return np.concatenate([pad(job["lead_s"]), y.astype(np.float32), pad(job["tail_s"])])


def write_wav(path, x):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((np.clip(x, -1, 1) * 32767).astype("<i2").tobytes())


def manifest_row(rel, job, kind):
    return {"audio": rel, "kind": kind, "phrase": job["phrase"], "source": "piper",
            "speaker_id": f'{job["voice"]}#{job["speaker"]}', "session_id": job["voice"],
            "source_group_id": f'{job["voice"]}#{job["speaker"]}'}


# ─── Piper adapter (the only part that needs the piper package) ──────────

class Piper:
    """Thin wrapper over piper-tts, tolerant of the 1.2 (`noise_w`) and 1.3+ (`SynthesisConfig`) APIs."""

    def __init__(self, model_path):
        from piper import PiperVoice
        self.voice = PiperVoice.load(str(model_path))
        self.sr = int(self.voice.config.sample_rate)
        self.n_speakers = int(getattr(self.voice.config, "num_speakers", 1) or 1)

    def synth(self, text, job):
        import io
        buf = io.BytesIO()
        with wave.open(buf, "wb") as w:
            try:
                from piper import SynthesisConfig
                cfg = SynthesisConfig(speaker_id=job["speaker"] if self.n_speakers > 1 else None,
                                      length_scale=job["length_scale"], noise_scale=job["noise_scale"],
                                      noise_w_scale=job["noise_w"])
                self.voice.synthesize_wav(text, w, syn_config=cfg)
            except ImportError:                                        # piper-tts 1.2.x
                self.voice.synthesize(text, w, speaker_id=job["speaker"] if self.n_speakers > 1 else None,
                                      length_scale=job["length_scale"], noise_scale=job["noise_scale"],
                                      noise_w=job["noise_w"])
        buf.seek(0)
        with wave.open(buf, "rb") as r:
            raw = r.readframes(r.getnframes())
        return np.frombuffer(raw, dtype="<i2").astype(np.float32) / 32768.0


# ─── commands ────────────────────────────────────────────────────────────

def _get(url, dest):
    dest = Path(dest)
    if dest.exists() and dest.stat().st_size > 0:
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url) as r, open(dest, "wb") as fh:
        fh.write(r.read())
    return dest


def cmd_voices(args):
    out = Path(args.base) / "synth"
    vdir = out / "voices"
    used = {}
    for name, path in VOICES.items():
        try:
            card = _get(HF + path.rsplit("/", 1)[0] + "/MODEL_CARD", vdir / f"{name}.MODEL_CARD").read_text(
                encoding="utf-8", errors="replace")
        except Exception as e:                                             # noqa: BLE001
            print(f"  skip {name}: no model card ({e})")
            continue
        ok, why, src = effective_license(name, card, args.allow_unknown_license)
        print(f"  {'USE ' if ok else 'skip'} {name}: {why} [{src}]")
        if not ok:
            continue
        _get(HF + path + ".onnx", vdir / f"{name}.onnx")
        _get(HF + path + ".onnx.json", vdir / f"{name}.onnx.json")
        used[name] = {"license": why, "license_source": src, "card": f"voices/{name}.MODEL_CARD"}
    (out / "voices.json").write_text(json.dumps(used, indent=2), encoding="utf-8")
    print(f"{len(used)} voice(s) ready in {vdir}")
    return 0 if used else 1


def _load_voices(base):
    out = Path(base) / "synth"
    meta = json.loads((out / "voices.json").read_text()) if (out / "voices.json").exists() else {}
    if not meta:
        raise SystemExit("no voices: run `wake_data_synth.py voices` first")
    engines = {n: Piper(out / "voices" / f"{n}.onnx") for n in meta}
    return engines


def _run(args, phrases, kind, dest_dir, prefix):
    base = Path(args.base)
    engines = _load_voices(base)
    jobs = plan_jobs({n: e.n_speakers for n, e in engines.items()}, phrases, args.per_phrase, args.seed)
    rows, done = [], 0
    for job in jobs:
        rel = f"{dest_dir}/{prefix}_{slug(job['voice'])}_{slug(job['phrase'])}_{job['n']:05d}.wav"
        path = base / rel
        if not path.exists():
            raw = engines[job["voice"]].synth(job["phrase"], job)
            if len(raw) < 800:                                              # a failed/empty synthesis
                continue
            write_wav(path, postprocess(raw, job, engines[job["voice"]].sr))
        rows.append(manifest_row(rel, job, kind))
        done += 1
        if done % 100 == 0:
            print(f"  {done}/{len(jobs)}", flush=True)
    mf = base / "synth" / "manifest.jsonl"
    old = mf.read_text(encoding="utf-8") if mf.exists() else ""
    mf.write_text(old + "".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
    print(f"wrote {done} {kind} clips to {base / dest_dir}")
    return 0


def cmd_positives(args):
    return _run(args, POSITIVE_PHRASES, "positive", "positive", "piper")


def cmd_negatives(args):
    rc = _run(args, ADVERSARIAL, "negative", "negative", "piper_adv")
    args.per_phrase = max(20, args.per_phrase // 4)
    rc |= _run(args, BORDERLINE, "borderline", "borderline", "piper_bl")
    print("NEXT: listen to wake_word_data/borderline with "
          "`python scripts/wake_audition.py review --dir wake_word_data/borderline` - some are really 'nexus'.")
    return rc


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base", default=str(BASE))
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("voices")
    p.add_argument("--allow-unknown-license", action="store_true")
    p.set_defaults(fn=cmd_voices)
    for name, fn, default in (("positives", cmd_positives, 800), ("negatives", cmd_negatives, 120)):
        p = sub.add_parser(name)
        p.add_argument("--per-phrase", type=int, default=default)
        p.add_argument("--seed", type=int, default=1337)
        p.set_defaults(fn=fn)
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
