#!/usr/bin/env python3
"""Decoder-prompt A/B for NEXUS speech recognition (plan 06 step 1, plan 09 layer B).

Question it answers with numbers: does adding the user's own app names to the Groq `prompt` help recognition of those names, and does it
make the recogniser INVENT those names (or repeat commands) on silence, noise or ordinary talk?

    python scripts/prompt_ab.py gen   [--out DIR]               # synthesise test clips with the Windows voices (no network)
    python scripts/prompt_ab.py run   [--out DIR] [--arms A,B,C] # transcribe every clip under every prompt (uses your Groq key: ~170 calls)
    python scripts/prompt_ab.py report [--out DIR]              # tables from the saved results

The Groq key comes from GROQ_API_KEY or the NEXUS settings file (the same rule `collect_nlu_samples.py` uses); it is never printed or saved.
Clips are synthetic speech and synthetic silence/noise: clean audio, so this measures prompt side effects (inventions, repeats) well and
real-microphone accuracy badly. Treat any accuracy gain as an upper bound and confirm with real recordings before enabling for users.
Standard library only; Windows (uses System.Speech through PowerShell).
"""

import argparse
import json
import math
import os
import random
import re
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
import wave

MODEL = "whisper-large-v3-turbo"
URL = "https://api.groq.com/openai/v1/audio/transcriptions"
NEUTRAL = "Transcribe this English audio exactly as spoken."
STATIC = ("NEXUS. Open Chrome. Open WhatsApp. Open Spotify. Open settings. Close the window. New tab. Search for. Play. Pause. Next song. Stop. "
          "Cancel. Ghost mode. Exit ghost mode. List pull requests. Send a WhatsApp message.")
ENTITIES = ["Notepad", "Brave", "Spotify", "WhatsApp", "Antigravity", "VS Code", "Discord", "YouTube"]
ARMS = {
    "A": ("neutral (today's default)", NEUTRAL),
    "B": ("static command sentences (today's opt-in)", STATIC),
    "C": ("bare list of the user's apps", ", ".join(ENTITIES) + "."),
    "D": ("sentences naming the user's apps", " ".join(f"Open {e}." for e in ENTITIES)),
    "E": ("framed list", "Computer commands. Apps: " + ", ".join(ENTITIES) + "."),
}

# (spoken text, expected words that must appear, kind)
PHRASES = [
    ("open notepad", ["notepad"], "cmd"), ("open brave", ["brave"], "cmd"), ("open spotify", ["spotify"], "cmd"),
    ("open whatsapp", ["whatsapp"], "cmd"), ("open antigravity", ["antigravity"], "cmd"), ("open vs code", ["code"], "cmd"),
    ("open discord", ["discord"], "cmd"), ("open youtube", ["youtube"], "cmd"), ("close notepad", ["close", "notepad"], "cmd"),
    ("hook up antigravity", ["hook", "antigravity"], "cmd"), ("shut down my laptop", ["shut", "laptop"], "cmd"),
    ("ghost mode", ["ghost"], "cmd"), ("open command hub", ["hub"], "cmd"), ("list pull requests", ["pull", "requests"], "cmd"),
    ("what time is it", ["time"], "cmd"),
    ("give me a brief summary", ["brief", "summary"], "talk"), ("I need a note pad of paper", ["paper"], "talk"),
    ("the weather is nice today", ["weather"], "talk"),
]
VOICES = ["Microsoft David Desktop", "Microsoft Zira Desktop"]


def norm(s):
    return re.sub(r"[^a-z0-9 ]", " ", s.lower()).split()


# ---------- clip generation ----------

def synth(phrases, voice, rate, wav_path):
    ps = (
        "Add-Type -AssemblyName System.Speech;$s=New-Object System.Speech.Synthesis.SpeechSynthesizer;"
        f"$s.SelectVoice('{voice}');$s.Rate={rate};$s.SetOutputToWaveFile('{wav_path}');$s.Speak('{phrases}');$s.Dispose()"
    )
    subprocess.run(["powershell", "-NoProfile", "-Command", ps], check=True, capture_output=True)


