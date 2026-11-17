# Plan: Claude Opus 5.5 as an advisor that only plans (2026-11-17)

> **Superseded for runtime by [note 09](09-free-for-everyone-advisor-plan-2026-11-17.md).** NEXUS is open source and free for every user, so Opus 5.5 is a maintainer-side tool, not a per-user dependency. Track 2 (plan-only Opus in development work) and the validation rules below still stand.

Status: **plan only.** No code, settings or runtime changed. Reading of the request: Opus 5.5 should *advise and plan*, never act or edit; cheaper or free models do the work.

## What Anthropic's advisor pattern is (verified in the docs)

Source: [Advisor tool docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/advisor-tool), [announcement](https://claude.com/blog/the-advisor-strategy).

- A cheaper **executor** model calls a stronger **advisor** model mid-task for guidance. The advisor runs server-side (`advisor_20260301`, beta header `advisor-tool-2026-03-01`), sees the executor's whole transcript, and returns only guidance text.
- **The advisor cannot call tools**, so it cannot edit or execute anything by construction. Executor tokens bill at executor rates, advisor tokens at the advisor's rate; `max_uses` and `max_tokens` cap spend.
- `claude-opus-5-5` is a supported advisor for Opus 5.5, Sonnet 5.5 and Haiku 5.5 executors.
- **Two catches that shape this plan:**
  1. With Opus 5.5 as advisor the advice comes back **encrypted** (`advisor_redacted_result`): our code cannot read, log or validate it. Plaintext advice needs `claude-opus-4-8` as advisor, or a normal direct call to Opus 5.5.
  2. The advisor tool only exists on Anthropic's API, so it does **not** work with NEXUS's free executors (Groq, Cerebras, Gemini).
- Advisor failures return an error block and the executor simply carries on without advice, so the design must work without it.

## Decision: two tracks, one rule

**Rule for both: the advisor has no hands.** It receives text, returns advice, and nothing it says is executed without code checking it first. That is enforced by what it is given (no tools, no write access), never by an instruction saying "don't".

- **Track 1, inside NEXUS: Command Advisor.** Implement the pattern ourselves with a **plain direct call to `claude-opus-5-5`** (not the advisor tool), so the answer is readable, validatable and works with NEXUS's free executors.
- **Track 2, in development work: Plan-only Opus.** Opus 5.5 plans and reviews with read-only access; Sonnet 5.5, Haiku 5.5 or the free coding models implement. Here the native advisor tool is a good fit when the executor is a Claude model.

## Track 1: Command Advisor inside NEXUS

**Job.** Settle the few turns the local layers cannot: a misheard command with two close candidates, a compound command, a "no, I said…" complaint. It returns advice; NEXUS code decides and acts.

**Where it sits.** After the local sound-and-meaning layer from note 06 (prompt biasing, word confidence, phonetic slot matcher) and the Windows-style tiers from note 07. The advisor sees only what those layers could not resolve.

**When it is called (all must hold):**
1. The local matcher found no candidate above the accept margin, **or** two within the ask margin.
2. The turn looks like a command (starts with a command verb) and is not safety-gated (delete, send, shut down, purchase keep their existing confirmation and never wait on an advisor).
3. The user has turned the advisor on, has an Anthropic key, is not in local-only speech mode, and today's budget is not spent.

**Never in the live path for:** ordinary commands the grammar already resolves, dictation clean-up, wake word, anything needing a sub-second answer.

**What it receives (minimal, redacted):** the transcript, the low-confidence words, **the closed candidate list with ids** the local matcher produced, the foreground app category, the last three commands. It passes through the existing PII filter and name redaction. It never gets the full app or contact list, keys, memory records, screen contents or audio.

**What it must return (strict JSON, nothing else):**
`{ "decision": "pick" | "ask" | "unknown", "pick": "<one of the given ids>", "alternatives": ["<ids>"], "question": "<short, spoken>", "confidence": 0..1, "reason": "<20 words>" }`

**What code does with it:** reject anything that is not valid JSON, any `pick` that is not in the supplied ids, any text outside the schema. `pick` above the confidence floor executes through the normal command path (all existing gates apply). `ask` speaks the numbered question. `unknown` falls back to the local "didn't catch that" flow. An error, timeout or budget stop behaves exactly as if the advisor were off.

**Latency rule: ask first, advise in parallel.** Voice cannot wait. Start the advisor call and the local numbered question together; if the advisor answers within about 1.2 s with a confident pick, use it, otherwise the user's answer wins and the advisor's result is only logged. This keeps the worst case equal to today's local behaviour.

**Cost and privacy controls.** Off by default; a Features-page switch; user's own key (BYOK, matching NEXUS's free-to-use stance); per-day and per-turn caps; a 60-second cooldown after two failures; a visible counter of calls today; no calls while meetings or local-only mode are on. Check current Opus pricing before setting the daily cap; this plan does not assume a price.

