# Feature 90 — About-Me Profile, Self-Learning Habits & the Memory Panel

**Goal (user's words):** the assistant remembers my chat, knows *who I am / what I can do / what I need / what my priorities are*, and learns from how I use it — on a free, BYOK, low-RAM stack.

## What it is

| Piece | File | Stored at | Used by |
|---|---|---|---|
| About-me **profile** (identity / can_do / needs / priorities) | `src-tauri/src/profile.rs` | `memory/profile.json` | injected into every cloud prompt (≤ 450 chars) |
| **Facts** ("favorite color is blue") | `memory.rs` (existing, hardened) | `memory/core.json` | injected when relevant; PII facts never leave the PC |
| **Conversation** memory | `conversation.rs` (existing, fixed) | `conversation.jsonl`, `conversation_brief.json` | injected as "Recent" + brief (≤ 1500 chars) |
| **Habits** (self-learning) | `src-tauri/src/habits.rs` | `memory/habits.json` | **never** injected; only produces suggestions |
| **Memory panel** | `memory_ipc.rs`, `frontend/src/settings-sidebar/MemoryTab.tsx` | — | view / add / delete / clear |

Everything is a small local file written atomically (temp file + rename; a corrupt file is kept as `*.corrupt`, never silently replaced). No embeddings, no LLM calls, no native deps → **RAM impact ≈ 0**; prompt overhead ≤ ~575 tokens per cloud turn.

## Voice grammar (deliberately narrow)

Ordinary speech must never write memory ("I am hungry", "I need help", "remember that we need milk" are ignored).

| Say | Effect |
|---|---|
| "my priority is …" / "my top priority is …" / "my priorities are …" / "set a priority: …" / "my goal is …" | adds a **priority** (cap 5, newest kept) |
| "remember that I am …" | **identity** |
| "remember I can …" / "…I know how to …" | **can do** |
| "remember that I need …" / "…I have to …" / "…I want to …" | **needs** |
| "what do you know about me" / "who am I" | speaks the profile (private entries are not read aloud) |
| "what are my priorities" / "what should I work on today" | speaks priorities |
| "forget my priority …" / "forget that I am …" | removes matching entries |
| "clear my priorities" | clears priorities |
| "forget my favorite color" | removes the stored fact |
| "my name is …", "my dog is …", "remember that …" | stores a fact (`memory.rs`); bare `my X is Y` only for personal attributes (name, age, city, job, favorite_*, family, pets, …) |

Matching runs only on `Unknown` intents, after all session intercepts, so it can never steal a real command.

## Privacy model

- Entries whose text contains PII (phone, e-mail, card, Aadhaar, PAN, IPv4 — `pii_filter`) are flagged **sensitive** 🔒: kept locally, **never put in a cloud prompt, never read aloud** (TTS may be a cloud engine). NEXUS says so when it saves one.
- Stored facts containing PII are skipped at prompt-build time (previously they were injected raw on the Worker path and redacted into `[REDACTED:PHONE]` noise on 9Router).
- Habit counters record **only** app/site opens, media control and feature opens — never dictated text, search queries, message bodies or contact names (`orchestrator::local_turn_summary` is a whitelist).
- "Forget everything" / per-scope clear in the Memory panel deletes the files. `clear_transcript` (Settings → "Clear conversation history") now really deletes the stored conversation.

## Self-learning loop

1. Each handled local command bumps a counter (`app:chrome`, `site:youtube`, `media`, `feature:architect`) with an hour-of-day histogram.
2. A habit seen ≥ 3 times becomes a **suggestion** ("You've opened VS Code 14 times, usually around 9 AM — Make progress in VS Code").
3. The Memory panel shows suggestions with **Add as priority** / **Dismiss**. Nothing enters the profile (and therefore no prompt) unless the user accepts. Dismissed habits never return.

## Conversation memory fixes (what "remember my chat" now means)

- Cloud turns (9Router *and* Worker) get the profile + facts + brief + recent turns. The Worker fallback used to ignore all of it.
- Local commands are recorded (privacy-whitelisted) so "what did I just ask you to open?" is answerable.
- Age decay: turns older than 48 h (unresolved follow-ups 7 d) are not replayed; a brief not refreshed for 14 d is dropped. The brief's key points keep the **newest** entries.

## Tests

`profile.rs` (12), `habits.rs` (6), `memory.rs` (+6), `conversation.rs` (+3), `memory_ipc.rs` (2), `orchestrator::local_turn_summary`, Worker `general_prompt` (5), frontend `memoryModel` (5). See change 78 for the full verification record and the live-test checklist.

## Not built (yet)

- LLM-based daily consolidation of the conversation into profile suggestions (would use the user's own free key; opt-in).
- Embedding / semantic recall.
- Cross-device sync (everything is local by design).
