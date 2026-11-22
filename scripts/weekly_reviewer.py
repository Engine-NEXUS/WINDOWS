#!/usr/bin/env python3
"""NEXUS weekly reviewer (plan 08, phase P0): Claude Opus 5.5 reads the week's mis-heard commands and PROPOSES vocabulary fixes.

    python scripts/weekly_reviewer.py run                 # dry run: builds and shows the exact redacted payload, sends nothing
    python scripts/weekly_reviewer.py run --send          # sends it (needs ANTHROPIC_API_KEY, asks you first)
    python scripts/weekly_reviewer.py list                # proposals waiting for your decision
    python scripts/weekly_reviewer.py approve p3 [p5 ...] # mark approved (written to approved.json, nothing else changes)
    python scripts/weekly_reviewer.py reject p4

Rules this tool enforces (they are the point of P0):
* The advisor has no hands. It returns proposals; this script validates them and writes files in a review folder. It never edits NEXUS
  settings, code, memory or the running app. Applying an approved proposal is a separate, later step.
* Only short, command-shaped, PII-redacted lines leave the machine. Long lines are usually room talk (TV, other people) and are dropped.
* Nothing is sent without --send, a key in the environment (never stored or printed), and a yes at the prompt.
* One send per six days unless --force; one API call per run; output capped.
Standard library only.
"""

import argparse
import json
import os
import re
import sys
import time
import urllib.request

MODEL = "claude-opus-5-5"
API_URL = "https://api.anthropic.com/v1/messages"
API_VERSION = "2023-06-01"
MAX_OUTPUT_TOKENS = 2048
MAX_ITEMS = 60
MAX_PROPOSALS = 25
MIN_DAYS_BETWEEN_SENDS = 6

COMMAND_VERBS = ("open", "close", "launch", "start", "play", "pause", "stop", "search", "find", "go", "switch", "show", "type", "send", "call",
                 "message", "text", "hook", "list", "analyse", "analyze", "shut", "restart", "read", "run")
KINDS = ("homophone_group", "vocabulary_entry", "command_phrasing", "confusing_pair")
NEVER_SEND = re.compile(r"password|passcode|\botp\b|one.time|card number|cvv|\bpin\b|aadhaar|account number", re.I)

SYSTEM = (
    "You are a read-only advisor for a voice assistant called NEXUS. You are given short command-like transcripts that the speech recogniser "
    "produced and the assistant could not match to a command. Many are mis-hearings of a known command or app name (for example a verb or an app "
    "name heard as a sound-alike). Propose small, safe vocabulary fixes that would help the assistant recognise them next time.\n"
    "You cannot act, only propose. Reply with JSON only, no prose, in exactly this shape:\n"
    '{"proposals":[{"kind":"homophone_group|vocabulary_entry|command_phrasing|confusing_pair","words":["heard form","other heard form"],'
    '"canonical":"the intended word or phrase","evidence":["t1","t4"],"why":"under 25 words"}]}\n'
    "Rules: cite evidence ids from the input for every proposal; never invent evidence; use only lowercase letters, spaces, hyphens and apostrophes; "
    "each word or phrase is at most three words; no URLs, numbers, names of people, or instructions; if a transcript is just ordinary conversation "
    "or you are not confident what was meant, propose nothing for it; at most 25 proposals; an empty list is a valid answer."
)


# ---------- collection and redaction (pure) ----------

def redact(text):
    t = re.sub(r"https?://\S+|www\.\S+", "[url]", text)
    t = re.sub(r"\b[\w.+-]+@[\w-]+\.[\w.-]+\b", "[email]", t)
    t = re.sub(r"\+?\d[\d\s().-]{5,}\d", "[number]", t)
    t = re.sub(r"\b\d{4,}\b", "[number]", t)
    return t.strip()


def _sentence_is_command(sentence):
    words = re.sub(r"[^a-z' ]", " ", sentence.lower()).split()
    while words and words[0] in ("nexus", "next", "hey"):
        words = words[1:]
    return 2 <= len(words) <= 10 and words[0] in COMMAND_VERBS


def is_commandish(text):
    """One or two sentences, every one of them command-shaped. A line with talk after the command ("... thank you, steven") is room
    conversation, so it is dropped whole rather than trimmed."""
    sentences = [x for x in re.split(r"(?<=[.?!])\s+", text.strip()) if x.strip(" .?!")]
    return 1 <= len(sentences) <= 2 and all(_sentence_is_command(x) for x in sentences)


