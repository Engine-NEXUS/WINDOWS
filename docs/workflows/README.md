# Multi-step workflows ("flows")

One spoken sentence that combines several NEXUS centers, run safely as one task.

- "check the email and send a test message to Ravi on WhatsApp" (two independent steps)
- "check the email and open Maps to the location in it" (step 2 uses data from step 1)

This folder is the single place for everything about the feature. Read in order:

| # | File | What it answers |
|---|---|---|
| 1 | [01-research.md](01-research.md) | What NEXUS did before, what other assistants and papers do, and what we took from each |
| 2 | [02-plan.md](02-plan.md) | The architecture, the decisions you made, the phased roadmap and how each phase was scoped |
| 3 | [03-implementation.md](03-implementation.md) | What was built, file by file: router, executor, safety rules, follow-up questions, cloud chooser, settings, logs |
| 4 | [04-testing.md](04-testing.md) | Every test: unit tests, phrase corpora with before/after scores, the live protocol and its results, how to rerun them |
| 5 | [05-gaps-and-next.md](05-gaps-and-next.md) | What is not verified, what is not built, known limits, and the next phases |

Older records kept where other docs link to them:

- Original research plan (phase F0–F5 roadmap, sources): [../features/193-multi-flow-command-workflows-plan.md](../features/193-multi-flow-command-workflows-plan.md)
- F0 baseline table (what the parser did before any change): [../testing/flow-baseline-f0.md](../testing/flow-baseline-f0.md)

## Status at a glance (2026-11-22)

| Area | State |
|---|---|
| Flows shipped | `mail_then_whatsapp`, `mail_then_maps` |
| Switch | Command Hub, Desktop control: "Multi-step workflows" (`flowsLive`, default **off**). Off = shadow mode, only `[FLOW]` log lines |
| Other switches | "Treat 'and' as a step break" (`flowsAndSplit`, default on), "Understand reworded workflow requests (cloud)" (`flowsCloudChooser`, default off) |
| Unit tests | 31 in `flows::` and `memcore::explain::place_tests` |
| Live tests | Real NEXUS (quiet mode) + fake WhatsApp bridge: 11 scenarios, results in [04-testing.md](04-testing.md) |
| Not verified live | A real WhatsApp bridge send, Flow B with a mail that really contains a place, Ghost mode |

## Code map

| Path | Role |
|---|---|
| `src-tauri/src/flows/mod.rs` | Flow format and validation, router, follow-up and cloud-chooser pure logic, safety rules, executor |
| `src-tauri/src/flows/live.rs` | Runtime: entry points, one-time confirmation tokens, follow-up questions, real adapters |
| `src-tauri/src/flows/builtin/*.json` | The shipped flows |
| `src-tauri/src/orchestrator.rs` | Three hooks: follow-up answer (early), flow routing (before compound planning), confirmation hand-off |
| `src-tauri/src/memcore/explain.rs` | Place / Maps link extraction from a mail (links read before markup is stripped) |
| `src-tauri/tests/fixtures/flows/` | Phrase corpora |
| `frontend/src/settings-sidebar/hub/featureRegistry.ts` | The three Hub switches |
