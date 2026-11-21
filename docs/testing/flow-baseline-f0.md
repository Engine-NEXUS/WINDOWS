# Multi-flow baseline (F0) — what today's parser does

Plan: `docs/features/193-multi-flow-command-workflows-plan.md`. Probe: `src-tauri/tests/flow_probe.rs`, corpus `src-tauri/tests/fixtures/flows/corpus_v0.jsonl` (18 phrases, written by the planner, not real STT text, owner-biased). Measured 2026-11-17 with `cargo test --test flow_probe -- --nocapture`. Offline, parser only: `parse_deterministic` on the whole sentence and on both halves split at " and ". Nothing executed. The NLU model, the admin brain and the cloud fallback were NOT consulted, so "None" means "the deterministic parser has no rule", not "NEXUS fails".

| id | phrase (short) | whole sentence | left half | right half | reading |
|---|---|---|---|---|---|
| A1/A2 | check the email and send a test message to ravi on whatsapp | None | None | SendWhatsAppMessage | "check the email" has no deterministic rule, so the plan cannot form |
| B1/B2 | check the email and open maps to the location link in the email | None | None | OpenApp | right half is read as "open an app" (wrong target), no link is bound |
| C1 | check the email and add the deadline to my calendar | **CalendarAdd** | None | CalendarAdd | whole sentence is read as a plain calendar add and the email step is silently ignored |
| D1 | directions to the address in the last email | Directions | – | – | would search the literal words "the address in the last email" |
| E1 | read my whatsapp messages and reply to ravi | None | WhatsappRead | None | reply half has no rule |
| F1 | order what i ordered last time and tell ravi on whatsapp | OrderFood | OrderFood | NeedMoreInfo | whole sentence is read as an order and the tell-ravi step is ignored |
| G1 | summarise the latest pr in zync and message the team | AnalyseLatestPr | AnalyseLatestPr | WhatsappChat | whole sentence wins and the message step is ignored |
| H1 | open my study setup and start the timer | OpenApp | OpenApp | OpenApp | only works because both halves are open-app lookalikes |
| S1 | check the email | None | – | – | single mail command is not deterministic at all |
| S2–S4 | single commands, then-compound | SendWhatsAppMessage / OpenApp / OpenApp | – | – | fine |
| N1 | search and rescue training near me | **PlacesSearch** | BrowserSearchFocus | PlacesSearch | negative: a plain split on "and" would break this; the verb gate is mandatory |
| N2–N4 | everyday sentences with "and" | None | None | None | safe today |

## Findings

1. **None of the 10 combined phrases produces a correct plan today**, because `split_compound` does not split on "and" (`command_center.rs`) and "check the email" has no deterministic rule. Counting what the parser alone yields: 0 of 10 correct, 4 of 10 silently act on only ONE half (C1, F1, G1 whole-sentence wins; D1 is single-step but takes the email reference literally).
2. **The silent half-execution is the worst outcome** (C1, F1, G1): the sentence is accepted as a single command and the second request is dropped without any spoken notice. F2/F3 must add a "did the sentence contain more than one request?" check before a single-intent win.
3. **The verb gate is required for splitting on "and"**: N1 shows "search and rescue" splits into two valid-looking intents.
4. **Mail is the missing deterministic anchor.** Whether NLU/brain/cloud catches "check the email" is not measured here; F3 needs that measured with the live router (needs the NLU sidecar), not this probe.

## Capability inventory (callable function with typed input/output?)

From the plan's diagnosis D5 and `center.rs` (read, not re-verified line by line): the centers that can run as a plan step today are the 4 subsystems in `execute_step` (local command, MCP, Worker backend, GitHub). Mail digest, WhatsApp read, directions, places, calendar add, briefing and screen-email watch are `StandaloneOnly` and run as bespoke `run_*` functions in `orchestrator.rs`; none returns structured output. Turning each into a capability with a typed result is F1/F2 work.

## Not measured

Real STT text, other speakers, NLU/brain/cloud routing, any live Gmail/WhatsApp/Maps behaviour, latency. Corpus is 18 phrases, far too small for accuracy numbers; no accuracy claim is made.
