#!/usr/bin/env python3
"""
NEXUS Brain Server — Qwen2.5-0.5B-Instruct local LLM for admin-only
intent classification, pronunciation learning, and phrasing generation.

Runs on port 39219 (separate from STT 39217 and NLU 39218).
Always loaded (no idle timeout) — admin only.

The brain:
  1. Classifies transcripts the deterministic parser + BERT-Mini miss
  2. Generates alternative phrasings for training BERT-Mini
  3. Cross-validates mispronunciations (e.g. "zys" -> "zync" via PR number lookup)
  4. Builds a personal pronunciation map that grows over time

Usage:
  python brain_server.py
  curl -X POST http://127.0.0.1:39219/classify -H "Content-Type: application/json" -d '{"text":"analyse pr 254 in zys"}'
"""

import json
import os
import sys
import time
import re
from pathlib import Path
from typing import Optional

import uvicorn
from fastapi import FastAPI
from pydantic import BaseModel

# ─── Config ────────────────────────────────────────────────────────────────

PORT = 39219
MODEL_DIR = Path(__file__).parent / "brain" / "model"
MODEL_PATH = MODEL_DIR / "qwen2.5-0.5b-instruct-q4_k_m.gguf"

# Known repos for cross-validation (matches intent_parser.rs KNOWN_REPOS)
KNOWN_REPOS = [
    "servx", "zync", "congi", "eesh264", "nexus-agent", "ultron",
    "nexus", "myrepo", "zync-meet/zync", "eesh264/congi",
]

# All 46 intent labels (must match train.py and nlu_server.py)
ALL_INTENTS = [
    "open_app", "open_url", "close_app", "whatsapp_chat",
    "open_architect", "search", "media_play_pause", "media_next",
    "media_previous", "media_stop", "greeting",
    "analyse_repo", "analyse_pr", "analyse_latest_pr", "check_branch",
    "merge_pr", "approve_pr", "close_pr", "list_prs", "get_pr",
    "create_pr", "update_branch", "revert_pr", "list_pr_files", "comment_pr",
    "add_collaborator", "remove_collaborator", "list_collaborators",
    "add_org_member", "remove_org_member", "list_org_members",
    "delete_branch", "list_branches", "create_release", "list_releases",
    "list_workflows", "list_workflow_runs", "rerun_workflow", "cancel_workflow",
    "unknown",
]

# ─── System Prompt ─────────────────────────────────────────────────────────

SYSTEM_PROMPT = """You are NEXUS's intent classifier brain. Given a voice transcript (possibly with mispronunciations), output ONLY valid JSON.

Intent labels (choose one):
  open_app, open_url, close_app, whatsapp_chat, open_architect, search,
  media_play_pause, media_next, media_previous, media_stop, greeting,
  analyse_repo, analyse_pr, analyse_latest_pr, check_branch,
  merge_pr, approve_pr, close_pr, list_prs, get_pr, create_pr,
  update_branch, revert_pr, list_pr_files, comment_pr,
  add_collaborator, remove_collaborator, list_collaborators,
  add_org_member, remove_org_member, list_org_members,
  delete_branch, list_branches, create_release, list_releases,
  list_workflows, list_workflow_runs, rerun_workflow, cancel_workflow,
  unknown

Slot types:
  app_name, url, contact, query, repo, owner, pr_number, author,
  username, org, branch, release_tag, workflow_id, title, head, base, body

IMPORTANT: Always fill in ALL relevant slots from the transcript. Put the SPOKEN value in slots (even if mispronounced), and put the corrected value in corrected_repo.

Output format (JSON only, no markdown, no explanation):
{"intent":"<label>","slots":{"<name>":"<value>"},"confidence":0.0-1.0,"corrected_repo":null}

Examples:
"open whatsapp" -> {"intent":"open_app","slots":{"app_name":"whatsapp"},"confidence":0.98,"corrected_repo":null}
"analyse pr 254 in zys" -> {"intent":"analyse_pr","slots":{"pr_number":"254","repo":"zys"},"confidence":0.92,"corrected_repo":"zync"}
"merge pr 23 in owner/repo" -> {"intent":"merge_pr","slots":{"repo":"owner/repo","pr_number":"23"},"confidence":0.95,"corrected_repo":null}
"close chrome" -> {"intent":"close_app","slots":{"app_name":"chrome"},"confidence":0.97,"corrected_repo":null}
"what is the weather" -> {"intent":"unknown","slots":{},"confidence":0.90,"corrected_repo":null}"""

