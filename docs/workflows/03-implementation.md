# 3. Implementation

Branches and PRs: `feat/multi-flow-workflows` (PR #73: F0–F2 + review fixes, the "sir" guard, the sidebar freeze
fix, scanner fixes) and `feat/flow-routing-f3` (this phase: smarter routing, follow-up questions, cloud chooser,
stop inside slow steps).

## 3.1 Files

| File | What it holds |
|---|---|
| `src-tauri/src/flows/mod.rs` | `Value`, `CapSpec` / `CAPS`, `user_only`; `Flow` / `SlotDef` / `StepDef`, `validate`, `builtin`; router (`split_clause_pairs`, `core`, `classify`, `whatsapp_parts`, `route`, `missing_for`); follow-up logic (`answer_for`, `is_cancel_reply`); cloud chooser logic (`worth_asking_cloud`, `chooser_system`, `parse_choice`); sink rules (`is_allowed_maps_url`, `validate_maps_destination`); executor (`FlowRun`, `Next`, `Progress`, `Runner`, `DryRunner`); `shadow`; unit tests |
| `src-tauri/src/flows/live.rs` | Runtime: `try_fill_slot`, `try_run`, `start`, `resume`, `discard`, the `PENDING` token map, `SLOT_WAIT`, `drive`, `precheck`, `resolve_contact`, `find_place`, `run_cap` (the real adapters), `cloud_choose`; unit tests for tokens, contacts, sender label |
| `src-tauri/src/flows/builtin/mail_then_whatsapp.json` | Flow A |
| `src-tauri/src/flows/builtin/mail_then_maps.json` | Flow B |
| `src-tauri/src/orchestrator.rs` | `try_fill_slot` right after the turn context is known; `try_run` before compound planning; in `orchestrator_mcp_confirm` a `nexus-flow` "no" discards the token and a "yes" calls `resume`; `speak_prompt_and_hold` is now `pub(crate)` |
| `src-tauri/src/memcore/explain.rs` | `message_parts`, `place_from_parts`, `extract_place`, `header_value` |
| `src-tauri/src/memcore/google_io.rs` | `gmail_full` (one raw message, memory only) |
| `src-tauri/src/commands.rs` | `NexusSettings.flows_live` (false), `flows_and_split` (true), `flows_cloud_chooser` (false) |
| `frontend/src/settings-sidebar/hub/featureRegistry.ts` | Hub rows `flows-live`, `flows-and-split`, `flows-cloud-chooser` |
| `src-tauri/tests/fixtures/flows/` | `corpus_v0.jsonl` (F0), `paraphrase_dev.jsonl`, `paraphrase_heldout.jsonl` |
| `src-tauri/tests/flow_probe.rs` | F0 offline probe |

## 3.2 Turn lifecycle

1. **Follow-up check** (`try_fill_slot`, runs before every other decision on the turn). If a flow is waiting for a
   detail (at most 90 seconds) and the voice is not uncertain or rejected:
   - "cancel" / "never mind" / "no" / "stop" → "Okay, I won't do it, sir."
   - a new workflow sentence → the wait is dropped and the sentence routes normally
   - otherwise the reply fills the missing slot (`answer_for` strips "saying", "tell him", "to", keeps your case);
     an unusable reply → "I didn't catch that, sir, so I haven't done anything."
2. **Routing** (`try_run`). L1 `route` with the "and" setting. If nothing matched and `flowsLive` +
   `flowsCloudChooser` are on and `worth_asking_cloud` is true → `cloud_choose` (4-second timeout).
   `flowsLive` off → `shadow` logs the decision and the turn continues normally.
3. **Start** (`start`). New request (fresh cancel flag). Uncertain/rejected voice → refused aloud. Missing slot →
   stored in `SLOT_WAIT`, question asked through `speak_prompt_and_hold` (the frontend speaks it and opens one
   listening window). Flows with a `contact_number` slot → `resolve_contact` (saved fact, then contacts book with
   learned aliases and kinship synonyms; ambiguous or unknown → spoken reason).
4. **Run** (`drive`). Loop: cancel? → stop silently. `next_action`:
   - `Run` → `run_cap`; a stop that arrived during the step wins over its result
   - `Confirm` → `precheck` (WhatsApp must be linked), `stash` the run under a fresh token, emit the normal
     `Confirm` event and the sidebar card with `{"kind":"mcp","server":"nexus-flow","params":{"nonce":…}}`
   - `Failed` / `Done` → speak what happened, including partial success ("I finished 1 of 3 steps. Then: …")