**Learning, safely.** Advisor decisions are never saved as corrections by themselves. Only an outcome the user confirmed (picked a number, or did not object) feeds the existing two-sighting learned-correction rule, with the Memory-page list and delete.

**Most valuable low-risk use: an offline weekly reviewer.** Once a week, off the voice path, send the week's misses (`missed_intents.jsonl`, rejected corrections, ask outcomes; redacted) to Opus 5.5 and get back *proposals*: new homophone groups, vocabulary entries, command phrasings, confusing pairs. They arrive as a list on the Memory page for the owner to approve or reject; nothing applies automatically. This costs a handful of calls per week, never touches latency, and improves every later turn.

**Measurement.** Use the confusion set from note 06 (200 or more slips, 200 or more must-not-change sentences). Replay offline in three arms: local only, local plus advisor, local plus free-model consult. Report corrected, wrongly corrected, asked and left alone, latency p95 and cost per 100 turns, each with numerator and denominator. **Ship rule: the advisor arm must not raise the wrong-action rate** and must resolve a meaningful share of what the local arm would have asked.

**Phases**

| Phase | What | Exit gate |
|---|---|---|
| P0 | Offline weekly reviewer on exported logs (no runtime change) | Owner approves or rejects proposals; at least one adopted entry measurably fixes a real slip |
| P1 | Offline replay of the confusion set, three arms | Wrong-action rate not worse; report written |
| P2 | **Shadow mode**: live advisor runs on ambiguous turns, result is logged only, never used (same pattern as the "practice" mode used for unattended shutdown) | A week of data; advisor agrees with the user's final choice at the agreed rate |
| P3 | Live "ask assist" behind the switch with the parallel-ask latency rule | Owner trial: no wrong actions, no added delay |
| P4 | Optional: compound-command planning advice (split a long spoken request into validated steps) | Only after P3 is stable; steps still go through existing gates |

**Risks and answers**

| Risk | Answer |
|---|---|
| Voice delay | Parallel ask; the advisor can only make a turn faster, never slower |
| Cost creep | Off by default, per-day cap, counter, kill switch |
| Privacy | Redacted minimal payload; no audio, memory, keys, screen; off in local-only mode |
| Advisor suggests something dangerous | It cannot act; picks must be supplied ids; safety actions keep their own confirmation |
| Prompt injection from heard speech | The transcript is data inside a fixed schema; output is validated; no free-text commands |
| Encrypted advice with the native tool | Track 1 uses direct calls, so output is readable and validated |
| Vendor outage or key missing | Falls back to the local layers; nothing depends on it |

## Track 2: Plan-only Opus 5.5 in development work

**Roles**
- **Advisor (Opus 5.5):** read-only. Allowed: read files, search, web research. Not allowed: edit, write, shell, git push, merge. Output: a written plan with goal, files to touch, ordered steps, the tests that prove each step, risks, and acceptance checks, plus a read-only review of the finished diff.
- **Executors:** Sonnet 5.5 or Haiku 5.5 (Claude models, so the native advisor tool applies), or the free coding models already used in this repo's workflow.

**How to run it**
1. Plan: ask Opus 5.5 for the plan in plan mode with a read-only tool allowlist; save the plan as a file in the repo (`docs/features/…`).
2. Execute: hand the saved plan to the executor; the executor edits, tests and commits.
3. Review: give Opus 5.5 the diff and test output, read-only, and ask for defects against the plan's acceptance checks.
4. If the executor is a Claude model and in-conversation advice is wanted, use the native tool: `type advisor_20260301`, `model claude-opus-5-5`, with `max_uses` (for example 3) and `max_tokens` set, called after orientation, before committing to an approach, and before declaring done. Because Opus 5.5 advice is encrypted, **keep the saved plan file as the audit record**, or use `claude-opus-4-8` as advisor where the text must be visible.

**Enforcement.** The "no editing" rule is a tool allowlist and the advisor tool's lack of tools, not a sentence in a prompt. Human approval stays at the merge.

## What I need from the owner to start

1. Confirm Track 1 should begin at **P0** (offline weekly reviewer; no runtime change) and Track 2 is a working agreement.
2. An Anthropic API key is needed only for P0 onward; none is needed to approve this plan.
3. Decide the daily call cap once current pricing is checked.