PHRASING_PROMPT = """You are NEXUS's training data generator. Given an intent and slots, generate {count} alternative ways to say this command, including:
  - Different verbs (open/launch/start/run for open_app, etc.)
  - STT mishearing variants (whatsapp->whats app, zync->zink/zys/zyns, architect->arcade mapper)
  - Filler word prefixes (and, so, but, then, please, hey)
  - Conversational variants

Output ONLY a JSON array of strings, no explanation.

Example:
Intent: analyse_pr, slots: {{"pr_number":"254","repo":"zync"}}
Output: ["analyse pr 254 in zync","analyse PR 254 zync","analyze pull request 254 in zync","and analyse pr 254 in zync","so analyse pr 254 zync","analyse the pr 254 in zync","deep analysis pr 254 in zync","analyse pr 254 in zink","analyse pr 254 in zys","analyse pr 254 in zinc","hey analyse pr 254 in zync","please analyse pr 254 in zync","analyse pull request 254 zync","but analyse pr 254 in zync","then analyse pr 254 in zync","analyse the pull request 254 in zync","analyse pr number 254 in zync","analyse pr 254 for zync","analyse pr 254 of zync","analyse pr 254 from zync"]"""

# ─── App ───────────────────────────────────────────────────────────────────

app = FastAPI(title="NEXUS Brain Server")

_llm = None
_pronunciation_map: dict[str, str] = {}  # mispronounced -> correct
_pronunciation_path = Path(os.environ.get(
    "APPDATA", str(Path.home() / ".local" / "share")
)) / "com.nexus.assistant" / "pronunciation_map.json"


def get_llm():
    """Lazy-load the Qwen model."""
    global _llm
    if _llm is None:
        if not MODEL_PATH.exists():
            raise RuntimeError(f"Brain model not found at {MODEL_PATH}")
        from llama_cpp import Llama
        print(f"[BRAIN] Loading Qwen 0.5B from {MODEL_PATH}...")
        start = time.time()
        _llm = Llama(
            model_path=str(MODEL_PATH),
            n_ctx=2048,           # 2K context is enough for intent classification
            n_threads=4,          # 4 CPU threads
            verbose=False,
            use_mlock=True,       # lock model in RAM (don't swap)
        )
        load_time = time.time() - start
        print(f"[BRAIN] Model loaded in {load_time:.1f}s")
    return _llm


def load_pronunciation_map():
    """Load the personal pronunciation map from disk."""
    global _pronunciation_map
    if _pronunciation_path.exists():
        try:
            with open(_pronunciation_path, "r", encoding="utf-8") as f:
                _pronunciation_map = json.load(f)
            print(f"[BRAIN] Loaded {len(_pronunciation_map)} pronunciation corrections")
        except Exception as e:
            print(f"[BRAIN] Warning: could not load pronunciation map: {e}")
            _pronunciation_map = {}


def save_pronunciation_map():
    """Save the personal pronunciation map to disk."""
    try:
        _pronunciation_path.parent.mkdir(parents=True, exist_ok=True)
        with open(_pronunciation_path, "w", encoding="utf-8") as f:
            json.dump(_pronunciation_map, f, indent=2, ensure_ascii=False)
    except Exception as e:
        print(f"[BRAIN] Warning: could not save pronunciation map: {e}")


def apply_pronunciation_corrections(text: str) -> str:
    """Apply known pronunciation corrections to a transcript."""
    words = text.lower().split()
    corrected = []
    for word in words:
        # Check exact match first
        if word in _pronunciation_map:
            corrected.append(_pronunciation_map[word])
        else:
            # Check fuzzy match against known mispronunciations
            matched = False
            for mispronounced, correct in _pronunciation_map.items():
                if levenshtein(word, mispronounced) <= 1 and len(word) >= 3:
                    corrected.append(correct)
                    matched = True
                    break
            if not matched:
                corrected.append(word)
    return " ".join(corrected)


