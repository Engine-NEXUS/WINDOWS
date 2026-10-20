#!/usr/bin/env python3
"""Held-out voice evaluation set: record, validate, summarise.

This set is the ONLY source of the accuracy numbers in the scorecard. It must
never be used for training (split is always "test", sessions are separate from
any training recordings). Record it with the owner plus 2-3 other people.

    python scripts/voice_eval_set.py plan
    python scripts/voice_eval_set.py record  --speaker owner --device laptop-array --section wake
    python scripts/voice_eval_set.py record  --speaker owner --section command
    python scripts/voice_eval_set.py record  --speaker owner --section negative
    python scripts/voice_eval_set.py record  --speaker owner --section silence
    python scripts/voice_eval_set.py room    --speaker owner --minutes 10 --environment tv
    python scripts/voice_eval_set.py validate
    python scripts/voice_eval_set.py stats

Layout (default eval_set/, git-ignored):
    eval_set/manifest.jsonl
    eval_set/audio/<speaker>/<session>/<id>.wav

Rows use the field names of scripts/wake_data_foundation.py (audio, label,
phrase, speaker_id, accent, device_class, environment, distance_cm, volume,
session_id, source_group_id, split, consent) plus `kind`, `expected_text`,
`expected_intent` which the Rust evaluators (examples/wake_eval.rs,
examples/stt_eval.rs) read. Recording needs `sounddevice`; everything else is
standard library only.
"""

import argparse
import json
import sys
import wave
from collections import Counter
from datetime import datetime
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parent.parent
DEFAULT_DIR = ROOT_DIR / "eval_set"
SAMPLE_RATE = 16000

KINDS = {"wake", "negative", "command", "silence", "noise", "room"}
VOLUMES = {"whisper", "quiet", "normal", "loud"}
ENVIRONMENTS = {"quiet", "fan", "tv", "talk", "street", "cafe"}
SECTIONS = {"wake", "negative", "command", "silence"}
REQUIRED = {
    "id", "audio", "kind", "phrase", "speaker_id", "accent", "device_class",
    "environment", "distance_cm", "volume", "session_id", "source_group_id",
    "split", "consent",
}

WAKE_PHRASES = ["nexus", "hey nexus", "okay nexus"]

# (name, distance_cm, volume, environment, takes per phrase)  -> 30 takes/phrase
WAKE_CONDITIONS = [
    ("near-normal", 30, "normal", "quiet", 10),
    ("far-normal", 150, "normal", "quiet", 5),
    ("near-whisper", 30, "whisper", "quiet", 3),
    ("near-loud", 30, "loud", "quiet", 2),
    ("near-noisy", 30, "normal", "tv", 6),
    ("far-noisy", 150, "normal", "tv", 4),
]

# Soundalikes and everyday words that must NOT wake the assistant. 3 takes each.
NEGATIVE_PHRASES = [
    "next us", "lexus", "nixes", "texas", "next is", "necessary", "nexium",
    "hey next", "okay next", "hey nexis", "hello", "hey there", "okay google",
    "hey siri", "computer", "excuse me", "what time is it", "thank you",
    "play some music", "documentation created and synchronized",
]
NEGATIVE_TAKES = 3

# (expected_text, expected_intent): taken from src-tauri/tests/e2e_voice_fixture.rs
# (labels are those the production parser must resolve).
COMMANDS = [
    ("open chrome", "open_app"), ("open whatsapp", "open_app"), ("open brave", "open_app"),
    ("open notepad", "open_app"), ("open spotify", "open_app"), ("open calculator", "open_app"),
    ("open discord", "open_app"), ("close chrome", "close_app"), ("quit notepad", "close_app"),
    ("exit whatsapp", "close_app"), ("close spotify", "close_app"), ("close discord", "close_app"),
    ("chat with mummy", "whatsapp_chat"), ("open chat with papa", "whatsapp_chat"),
    ("search for cats", "search"), ("search flutter tutorial", "search"),
    ("play", "media_play_pause"), ("pause", "media_play_pause"),
    ("next song", "media_next"), ("previous song", "media_previous"),
    ("type hello world", "type_text"), ("press enter", "press_key"),
    ("new tab", "browser_new_tab"), ("open new tab", "browser_new_tab"),
    ("create a new tab", "browser_new_tab"), ("send it", "confirm_send"),
    ("ghost mode", "enter_ghost_control"), ("exit ghost mode", "exit_ghost_control"),
    ("hello", "greeting"), ("good morning", "greeting"),
    ("open settings", "open_settings"), ("open command center", "open_settings"),
    ("open architect", "open_architect"),
    ("list pull requests", "list_prs"), ("merge pr 23 in zync", "merge_pr"),
    ("order biryani", "order_food"), ("order pizza from dominos", "order_food"),
    ("search for shoes on amazon", "search_product"),
    ("send mummy a whatsapp message saying hi", "send_whatsapp_message"),
    ("tell mom i love you", "none"), ("what is the capital of france", "none"),
]
# Said in ONE breath after the wake word, no pause: "nexus open chrome".
ONE_BREATH = [
    ("open chrome", "open_app"), ("search for cats", "search"), ("new tab", "browser_new_tab"),
    ("play", "media_play_pause"), ("open settings", "open_settings"),
    ("close notepad", "close_app"), ("ghost mode", "enter_ghost_control"),
    ("order biryani", "order_food"), ("open spotify", "open_app"), ("pause", "media_play_pause"),
]
COMMAND_TAKES = 2

