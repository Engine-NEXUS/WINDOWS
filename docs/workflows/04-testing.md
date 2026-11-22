# 4. Testing

Everything below was run on 2026-11-21/22 on the owner's Windows laptop. Where a result is from a fake component,
the table says so.

## 4.1 Unit tests

Run: `cd src-tauri && cargo test --lib -- flows:: place_tests`

| Area | Tests (names abbreviated) |
|---|---|
| Loading | both shipped flows load and validate; every example in a flow file routes to that flow |
| Router | owner flows and their slots; message keeps the spoken case; other ways of saying who ("send ravi a message", "message Ravi Kumar", "whatsapp ravi saying"); a missing body is reported, not guessed; ordinary sentences and single commands are not flows; the "and" split needs verbs on both sides and has a kill switch |
| Paraphrase corpora | dev and held-out sets must stay perfect (`paraphrase_scores_are_reported`, prints `[flow-eval]` lines) |
| Validation | untrusted data into a recipient, a mail id into the number, untrusted text into the mail-id input, unknown capability, binding to a later step: all refused |
| Executor | Flow A reads mail, asks once, sends only after yes; cancelling at the read-back sends nothing; a recipient or number not from the user is refused at run time; Flow B passes the place with its source; the dry run never acts |
| Safety | hostile links never reach Maps (other hosts, `javascript:`, `http:`, `file:`, markup, `/maps/../url`, `%2e%2e`); lookalike hosts, credentials in URLs, ports, `goo.gl/maps`, backslashes refused; instruction text in a mail never adds a step; over-long and markup destinations refused; no shipped flow binds mail text to a recipient or number |
| Follow-up | answers fill the missing slot in the spoken case ("saying", "tell him", "to"); cancel replies recognised; non-names refused |
| Cloud chooser | only sentences touching two workflow areas qualify; the choice is checked against the user's words (hallucinated contact/body dropped, unknown flow / "none" / prose refused, fenced JSON accepted) |
| Tokens (`live.rs`) | a token works once; unknown/forged tokens find nothing; old tokens expire and a "no" discards; expired entries are swept |
| Contacts | no book → asks for a number; two "ravi" → ambiguous with both names; exact name → number; unknown → asks |
| Mail parsing (`explain.rs`) | Maps link inside an `href` found before markup is stripped (including `&amp;`); other links ignored, `Location:` line fallback; a Maps link in the footer is the sender's office and is skipped; headers read case-insensitively |

Result on the final code: **32 passed** (flows + place tests), plus the full-suite run in §4.5.

## 4.2 Phrase corpora and scores

Files: `src-tauri/tests/fixtures/flows/paraphrase_dev.jsonl` (30 lines: 10 Flow A, 8 Flow B, 12 "not a workflow")
and `paraphrase_heldout.jsonl` (12 lines: 5 A, 3 B, 4 none). Scored with the local router only (no cloud).

| Set | Before F3 | After F3 |
|---|---|---|
| Development | 16/30 (all 12 negatives right, 4/18 workflow phrasings) | **30/30** |
| Held-out | 8/12 | **12/12** |

Honesty note: the held-out set was written before the rule changes, but its misses were printed in the baseline
run, so it is not a fully blind test. Both sets were written by the developer, not taken from real speech.

F0 baseline for comparison ([../testing/flow-baseline-f0.md](../testing/flow-baseline-f0.md)): before any flow
code, none of 10 combined phrases produced a correct plan.

## 4.3 Live test protocol

- The real NEXUS build (`node nexus.mjs build --profile personal`), started with `NEXUS_QUIET=1` (no sound, no
  volume change) and a WebView2 debug port (9333, because 9222 was taken by another app).
- Sentences sent through the same command the voice path uses (`orchestrator_process`), answers through the same
  command the confirmation card and the spoken yes/no use (`orchestrator_mcp_confirm`), stop through
  `orchestrator_cancel` (what barge-in uses).
- A **fake WhatsApp bridge** on 127.0.0.1:8765 answers `pairing_status` = ready and records every `send_message`
  to a file: the proof of what was really sent.
- A temporary `contacts.json` with "Flowtest Dummy" and two "Ravi" contacts, removed afterwards.
- `flowsLive` and `flowsCloudChooser` switched on only for the test, switched off afterwards.
- Real Gmail (your connected account) and your own Groq key for the cloud chooser.
- Scripts kept in the session scratchpad: `flow_test2.mjs`, `stop_test.mjs`, `mock_wa.py`.