def levenshtein(a: str, b: str) -> int:
    """Compute Levenshtein distance between two strings."""
    if len(a) < len(b):
        a, b = b, a
    if len(b) == 0:
        return len(a)
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a):
        curr = [i + 1]
        for j, cb in enumerate(b):
            curr.append(min(
                prev[j + 1] + 1,
                curr[j] + 1,
                prev[j] + (ca != cb),
            ))
        prev = curr
    return prev[-1]


def fuzzy_match_repo(spoken: str, known_repos: list[str]) -> Optional[str]:
    """Fuzzy match a spoken repo name against known repos.
    
    Rules:
      - Input must be at least 3 characters
      - At least 2 letters must match in order
      - Levenshtein distance ≤ 3
    """
    spoken = spoken.lower().strip()
    if len(spoken) < 3:
        return None
    
    best_match = None
    best_distance = 999
    
    for known in known_repos:
        known_lower = known.lower()
        # Extract repo name from owner/repo format
        if "/" in known_lower:
            known_lower = known_lower.split("/")[-1]
        
        # Count shared letters in order
        shared = 0
        ki = 0
        for c in spoken:
            while ki < len(known_lower):
                if known_lower[ki] == c:
                    shared += 1
                    ki += 1
                    break
                ki += 1
        
        if shared >= 2:
            dist = levenshtein(spoken, known_lower)
            if dist <= 3 and dist < best_distance:
                best_match = known
                best_distance = dist
    
    return best_match


def cross_validate_with_pr(spoken_repo: str, pr_number: str) -> Optional[str]:
    """Cross-validate a mispronounced repo using the PR number.

    If PR {pr_number} exists in a known repo that fuzzy-matches {spoken_repo},
    we can conclude the spoken_repo is a mispronunciation of that known repo.

    In a full implementation, this would call the GitHub API to check
    if PR {pr_number} exists in the candidate repo. For now, we use
    the known repos list + fuzzy matching.
    """
    # Try fuzzy match against known repos first
    match = fuzzy_match_repo(spoken_repo, KNOWN_REPOS)
    if match:
        return match

    # If no known repo matches, try common mispronunciation patterns
    # This will be expanded when the brain learns more corrections
    return None


def extract_slots_from_text(text: str) -> dict:
    """Fallback slot extraction using regex when the brain doesn't fill slots."""
    slots = {}
    text_lower = text.lower().strip()

    # Extract PR number
    pr_match = re.search(r'\bpr\s+(?:number\s+)?(\d+)\b', text_lower)
    if pr_match:
        slots["pr_number"] = pr_match.group(1)

    # Extract repo (after "in", "of", "for", "from", "on", or at the end)
    repo_match = re.search(r'\b(?:in|of|for|from|on)\s+(\S+)\s*$', text_lower)
    if not repo_match:
        # Try "pr <num> <repo>" pattern
        repo_match = re.search(r'\bpr\s+(?:number\s+)?\d+\s+(\S+)\s*$', text_lower)
    if repo_match:
        slots["repo"] = repo_match.group(1)

    # Extract owner/repo
    owner_repo_match = re.search(r'\b(\S+)/(\S+)\s*$', text_lower)
    if owner_repo_match:
        slots["owner"] = owner_repo_match.group(1)
        slots["repo"] = owner_repo_match.group(2)

    # Extract app_name (after "open", "launch", "start", "run", "close", "quit")
    app_match = re.search(r'\b(?:open|launch|start|run|close|quit|exit|kill)\s+(.+?)(?:\s+(?:app|application|for me))?\s*$', text_lower)
    if app_match:
        slots["app_name"] = app_match.group(1).strip()

    # Extract query (after "search for", "google", "look up", "find")
    query_match = re.search(r'\b(?:search\s+for|search|google|look\s+up|find\s+me|find|look\s+for)\s+(.+?)\s*$', text_lower)
    if query_match:
        slots["query"] = query_match.group(1).strip()

    # Extract contact (after "chat with", "message", "send message to")
    contact_match = re.search(r'\b(?:chat\s+with|open\s+chat\s+with|message|send\s+message\s+to|send\s+whatsapp\s+to|whatsapp)\s+(.+?)(?:\s+on\s+whatsapp)?\s*$', text_lower)
    if contact_match:
        slots["contact"] = contact_match.group(1).strip()

    return slots