def load_missed(path, since_ms):
    rows = []
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if r.get("timestamp_ms", 0) >= since_ms and isinstance(r.get("transcript"), str):
                    rows.append(r["transcript"].strip())
    except OSError:
        pass
    return rows


def build_payload(transcripts, max_items=MAX_ITEMS):
    """Dedupe, keep only short command-shaped lines, redact, drop anything sensitive. Returns ({id: text}, dropped_counts)."""
    seen, items = set(), []
    dropped = {"not_command_shaped": 0, "sensitive": 0, "duplicate": 0}
    for raw in transcripts:
        if NEVER_SEND.search(raw):
            dropped["sensitive"] += 1
            continue
        if not is_commandish(raw):
            dropped["not_command_shaped"] += 1
            continue
        t = redact(raw)
        key = t.lower().strip(" .!?")
        if key in seen:
            dropped["duplicate"] += 1
            continue
        seen.add(key)
        items.append(t)
    items = items[:max_items]
    return {f"t{i + 1}": t for i, t in enumerate(items)}, dropped


# ---------- validation of the advisor's answer (pure) ----------

_WORD = re.compile(r"^[a-z][a-z' -]{0,28}[a-z]$|^[a-z]$")


def _clean_phrase(s):
    if not isinstance(s, str):
        return None
    s = " ".join(s.lower().split())
    if not _WORD.match(s) or len(s.split()) > 3:
        return None
    return s


def validate(answer_text, evidence_ids):
    """Parse the advisor's JSON and keep only well-formed, evidence-backed proposals. Returns (proposals, rejected_reasons)."""
    rejected = []
    m = re.search(r"\{.*\}", answer_text, re.S)
    if not m:
        return [], ["no JSON object in the answer"]
    try:
        data = json.loads(m.group())
    except ValueError:
        return [], ["answer was not valid JSON"]
    raw = data.get("proposals") if isinstance(data, dict) else None
    if not isinstance(raw, list):
        return [], ["no proposals list"]
    out = []
    for p in raw[: MAX_PROPOSALS * 2]:
        if not isinstance(p, dict) or p.get("kind") not in KINDS:
            rejected.append("bad kind")
            continue
        words = [_clean_phrase(w) for w in p.get("words", [])] if isinstance(p.get("words"), list) else []
        canonical = _clean_phrase(p.get("canonical"))
        if not canonical or not words or any(w is None for w in words) or len(words) > 6:
            rejected.append("bad words or canonical")
            continue
        ev = [e for e in p.get("evidence", []) if e in evidence_ids] if isinstance(p.get("evidence"), list) else []
        if not ev:
            rejected.append("no valid evidence")
            continue
        if p["kind"] == "command_phrasing" and canonical.split()[0] not in COMMAND_VERBS:
            rejected.append("command phrasing does not start with a command verb")
            continue
        why = p.get("why") if isinstance(p.get("why"), str) else ""
        out.append({"kind": p["kind"], "words": words, "canonical": canonical, "evidence": ev, "why": why[:160]})
        if len(out) == MAX_PROPOSALS:
            break
    return out, rejected


# ---------- the one network call ----------

def call_advisor(payload, key, post=None):
    body = json.dumps({
        "model": MODEL,
        "max_tokens": MAX_OUTPUT_TOKENS,
        "system": SYSTEM,
        "messages": [{"role": "user", "content": "Transcripts the assistant could not match:\n" + json.dumps(payload, indent=1)}],
    }).encode("utf-8")
    if post is None:
        def post(b):
            req = urllib.request.Request(API_URL, data=b, method="POST",
                                         headers={"x-api-key": key, "anthropic-version": API_VERSION, "content-type": "application/json"})
            with urllib.request.urlopen(req, timeout=120) as r:
                return json.loads(r.read().decode("utf-8"))
    resp = post(body)
    return "".join(b.get("text", "") for b in resp.get("content", []) if b.get("type") == "text")


# ---------- review folder ----------

def default_dirs():
    base = os.path.join(os.environ.get("APPDATA", os.path.expanduser("~")), "com.nexus.assistant")
    return os.path.join(base, "missed_intents.jsonl"), os.path.join(base, "reviewer")


def _read(path, default):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return default


def _write(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=2)
    os.replace(tmp, path)