Two setup problems found and handled during testing:
1. Another NEXUS (a contributor build from `D:\ULTRON-which`) was running and held the shared startup lock, so the
   test copy started as a secondary instance, opened the first-run setup window and never ran flows. It was stopped
   with your permission (it was relaunched once more and stopped again).
2. An earlier round (PR #73) found a sidebar freeze caused by `DEV_KEEP_WINDOWS_ALIVE`; fixed there.

## 4.4 Live results (final build `c61940eeabb19714`, plus the footer fix noted below)

| # | Scenario | Result |
|---|---|---|
| T1 | "look at my inbox and drop a whatsapp to Flowtest Dummy saying Hi There" → yes | Read-back with name and number; **one** message `"Hi There"` to `919000000001`; "Sent, sir." |
| T2 | "check my mail and send a message to Flowtest Dummy on whatsapp" → follow-up → "saying I am Late" → yes | "What should the message say, sir?"; then one message `"I am Late"` (case kept) |
| T3 | Same, then "never mind" | "Okay, I won't do it, sir."; nothing sent |
| T4 | "peek at my inbox real quick and ping Flowtest Dummy on WhatsApp with Running Late" (beyond the local rules) → yes | Cloud chooser (Groq) picked `mail_then_whatsapp` with both details from the sentence; one message `"Running Late"` |
| T5 | "my inbox is full of whatsapp spam lately, any tips to stop it" | Chooser said none; normal cloud answer; no flow |
| T6 | "search and rescue team near me" | Not a flow; normal Maps search |
| T7 | "check my mail and take me to the place in that email" → no | First runs: no place in recent mail → honest "couldn't find a place". On the final run a real Maps link was found — **in a Vercel notification's footer** (company address). The read-back named the source and asked, so nothing opened; this led to the footer fix (§4.6) |
| T8 | Flow B, stop after 0.8 s and after 2.5 s | First build: the mail scan ran to the end and the result was spoken after the stop (**bug**). Fixed (cancel checked inside the scan and after each step; a stopped flow speaks nothing). Final build: both stops ended silently, log "Stopped … 1 of 3 steps" |
| T9 | Yes, then a second yes | One message; second yes ignored silently |
| T10 | Forged token (and old-style payload with a recipient) | Ignored; nothing sent |
| T11 | "… a test message to ravi …" with two Ravis | "I have more than one ravi in your contacts, sir: Ravi Kumar, Ravi Teja. Say the full name."; nothing sent |
| T12 | No | "Cancelled, sir."; nothing sent |
| T13 | Unknown contact "Zed" | "I don't have a number for Zed, sir. Say remember that wa phone Zed is the number, or import your contacts, then ask again." |
| — | Responsiveness | NEXUS stayed responsive through every scenario |

## 4.5 Full suites and gates

See the "Final checks" block appended at the end of this file.

## 4.6 Footer fix (found by T7)

`place_from_parts` now skips a Maps link or `Location:`/`Address:` line that sits within 600 bytes of footer
markers ("unsubscribe", "all rights reserved", "©", "privacy policy", "you are receiving", "manage preferences",
"notification settings", "view in browser"). Unit test: the footer link alone gives no place; an invitation link
far from the footer is still found; a `Location:` line at the top beats an address line next to "Unsubscribe".

## 4.7 Final checks (2026-11-22)

| Check | Result |
|---|---|
| Rust library, all tests (`cargo test --lib`, 4 threads) | 3397 passed, 1 failed, 37 ignored on the first run. The failure was `mcp_client::tests::test_connect_state_live_bridges`: it probes the real local bridge ports and found the fake WhatsApp bridge used for the live tests still running on 8765. After the fake bridge was stopped it passed (rerun alone: 1 passed) |
| Flow and place unit tests | 32 passed |
| Frontend (`vitest run`, `tsc --noEmit`) | 116 files, 1230 tests passed; TypeScript clean |
| Worker (`vitest run`) | 11 files, 145 tests passed |
| Gates | repo hygiene, turn-end, IPC contract (272 commands) and dead-code ratchet (139 known, none new): all passed |
| Live scenarios | 13 on the real app with the fake bridge, see §4.4 |
| Not covered live | the footer fix (unit-tested only), a real WhatsApp bridge, Flow B opening Maps for a real place, Ghost mode |

Pitfall recorded: the live tests need the fake bridge, but `test_connect_state_live_bridges` needs the bridge port to be free. Stop the fake bridge before running the full Rust suite.