def extract_repo_from_text(text: str) -> str:
    """Extract just the repo name from a transcript."""
    text_lower = text.lower().strip()
    # After "in", "of", "for", "from", "on"
    repo_match = re.search(r'\b(?:in|of|for|from|on)\s+(\S+)\s*$', text_lower)
    if repo_match:
        return repo_match.group(1)
    # After "pr <num>"
    repo_match = re.search(r'\bpr\s+(?:number\s+)?\d+\s+(\S+)\s*$', text_lower)
    if repo_match:
        return repo_match.group(1)
    return ""


# ─── API Models ────────────────────────────────────────────────────────────

class ClassifyRequest(BaseModel):
    text: str
    known_repos: Optional[list[str]] = None


class ClassifyResponse(BaseModel):
    intent: str
    slots: dict
    confidence: float
    corrected_repo: Optional[str] = None
    original_text: str
    corrected_text: str
    latency_ms: float


class PhrasingRequest(BaseModel):
    intent: str
    slots: dict
    count: int = 20


class PhrasingResponse(BaseModel):
    phrasings: list[str]
    latency_ms: float


class PronunciationAddRequest(BaseModel):
    mispronounced: str
    correct: str


class PronunciationMapResponse(BaseModel):
    map: dict[str, str]
    count: int


# ─── Endpoints ─────────────────────────────────────────────────────────────

@app.get("/health")
async def health():
    return {
        "status": "ok",
        "model_loaded": _llm is not None,
        "pronunciation_count": len(_pronunciation_map),
        "model_path": str(MODEL_PATH),
        "model_exists": MODEL_PATH.exists(),
    }


@app.post("/classify", response_model=ClassifyResponse)
async def classify(req: ClassifyRequest):
    start = time.time()
    
    # Apply known pronunciation corrections first
    corrected_text = apply_pronunciation_corrections(req.text)
    
    # Build the prompt
    user_msg = f"Transcript: \"{corrected_text}\""
    
    # Get LLM response
    llm = get_llm()
    response = llm.create_chat_completion(
        messages=[
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_msg},
        ],
        max_tokens=200,
        temperature=0.1,       # low temperature for deterministic classification
        stop=["```", "\n\n\n"],
    )
    
    raw_output = response["choices"][0]["message"]["content"].strip()
    
    # Parse JSON from the response (handle markdown code blocks)
    try:
        # Strip markdown code fences if present
        if raw_output.startswith("```"):
            raw_output = re.sub(r"^```(?:json)?\s*", "", raw_output)
            raw_output = re.sub(r"\s*```$", "", raw_output)
        
        result = json.loads(raw_output)
        intent = result.get("intent", "unknown")
        slots = result.get("slots") or {}
        confidence = float(result.get("confidence", 0.5))
        corrected_repo = result.get("corrected_repo")
    except (json.JSONDecodeError, KeyError, TypeError):
        intent = "unknown"
        slots = {}
        confidence = 0.0
        corrected_repo = None
    
    # Fallback: if slots are empty but intent is analyse_pr, extract from transcript
    if not slots and intent in ("analyse_pr", "analyse_repo", "analyse_latest_pr", "check_branch",
                                 "merge_pr", "approve_pr", "close_pr", "get_pr", "revert_pr",
                                 "update_branch", "list_pr_files"):
        slots = extract_slots_from_text(corrected_text)
        # If we found a repo and corrected_repo is set, override
        if corrected_repo and "repo" in slots:
            slots["repo"] = corrected_repo

    # Cross-validate repo if the brain thinks it's mispronounced
    if corrected_repo and corrected_repo.lower() != slots.get("repo", "").lower():
        # The brain detected a mispronunciation
        spoken_repo = slots.get("repo", "")
        if not spoken_repo:
            # Try to extract from transcript
            spoken_repo = extract_repo_from_text(corrected_text)
        if spoken_repo:
            # Verify via cross-validation
            verified = cross_validate_with_pr(spoken_repo, slots.get("pr_number", ""))
            if verified:
                corrected_repo = verified
                slots["repo"] = verified
                # Add to pronunciation map
                _pronunciation_map[spoken_repo.lower()] = verified
                save_pronunciation_map()
                print(f"[BRAIN] Learned pronunciation: {spoken_repo} -> {verified}")
            elif corrected_repo:
                # Brain's correction is our best guess
                slots["repo"] = corrected_repo
                _pronunciation_map[spoken_repo.lower()] = corrected_repo
                save_pronunciation_map()
                print(f"[BRAIN] Learned pronunciation (from brain): {spoken_repo} -> {corrected_repo}")

    # Also check if the repo in slots fuzzy-matches a known repo
    repo_in_slots = slots.get("repo", "")
    if repo_in_slots and not any(repo_in_slots.lower() == r.lower() or
                                  repo_in_slots.lower() == r.split("/")[-1].lower()
                                  for r in KNOWN_REPOS):
        # Not an exact match — try fuzzy
        fuzzy_result = cross_validate_with_pr(repo_in_slots, slots.get("pr_number", ""))
        if fuzzy_result and fuzzy_result.lower() != repo_in_slots.lower():
            _pronunciation_map[repo_in_slots.lower()] = fuzzy_result
            save_pronunciation_map()
            slots["repo"] = fuzzy_result
            corrected_repo = fuzzy_result
            print(f"[BRAIN] Learned pronunciation: {repo_in_slots} -> {fuzzy_result}")
    
    latency_ms = (time.time() - start) * 1000
    return ClassifyResponse(
        intent=intent,
        slots=slots,
        confidence=confidence,
        corrected_repo=corrected_repo,
        original_text=req.text,
        corrected_text=corrected_text,
        latency_ms=latency_ms,
    )