SILENCE_TAKES = 10      # 5 s of room tone, nobody speaking
NOISE_TAKES = 10        # 5 s of TV / other people talking, nobody addressing the device


class Item(dict):
    """One recording slot. A plain dict so it serialises straight to the manifest."""


def slug(text):
    return "".join(c if c.isalnum() else "-" for c in text.lower()).strip("-")


def _row(item_id, kind, phrase, speaker, session, device, accent, env, dist, vol, **extra):
    row = {
        "id": item_id,
        "audio": "",  # filled by the recorder
        "kind": kind,
        "phrase": phrase,
        "speaker_id": speaker,
        "accent": accent,
        "device_class": device,
        "environment": env,
        "distance_cm": dist,
        "volume": vol,
        "session_id": session,
        "source_group_id": f"{speaker}-{session}",
        "split": "test",
        "consent": True,
    }
    row.update(extra)
    return Item(row)


def build_plan(section, speaker, session, device, accent="unspecified"):
    """All recording slots for one speaker + section, in a stable order."""
    if section not in SECTIONS:
        raise ValueError(f"unknown section {section!r}")
    items = []
    if section == "wake":
        for phrase in WAKE_PHRASES:
            for cond, dist, vol, env, takes in WAKE_CONDITIONS:
                for t in range(1, takes + 1):
                    iid = f"{speaker}-{session}-wake-{slug(phrase)}-{cond}-{t:02d}"
                    items.append(_row(iid, "wake", phrase, speaker, session, device, accent, env, dist, vol,
                                      condition=cond, take=t, label="positive", seconds=2.5))
    elif section == "negative":
        for phrase in NEGATIVE_PHRASES:
            for t in range(1, NEGATIVE_TAKES + 1):
                iid = f"{speaker}-{session}-neg-{slug(phrase)}-{t:02d}"
                items.append(_row(iid, "negative", phrase, speaker, session, device, accent,
                                  "quiet", 30, "normal", take=t, label="negative", seconds=2.5))
    elif section == "command":
        for text, intent in COMMANDS:
            for t in range(1, COMMAND_TAKES + 1):
                iid = f"{speaker}-{session}-cmd-{slug(text)}-{t:02d}"
                items.append(_row(iid, "command", text, speaker, session, device, accent,
                                  "quiet", 40, "normal", take=t, label="command", seconds=4.0,
                                  expected_text=text, expected_intent=intent, one_breath=False))
        for text, intent in ONE_BREATH:
            for t in range(1, COMMAND_TAKES + 1):
                said = f"nexus {text}"
                iid = f"{speaker}-{session}-one-{slug(text)}-{t:02d}"
                items.append(_row(iid, "command", said, speaker, session, device, accent,
                                  "quiet", 40, "normal", take=t, label="command", seconds=4.0,
                                  expected_text=said, expected_intent=intent, one_breath=True))
    else:  # silence / noise
        for t in range(1, SILENCE_TAKES + 1):
            iid = f"{speaker}-{session}-silence-{t:02d}"
            items.append(_row(iid, "silence", "", speaker, session, device, accent,
                              "quiet", 40, "normal", take=t, label="background", seconds=5.0,
                              expected_text="", expected_intent="none"))
        for t in range(1, NOISE_TAKES + 1):
            iid = f"{speaker}-{session}-noise-{t:02d}"
            items.append(_row(iid, "noise", "", speaker, session, device, accent,
                              "tv", 150, "normal", take=t, label="background", seconds=5.0,
                              expected_text="", expected_intent="none"))
    return items


# ─── Manifest I/O and validation ─────────────────────────────────────────

def read_manifest(path):
    rows = []
    if not Path(path).exists():
        return rows
    with open(path, encoding="utf-8") as fh:
        for n, line in enumerate(fh, 1):
            line = line.strip()
            if line:
                try:
                    rows.append(json.loads(line))
                except json.JSONDecodeError as e:
                    raise ValueError(f"{path}:{n}: {e}") from e
    return rows


def append_manifest(path, row):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    with open(path, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(row, ensure_ascii=False) + "\n")