5. **Answer**. The frontend's existing confirm path calls `orchestrator_mcp_confirm`. For `nexus-flow`: "no" →
   `discard`; "yes" → `resume` → `take` (unknown/used → silent; older than 10 minutes → "That was a while ago…")
   → `approve` → `drive` again under a fresh cancel flag.

## 3.3 The router in detail

- `split_clause_pairs(text, split_and)` lowercases for matching but keeps the original text at the same byte
  offsets (when lowercasing changes a byte length, both use the lowercase text). Strong separators: ", and then",
  " and then", ", then", " then", " after that", "; ", ", also ", " and also ", "? ". A plain " and " splits only if
  both sides start with a command verb (`VERBS`, after polite openers like "please", "could you", "can you",
  "nexus"). Leading connector words left by a split ("then", "and", "also") are skipped; trailing punctuation is
  trimmed.
- `classify(low, orig)`:
  - `whatsapp_send`: first verb send/message/text/tell/whatsapp/ping/drop/shoot/write, and "whatsapp" in the clause.
    Contact from "… to NAME", or the words after the verb up to a filler ("a message", "a test message", "a text",
    "a whatsapp", …). 1–3 plain words, never a filler word. Body after "saying" / "that says" / "says"; "test
    message" when the clause says so.
  - `maps_from_mail`: a map word (maps, map, directions, navigate, route, "take me to", "get me to", "how do i get
    to"), a reference to the mail ("in the email", "from that email", "in the latest email", "in it", "it mentions",
    "mentioned in", …) and a fitting verb.
  - `mail_check`: a mail word (mail, mails, email, emails, inbox, gmail), a fitting verb (check, read, open, show,
    see, look, go, any, scan, have) and no map word.
- `route` requires the clause kinds, in order, to equal an enabled flow's `signature`; anything unclassified means
  "not a flow".

## 3.4 The cloud chooser in detail

- `worth_asking_cloud`: at least 5 words, a mail word, and a WhatsApp word or a place/map word.
- `chooser_system`: fixed instructions plus `- id: description` for each enabled flow. The user turn is the sentence.
- Call: `router::route_with_system` (your Groq → Gemini → Cerebras keys), 120 tokens, 4-second timeout.
- `parse_choice`: the JSON between the first `{` and last `}`; `flow` must be an enabled id; `contact` and `body`
  are kept only if they occur in the sentence (case-insensitive), and the value is taken from the sentence in your
  case; a contact must also pass the plain-name check. Then the normal pipeline runs: missing slots are asked,
  contacts are resolved, one confirmation.

## 3.5 Real adapters

| Capability | What it does |
|---|---|
| `mail.check` | Reads the mail the inbox watcher already filed as important (`briefing::important_mail`), speaks "You have N important emails…", outputs the top line (untrusted) and its Gmail id |
| `mail.extract_place` | Looks in the mail just read (trying every connected account), then the 5 newest inbox mails of every connected account; first Maps link (read from `href`s before markup is removed) or `Location:`/`Venue:`/`Address:` line wins; checks cancel between mail fetches; origin = "an email from <sender>" |
| `whatsapp.send` | MCP `send_message` with the **number** from the contacts book and the body; "Sent, sir." |
| `maps.open_route` | Opens the allowed link, or a Google Maps directions URL for plain place text, in the existing no-IPC Maps window |

## 3.6 Logs (`[FLOW]` lines)

| Line | Meaning |
|---|---|
| `shadow route chosen=… -> would ask before the first side effect` | `flowsLive` off: what would have happened |
| `live route layer=L1\|L3 chosen=… slots=[…] missing=[…] owner=…` | A flow started (slot names only, never values) |
| `follow-up answer filled slot=body flow=…` | A follow-up reply was used |
| `cloud chooser (Groq) -> Some(("mail_then_whatsapp", ["body","contact"]))` / `-> None` | What the chooser decided |
| `confirmed, resuming` | A valid "yes" |
| `confirmation ignored: nothing is waiting for it` | A used, discarded or forged token |
| `confirmation expired, nothing was done` | Older than 10 minutes |
| `Stopped, sir. I finished N of M steps.` | A stop cancelled the flow (logged, not spoken) |

Mail content and message text are never logged.

## 3.7 Other changes shipped alongside (PR #73)

- **"Sir" guard**: butler mode never addresses you by name (prompt rule in 9Router and the Worker, plus
  `persona::enforce_address` on every Result).
- **Sidebar freeze fix**: `DEV_KEEP_WINDOWS_ALIVE` set to `false` (a hidden sidebar shown again froze the app).
- **Reverse-engineer scanner**: a manifest link alone is no longer a Progressive Web App; the bare word "auth0" is
  a weak hint.