def write_noise(path, seconds, level_db, kind, seed):
    rnd = random.Random(seed)
    sr, n = 16000, 16000 * seconds
    amp = 32767 * (10 ** (level_db / 20))
    out, lp = [], 0.0
    for i in range(n):
        x = rnd.uniform(-1, 1)
        if kind == "hum":  # low rumble plus mains-like hum
            lp = 0.995 * lp + 0.005 * x
            x = lp * 14 + 0.5 * math.sin(2 * math.pi * 50 * i / sr)
        elif kind == "silence":
            x = 0.0
        out.append(int(max(-1, min(1, x)) * amp))
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(struct.pack("<%dh" % len(out), *out))


def cmd_gen(args):
    d = os.path.join(args.out, "clips")
    os.makedirs(d, exist_ok=True)
    manifest = []
    for vi, voice in enumerate(VOICES):
        for pi, (text, expect, kind) in enumerate(PHRASES):
            p = os.path.join(d, f"s_{vi}_{pi:02d}.wav")
            synth(text.replace("'", " "), voice, 0, p)
            manifest.append({"file": p, "truth": text, "expect": expect, "kind": kind, "voice": voice})
    for i, (kind, db) in enumerate([("silence", -90), ("white", -52), ("white", -42), ("hum", -46), ("white", -60), ("hum", -38)]):
        p = os.path.join(d, f"n_{i}.wav")
        write_noise(p, 3, db, kind, i)
        manifest.append({"file": p, "truth": "", "expect": [], "kind": "noise", "voice": f"{kind}{db}"})
    with open(os.path.join(args.out, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1)
    print(len(manifest), "clips in", d)


# ---------- transcription ----------

def load_key():
    key = os.environ.get("GROQ_API_KEY", "").strip()
    if key:
        return key
    appdata = os.environ.get("APPDATA", "")
    for sub in ("com.nexus.assistant", "com.nexus.assistant.contributor"):
        try:
            with open(os.path.join(appdata, sub, "settings.json"), encoding="utf-8") as f:
                data = json.load(f)
            key = (data.get("groqApiKey") or data.get("groq_api_key") or "").strip()
            if key:
                return key
        except (OSError, ValueError):
            continue
    return ""


def transcribe(path, prompt, key):
    boundary = uuid.uuid4().hex
    parts = []
    for k, v in (("model", MODEL), ("language", "en"), ("temperature", "0"), ("response_format", "verbose_json"), ("prompt", prompt)):
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode())
    with open(path, "rb") as f:
        audio = f.read()
    parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="a.wav"\r\nContent-Type: audio/wav\r\n\r\n'.encode() + audio + b"\r\n")
    parts.append(f"--{boundary}--\r\n".encode())
    req = urllib.request.Request(URL, data=b"".join(parts), method="POST",
                                 headers={"Authorization": "Bearer " + key, "Content-Type": "multipart/form-data; boundary=" + boundary,
                                          "User-Agent": "nexus-prompt-ab/1.0"})
    for attempt in range(6):
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return json.loads(r.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            if e.code == 429:
                wait = float(e.headers.get("retry-after") or 8 * (attempt + 1))
                time.sleep(min(wait, 60))
                continue
            return {"error": f"HTTP {e.code}"}
        except (urllib.error.URLError, TimeoutError):
            time.sleep(3)
    return {"error": "gave up"}


def cmd_run(args):
    key = load_key()
    if not key:
        print("no Groq key: set GROQ_API_KEY or run NEXUS once so settings.json has one", file=sys.stderr)
        return 2
    with open(os.path.join(args.out, "manifest.json"), encoding="utf-8") as f:
        manifest = json.load(f)
    arms = [a for a in args.arms.split(",") if a in ARMS]
    path = os.path.join(args.out, "results.json")
    try:
        with open(path, encoding="utf-8") as f:
            results = json.load(f)
    except (OSError, ValueError):
        results = {}
    total = len(arms) * len(manifest)
    done = 0
    for arm in arms:
        for clip in manifest:
            k = f"{arm}|{clip['file']}"
            done += 1
            if k in results and "text" in results[k]:
                continue
            t0 = time.time()
            resp = transcribe(clip["file"], ARMS[arm][1], key)
            segs = resp.get("segments") or []
            results[k] = {"text": (resp.get("text") or "").strip(), "error": resp.get("error"),
                          "avg_logprob": min([s.get("avg_logprob", 0) for s in segs], default=None),
                          "no_speech_prob": max([s.get("no_speech_prob", 0) for s in segs], default=None)}
            with open(path, "w", encoding="utf-8") as f:
                json.dump(results, f)
            print(f"[{done}/{total}] {arm} {os.path.basename(clip['file'])}: {results[k]['text'][:60]!r}", flush=True)
            time.sleep(max(0.0, 3.4 - (time.time() - t0)))  # stay under 20 requests a minute
    return 0


# ---------- scoring ----------

def entity_words():
    return {w for e in ENTITIES for w in norm(e)} - {"vs"}


def repeated(words):
    """True when the transcript says the same word twice in a row or repeats a whole short phrase."""
    if any(a == b for a, b in zip(words, words[1:])):
        return True
    n = len(words)
    for size in range(2, n // 2 + 1):
        if words[:size] == words[size:2 * size] and n <= 2 * size + 1:
            return True
    return False


def score(manifest, results, arm):
    ents = entity_words()
    cmd = [c for c in manifest if c["kind"] == "cmd"]
    talk = [c for c in manifest if c["kind"] == "talk"]
    noise = [c for c in manifest if c["kind"] == "noise"]

    def text(c):
        return results.get(f"{arm}|{c['file']}", {}).get("text", "")

    def has_all(c):
        w = set(norm(text(c)))
        return all(x in w for x in c["expect"])

    hit = sum(has_all(c) for c in cmd)
    exact = sum(norm(text(c)) == norm(c["truth"]) for c in cmd)
    dup = sum(repeated(norm(text(c))) for c in cmd + talk)
    # a listed app name that was NOT spoken
    invented_entities = sum(1 for c in cmd + talk if (set(norm(text(c))) & ents) - set(norm(c["truth"])) - {"code"})
    talk_ok = sum(has_all(c) for c in talk)
    noise_text = [text(c) for c in noise]
    leak_any = sum(1 for t in noise_text if re.search(r"[a-z]", t.lower()))
    leak_entity = sum(1 for t in noise_text if set(norm(t)) & (ents | {"open", "close", "play", "stop", "cancel"}))
    return {"cmd": len(cmd), "cmd_hit": hit, "cmd_exact": exact, "talk": len(talk), "talk_ok": talk_ok, "repeats": dup,
            "invented_apps": invented_entities, "noise": len(noise), "noise_any_text": leak_any, "noise_command_words": leak_entity}


def cmd_report(args):
    with open(os.path.join(args.out, "manifest.json"), encoding="utf-8") as f:
        manifest = json.load(f)
    with open(os.path.join(args.out, "results.json"), encoding="utf-8") as f:
        results = json.load(f)
    print("arm | commands with the right words | exact | talk kept | repeats | invented app names | noise->any text | noise->command words")
    for arm in sorted({k.split("|")[0] for k in results}):
        s = score(manifest, results, arm)
        print(f"{arm} {ARMS[arm][0]}: {s['cmd_hit']}/{s['cmd']} | {s['cmd_exact']}/{s['cmd']} | {s['talk_ok']}/{s['talk']} | {s['repeats']} | "
              f"{s['invented_apps']} | {s['noise_any_text']}/{s['noise']} | {s['noise_command_words']}/{s['noise']}")
    print("\nclips where arms disagree on the right words (command clips):")
    for c in manifest:
        if c["kind"] != "cmd":
            continue
        row = {a: results.get(f"{a}|{c['file']}", {}).get("text") for a in sorted({k.split('|')[0] for k in results})}
        words = [set(norm(t or "")) for t in row.values()]
        ok = [all(x in w for x in c["expect"]) for w in words]
        if len(set(ok)) > 1:
            print(f"  {c['truth']!r} ({c['voice'].split()[1]}): " + " | ".join(f"{a}={t!r}" for a, t in row.items()))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=["gen", "run", "report"])
    ap.add_argument("--out", default=os.path.join(os.environ.get("TEMP", "."), "nexus_prompt_ab"))
    ap.add_argument("--arms", default="A,B,C,E")
    args = ap.parse_args(argv)
    os.makedirs(args.out, exist_ok=True)
    return {"gen": cmd_gen, "run": cmd_run, "report": cmd_report}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