def wav_stats(path):
    """(sample_rate, channels, seconds, rms 0..1) for a PCM16 WAV, stdlib only."""
    with wave.open(str(path), "rb") as w:
        sr, ch, width, n = w.getframerate(), w.getnchannels(), w.getsampwidth(), w.getnframes()
        raw = w.readframes(n)
    if width != 2:
        raise ValueError("expected 16-bit PCM")
    import array
    a = array.array("h")
    a.frombytes(raw)
    if sys.byteorder == "big":
        a.byteswap()
    if not a:
        return sr, ch, 0.0, 0.0
    total = sum(float(s) * float(s) for s in a)
    rms = (total / len(a)) ** 0.5 / 32768.0
    return sr, ch, len(a) / ch / sr, rms


def validate(root, rows=None):
    """Return (errors, warnings). Errors make the set unusable for scoring."""
    root = Path(root)
    rows = rows if rows is not None else read_manifest(root / "manifest.jsonl")
    errors, warnings = [], []
    seen = set()
    for r in rows:
        rid = r.get("id", "<no id>")
        missing = REQUIRED - set(r)
        if missing:
            errors.append(f"{rid}: missing fields {sorted(missing)}")
            continue
        if rid in seen:
            errors.append(f"{rid}: duplicate id")
        seen.add(rid)
        if r["kind"] not in KINDS:
            errors.append(f"{rid}: bad kind {r['kind']!r}")
        if r["volume"] not in VOLUMES:
            errors.append(f"{rid}: bad volume {r['volume']!r}")
        if r["split"] != "test":
            errors.append(f"{rid}: held-out rows must have split='test' (got {r['split']!r})")
        if r["consent"] is not True:
            errors.append(f"{rid}: consent must be true")
        if r["kind"] == "command" and not r.get("expected_text"):
            errors.append(f"{rid}: command row without expected_text")
        p = root / r["audio"]
        if not r["audio"] or not p.exists():
            errors.append(f"{rid}: audio file missing ({r['audio']!r})")
            continue
        try:
            sr, ch, secs, rms = wav_stats(p)
        except Exception as e:  # noqa: BLE001
            errors.append(f"{rid}: unreadable WAV ({e})")
            continue
        if sr != SAMPLE_RATE:
            warnings.append(f"{rid}: sample rate {sr} (evaluator resamples, but record at {SAMPLE_RATE})")
        if r["kind"] in ("wake", "command") and rms < 0.003:
            warnings.append(f"{rid}: almost silent (rms {rms:.4f}) - re-record?")
        if r["kind"] == "wake" and secs < 0.8:
            warnings.append(f"{rid}: only {secs:.2f}s long")
    speakers = {r.get("speaker_id") for r in rows if r.get("kind") == "wake"}
    if rows and len(speakers) < 2:
        warnings.append(f"only {len(speakers)} speaker(s) with wake clips; the plan calls for the owner plus 2-3 others")
    return errors, warnings


def summarize(rows):
    """Counts the scorecard and the recorder both want to show."""
    out = {"total": len(rows), "by_kind": Counter(r.get("kind") for r in rows)}
    out["speakers"] = sorted({r.get("speaker_id") for r in rows if r.get("speaker_id")})
    out["wake_by_phrase"] = Counter(r.get("phrase") for r in rows if r.get("kind") == "wake")
    out["wake_by_speaker"] = Counter(r.get("speaker_id") for r in rows if r.get("kind") == "wake")
    out["wake_by_condition"] = Counter(r.get("condition") for r in rows if r.get("kind") == "wake")
    out["room_seconds"] = sum(float(r.get("seconds", 0)) for r in rows if r.get("kind") == "room")
    return out


# ─── Recording (needs sounddevice) ───────────────────────────────────────

def _record(seconds):
    import sounddevice as sd  # lazy: only the recorder needs it
    data = sd.rec(int(seconds * SAMPLE_RATE), samplerate=SAMPLE_RATE, channels=1, dtype="int16")
    sd.wait()
    return data.tobytes()


def _write_wav(path, pcm16_bytes):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SAMPLE_RATE)
        w.writeframes(pcm16_bytes)


def _prompt_text(item):
    k = item["kind"]
    if k == "wake":
        return (f'say "{item["phrase"].upper()}"   [{item["condition"]}: {item["distance_cm"]} cm, '
                f'{item["volume"]}, {item["environment"]}]  take {item["take"]}')
    if k == "negative":
        return f'say "{item["phrase"]}"  (should NOT wake)  take {item["take"]}'
    if k == "command":
        tail = "  - ONE breath, no pause" if item.get("one_breath") else ""
        return f'say "{item["expected_text"]}"{tail}  take {item["take"]}'
    if k == "silence":
        return "stay SILENT (room tone)"
    return "keep the TV / background talk running, do NOT address the device"