@app.post("/generate_phrasings", response_model=PhrasingResponse)
async def generate_phrasings(req: PhrasingRequest):
    start = time.time()
    
    prompt = PHRASING_PROMPT.format(count=req.count) + f'\n\nIntent: {req.intent}, slots: {json.dumps(req.slots)}'
    
    llm = get_llm()
    response = llm.create_chat_completion(
        messages=[
            {"role": "system", "content": "You are a training data generator. Output ONLY a JSON array of strings."},
            {"role": "user", "content": prompt},
        ],
        max_tokens=500,
        temperature=0.7,       # higher temperature for variety
        stop=["```", "\n\n\n"],
    )
    
    raw_output = response["choices"][0]["message"]["content"].strip()
    
    # Parse JSON array
    try:
        if raw_output.startswith("```"):
            raw_output = re.sub(r"^```(?:json)?\s*", "", raw_output)
            raw_output = re.sub(r"\s*```$", "", raw_output)
        phrasings = json.loads(raw_output)
        if not isinstance(phrasings, list):
            phrasings = []
    except json.JSONDecodeError:
        phrasings = []
    
    latency_ms = (time.time() - start) * 1000
    return PhrasingResponse(
        phrasings=phrasings[:req.count],
        latency_ms=latency_ms,
    )


@app.get("/pronunciation_map", response_model=PronunciationMapResponse)
async def get_pronunciation_map():
    return PronunciationMapResponse(
        map=_pronunciation_map,
        count=len(_pronunciation_map),
    )


@app.post("/pronunciation_map/add")
async def add_pronunciation(req: PronunciationAddRequest):
    _pronunciation_map[req.mispronounced.lower()] = req.correct.lower()
    save_pronunciation_map()
    return {"status": "ok", "mispronounced": req.mispronounced, "correct": req.correct}


@app.delete("/pronunciation_map/clear")
async def clear_pronunciation_map():
    global _pronunciation_map
    _pronunciation_map = {}
    save_pronunciation_map()
    return {"status": "ok", "count": 0}


# ─── Main ───────────────────────────────────────────────────────────────────

def main():
    if not MODEL_PATH.exists():
        print(f"[BRAIN] ERROR: Model not found at {MODEL_PATH}")
        print("[BRAIN] Download with:")
        print("  python -c \"from huggingface_hub import hf_hub_download; hf_hub_download('Qwen/Qwen2.5-0.5B-Instruct-GGUF', 'qwen2.5-0.5b-instruct-q4_k_m.gguf', local_dir='server/brain/model')\"")
        sys.exit(1)
    
    # Load pronunciation map
    load_pronunciation_map()
    
    # Pre-load model
    print("[BRAIN] Pre-loading Qwen 0.5B model...")
    get_llm()
    
    print(f"[BRAIN] Ready. Listening on port {PORT}")
    print(f"[BRAIN] Pronunciation corrections: {len(_pronunciation_map)}")
    
    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="warning")


if __name__ == "__main__":
    main()
