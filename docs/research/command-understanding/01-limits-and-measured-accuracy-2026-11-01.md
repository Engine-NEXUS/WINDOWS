# Command understanding: limits and measured accuracy (2026-11-01)

Goal from the owner: no limits. Any command, in any wording, understood the first time and executed with the right details. NEXUS should learn the owner's own habitual phrasing and match a new phrasing to the closest known command.

## 1. How a command is understood today

1. `parse_deterministic` in `intent_parser.rs` (12,000 lines): hand-written exact phrase patterns, one parser per command.
2. If that misses and the sidecar is up: the BERT-Mini NLU model (55 intents). Accepted only at confidence 0.85 or higher (`nlu_client.rs:51`). Needs a Python sidecar (lazy start, 15 s timeout, 60 s cooldown after a failure).
3. Admin-only Qwen brain (not in family builds).
4. Otherwise the sentence goes to the cloud chat model, which answers as conversation. The owner experiences this as "it did not understand, I have to say it again".

## 2. What learns today (and what does not)

| Mechanism | Learns | Limit |
|---|---|---|
| `memcore/corrections.rs` | single-word STT fixes ("no, I said X") | applied only after two sightings, only to command-shaped text; word level, not meaning |
| `memcore/routines.rs` | sequences of open-app/site/music | only those three action types |
| `intents.yaml` | phrases the owner writes by hand | manual; only "open app" + "say" |
| `power_cmd` / `wa_open` aliases | mishearings of shutdown and contact names | two commands only |
| `improve.rs` + `missed_intents.jsonl` | counts repeated misses | record only: nothing is ever fed back |

Nothing learns "this new sentence means the same as what I usually say", and nothing uses the owner's own history to choose between candidates.

## 3. Measured accuracy

Probe: 146 natural phrasings of 36 commands (GitHub-heavy: canonical, casual, polite, partial, spoken numbers, mishearings). Script: `scripts/phrasing_probe/pipeline_probe_set.py`; run through `cargo test --test phrasing_probe -- --ignored` (deterministic stage) and the real `nexus_nlu.onnx` (BERT stage), with the 0.85 gate. This is my phrase set, not a random sample of your speech, so treat the percentages as a stress test, not a forecast.

| Stage | Result |
|---|---|
| Deterministic parser alone | 60 / 146 right (41 %), 67 clean misses, 19 wrong action |
| BERT alone (argmax) | 73 / 146 (50 %) |
| Full pipeline (deterministic, then BERT at 0.85, else chat) | **74 / 146 right (51 %), 26 wrong action (18 %), 46 fall to chat (31 %)** |

Weakest groups (right / tried): create PR 0/3, workflow runs ("did the build pass", "is the ci green") 0/5, add collaborator 0/2, find PR 0/3, files in a PR 0/3, "analyse the latest PR" 1/5, approve PR 1/5, get PR 1/4, analyse repo 4/12. Strongest: GitHub dashboard 4/4, close/open/search/WhatsApp/music 90-100 %.

### Wrong actions (worse than a miss)
- "close pr 254", "close the pr 34", "shut pr 254" are parsed as **close app** (would try to close a program called "pr 254").
- "analyse the latest pr" is parsed as analyse repo.
- "list pull requests", "show me open prs", "show me the prs" open the GitHub dashboard instead of listing.
- "show workflow runs", "show me all branches", "show me the releases", "show branch status" are parsed as **open app** (they try to launch a program with that name).
- "navigate to secunderabad station" becomes open app; "code rabbit review pr 254" becomes find PR.

### Right answer, thrown away
About 12 phrases were classified correctly by BERT at 0.65 to 0.83 and then discarded by the 0.85 gate ("merge number 254" 0.76, "approve pr 254" 0.65, "approve the pr 34" 0.83, "give priya access to servx" 0.82, "revert pr 254" 0.78, "add a comment to pull request 12..." 0.65). They went to chat.

### Real log (your own speech)
`missed_intents.jsonl`: 222 entries. After removing background noise and TV, the real commands it missed include: "merge pr statistics in nexus engines" (merge PR with a mishearing), "nexus comment hub" and "command hop" (command hub), "shed down my laptop", "cancel the ghost mode", "the post mode", "shift to tab to", "wave browser", "analyzing repository from my github for me", "let us analyze this repository" (fixed today). Each one was repeated by you afterwards, and that repetition is exactly the signal nothing uses.

## 4. Why a bigger regex list cannot reach "no limit"
Every wording needs a hand-written pattern, and patterns collide (the `close pr` / `close app` and `show ... / open app` errors above). The order of 12,000 lines of parsers also decides who wins. We need understanding by meaning, with the owner's history deciding ties.

## 5. Design (proposed, not built)

**A. A command catalogue as data, one entry per command** (name, slots with types, example phrasings, risk level). The 40 GitHub commands plus every app/web/message command. This replaces "the parser order decides".

**B. Meaning match in Rust, no sidecar:** a small multilingual sentence embedding model (ONNX, about 25-90 MB, runs in-process like the wake word models) embeds the sentence and finds the nearest catalogue entries and the nearest sentences the owner has said before. Output: ranked candidates with a score. Works for wordings nobody wrote a pattern for, and survives mishearings because the match is on meaning.

**C. Slot filling by rules, checked against reality:** repo names against the real repo list (`github_repo_index.rs`), PR numbers (including spoken "two fifty four"), contacts against the contacts book, apps against the app registry. A candidate whose slots do not resolve is rejected, which is what stops "close pr 254" becoming close app.

**D. Learn from the owner automatically:**
1. Every successful command stores (sentence, command, slots) in the sealed memory store.
2. After a miss, the next utterance within about 20 s that does resolve is paired with the missed one (the repeat is the label). Two such pairings, or one plus a confirmed "yes", make the missed wording a permanent alias.
3. The owner's frequency and recency re-rank candidates: what you say every day beats a rare near-match.
4. A visible list in the Memory page (what NEXUS learned, remove any item).

**E. Safety by risk, not by confidence alone:**
- Read/open commands: run when the best candidate is clearly ahead.
- Change commands (merge, close, delete, send): run only when the slots are all resolved and read back once ("Merge pull request 254 in zync, sir?"), unless the owner has confirmed that exact phrase before.
- Unsure between two candidates: one short question naming both, never a generic "say again", and the answer is learned.

**F. Never silent chat for a command-shaped sentence:** if it starts like a command (verb + noun we know), it is a command attempt; the fallback is the question in E, not conversation.

**G. A permanent scorecard:** this probe, extended with your real log, runs in CI and prints the table in section 3, so every change shows its effect on accuracy.

### Order of work and honest estimate
1. **Quick fixes in the current system (about a day):** lower the BERT gate to 0.6 when all slots resolve and the command is read-only or confirmed; fix the `close pr` / `show ... runs` / `show prs` / `navigate to` collisions; add the missing GitHub phrasings; wire `missed_intents` pairs into learned aliases. Expected: 51 % to roughly 75 % on this probe. Not "no limits", but immediate.
2. **Catalogue + slot validation (C, A):** removes the wrong-action class.
3. **Embedding match + owner history (B, D, E):** the real step toward understanding anything. Needs a model choice (licence, size, language coverage, RAM budget) and a measurement on your recordings before we promise a number.
4. **Scorecard (G)** first, so each step is measured.

### What I cannot promise
"Perfect" cannot be proven; what can be promised is a measured number on your own speech and a rule that a wrong action is worse than a question. The speech-to-text step (Groq Whisper) also mishears ("rap up", "command hop"); meaning match helps, but a sentence whose words are lost cannot be recovered. Everything above is unbuilt and unverified live.