def cmd_record(args):
    root = Path(args.dir)
    session = args.session or datetime.now().strftime("s%Y%m%d")
    plan = build_plan(args.section, args.speaker, session, args.device, args.accent)
    done = {r["id"] for r in read_manifest(root / "manifest.jsonl")}
    todo = [i for i in plan if i["id"] not in done]
    print(f"{len(plan) - len(todo)}/{len(plan)} already recorded; {len(todo)} to go. Ctrl+C to stop - progress is saved.")
    for n, item in enumerate(todo, 1):
        while True:
            input(f"[{n}/{len(todo)}] {_prompt_text(item)}\n    Enter to record, then speak... ")
            pcm = _record(item["seconds"])
            tmp = root / "audio" / args.speaker / session / f"{item['id']}.wav"
            _write_wav(tmp, pcm)
            _, _, _, rms = wav_stats(tmp)
            print(f"    level rms={rms:.4f}" + ("  (very quiet!)" if rms < 0.003 and item["kind"] in ("wake", "command") else ""))
            ans = input("    [k]eep / [r]etry / [s]kip > ").strip().lower() or "k"
            if ans.startswith("r"):
                continue
            if ans.startswith("s"):
                tmp.unlink(missing_ok=True)
                break
            item["audio"] = str(tmp.relative_to(root)).replace("\\", "/")
            append_manifest(root / "manifest.jsonl", dict(item))
            break
    print("section complete.")


def cmd_room(args):
    root = Path(args.dir)
    session = args.session or datetime.now().strftime("s%Y%m%d")
    secs = int(args.minutes * 60)
    rid = f"{args.speaker}-{session}-room-{args.environment}-{datetime.now().strftime('%H%M%S')}"
    print(f"Recording {args.minutes} min of '{args.environment}' room audio. Do NOT say the wake word. Ctrl+C aborts.")
    pcm = _record(secs)
    path = root / "audio" / args.speaker / session / f"{rid}.wav"
    _write_wav(path, pcm)
    row = _row(rid, "room", "", args.speaker, session, args.device, args.accent, args.environment, 150, "normal",
               label="background", seconds=secs, take=1)
    row["audio"] = str(path.relative_to(root)).replace("\\", "/")
    append_manifest(root / "manifest.jsonl", dict(row))
    print(f"saved {row['audio']}")


def cmd_plan(args):
    total = Counter()
    for sec in sorted(SECTIONS):
        n = len(build_plan(sec, "speaker", "s0", "device"))
        total[sec] = n
        print(f"  {sec:<9} {n:>4} recordings per speaker")
    mins = sum(total[s] * 8 for s in total) / 60
    print(f"  total     {sum(total.values()):>4} per speaker (~{mins:.0f} min with prompts), plus 30+ min of room audio overall")


def cmd_validate(args):
    errors, warnings = validate(args.dir)
    for w in warnings[:50]:
        print("warning:", w)
    for e in errors[:50]:
        print("ERROR:", e)
    print(f"{len(errors)} error(s), {len(warnings)} warning(s)")
    return 1 if errors else 0


def cmd_stats(args):
    s = summarize(read_manifest(Path(args.dir) / "manifest.jsonl"))
    print(f"total clips: {s['total']}   speakers: {', '.join(s['speakers']) or '-'}")
    for k, v in sorted(s["by_kind"].items(), key=lambda kv: str(kv[0])):
        print(f"  {k:<10} {v}")
    print("wake clips by phrase:", dict(s["wake_by_phrase"]))
    print("wake clips by speaker:", dict(s["wake_by_speaker"]))
    print(f"room audio: {s['room_seconds'] / 60:.1f} min (target >= 30)")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dir", default=str(DEFAULT_DIR), help="evaluation set directory (default eval_set/)")
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("plan").set_defaults(fn=cmd_plan)
    sub.add_parser("validate").set_defaults(fn=cmd_validate)
    sub.add_parser("stats").set_defaults(fn=cmd_stats)
    for name, fn in (("record", cmd_record), ("room", cmd_room)):
        p = sub.add_parser(name)
        p.add_argument("--speaker", required=True, help="stable id, e.g. owner, friend1 (no real names needed)")
        p.add_argument("--device", default="unknown-mic", help="e.g. laptop-array, usb-condenser, bt-headset")
        p.add_argument("--accent", default="unspecified")
        p.add_argument("--session", default=None, help="session id (default: today). Use a NEW session per sitting.")
        if name == "record":
            p.add_argument("--section", required=True, choices=sorted(SECTIONS))
        else:
            p.add_argument("--minutes", type=float, default=10)
            p.add_argument("--environment", default="tv", choices=sorted(ENVIRONMENTS))
        p.set_defaults(fn=fn)
    args = ap.parse_args(argv)
    return args.fn(args) or 0


if __name__ == "__main__":
    sys.exit(main())