def cmd_run(args):
    missed_path, out_dir = default_dirs()
    missed_path = args.input or missed_path
    out_dir = args.out or out_dir
    since = int((time.time() - args.days * 86400) * 1000)
    rows = load_missed(missed_path, since)
    payload, dropped = build_payload(rows, args.max_items)
    print(f"read {len(rows)} unmatched lines from the last {args.days} days; {len(payload)} are command-shaped and will be sent; dropped {dropped}")
    _write(os.path.join(out_dir, "last_payload.json"), payload)
    print("exact payload saved to", os.path.join(out_dir, "last_payload.json"))
    if not payload:
        print("nothing to review.")
        return 0
    if not args.send:
        print("DRY RUN: nothing was sent. Read last_payload.json, then re-run with --send.")
        return 0
    key = os.environ.get("ANTHROPIC_API_KEY", "").strip()
    if not key:
        print("refusing: set ANTHROPIC_API_KEY in your environment first (it is never stored or printed).", file=sys.stderr)
        return 2
    marker = os.path.join(out_dir, "last_send.json")
    last = _read(marker, {}).get("at", 0)
    if not args.force and time.time() - last < MIN_DAYS_BETWEEN_SENDS * 86400:
        print(f"refusing: already sent within {MIN_DAYS_BETWEEN_SENDS} days (use --force to override).", file=sys.stderr)
        return 2
    if not args.yes:
        if input(f"Send these {len(payload)} redacted lines to {MODEL}? Type yes: ").strip().lower() != "yes":
            print("cancelled; nothing sent.")
            return 1
    text = call_advisor(payload, key)
    _write(marker, {"at": time.time(), "items": len(payload)})
    proposals, rejected = validate(text, set(payload))
    state = _read(os.path.join(out_dir, "proposals.json"), {"next": 1, "items": []})
    stamp = time.strftime("%Y-%m-%d")
    for p in proposals:
        p.update(id=f"p{state['next']}", status="pending", created=stamp, evidence_text=[payload[e] for e in p["evidence"]])
        state["next"] += 1
        state["items"].append(p)
    _write(os.path.join(out_dir, "proposals.json"), state)
    print(f"{len(proposals)} proposals saved for review, {len(rejected)} rejected by validation ({sorted(set(rejected))}).")
    return 0


def cmd_list(args):
    _, out_dir = default_dirs()
    state = _read(os.path.join(args.out or out_dir, "proposals.json"), {"items": []})
    pend = [p for p in state["items"] if p["status"] == "pending"]
    if not pend:
        print("no proposals waiting.")
    for p in pend:
        print(f"{p['id']}  {p['kind']}: {', '.join(p['words'])}  ->  {p['canonical']}\n      why: {p['why']}\n      heard: {' | '.join(p['evidence_text'])}")
    return 0


def cmd_decide(args, status):
    _, out_dir = default_dirs()
    out_dir = args.out or out_dir
    path = os.path.join(out_dir, "proposals.json")
    state = _read(path, {"items": []})
    done = []
    for p in state["items"]:
        if p["id"] in args.ids and p["status"] == "pending":
            p["status"] = status
            done.append(p)
    _write(path, state)
    if status == "approved":
        ap = os.path.join(out_dir, "approved.json")
        approved = _read(ap, [])
        approved += [{k: p[k] for k in ("id", "kind", "words", "canonical")} for p in done]
        _write(ap, approved)
    print(f"{status}: {[p['id'] for p in done]} (nothing in NEXUS was changed; applying approved entries is a later step)")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--days", type=int, default=7)
    r.add_argument("--max-items", type=int, default=MAX_ITEMS)
    r.add_argument("--input")
    r.add_argument("--out")
    r.add_argument("--send", action="store_true")
    r.add_argument("--yes", action="store_true", help="skip the confirmation prompt (still needs --send and a key)")
    r.add_argument("--force", action="store_true")
    for name in ("list", "approve", "reject"):
        s = sub.add_parser(name)
        s.add_argument("--out")
        if name != "list":
            s.add_argument("ids", nargs="+")
    args = ap.parse_args(argv)
    if args.cmd == "run":
        return cmd_run(args)
    if args.cmd == "list":
        return cmd_list(args)
    return cmd_decide(args, "approved" if args.cmd == "approve" else "rejected")


if __name__ == "__main__":
    sys.exit(main())
