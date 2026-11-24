# 1. Research

Date of the research: 2026-11-17 (Opus advisor pass, read-only), extended during the build up to 2026-11-22.
The full source list with URLs is in [../features/193-multi-flow-command-workflows-plan.md](../features/193-multi-flow-command-workflows-plan.md) §9.
Items there are marked (v) when checked by web search on the day, (k) when taken from prior knowledge.

## 1.1 The question

You asked for multi-step commands across the Command Hub's centers, added one flow at a time, where NEXUS
"knows which flow to use when and where perfectly without any issue". Two examples set the bar:

- **Flow A** — "check the email and send a test message to X in WhatsApp": two independent actions.
- **Flow B** — "check the email and open Maps of the location link in the email": step 2 needs *data* produced by step 1.

Three hard problems hide in that request:

1. **Choosing**: the same words can fit several flows, most sentences fit none, and a wrong choice can send a message.
2. **Passing data** between steps ("the location in the email") without letting the data take control.
3. **Safety**: mail text is written by strangers. A flow that reads mail and then sends a WhatsApp message has all
   three legs of what Simon Willison calls the "lethal trifecta" (private data, untrusted content, outbound channel).

## 1.2 What NEXUS did before (code reading)

The path of a sentence was:

```
orchestrator::process_transcript
  ├─ wake strip, filters, directed gate, dictation, pending offers
  ├─ long chain of ParsedIntent → run_* interceptors
  ├─ command_center::build_plan_with_brain          (compound "X then Y")
  │     split_compound → parse_deterministic per part → CompoundPolicy must be Allowed
  └─ single-intent routing (deterministic → NLU BERT-Mini → admin brain → Worker / 9Router)
```

Findings (file references are to the code before this feature):

| # | Finding | Effect on your flows |
|---|---|---|
| D1 | `STEP_SEPARATORS` in `command_center.rs` deliberately leave out a plain " and " ("search and rescue") | "check the email **and** send ..." was never seen as two steps |
| D2 | Only 16 intents may be part of a compound (`center.rs`); mail, WhatsApp read, directions, calendar, briefing are standalone-only | Both flows refused even if split |
| D3 | `build_plan` gives up if any part fails `parse_deterministic` | "open Maps to the location link in the email" has no rule, so the plan died |
| D4 | `StepResult` carries only spoken text | No way to say "the place from step 1" |
| D5 | Only 4 executors can run inside a compound; most centers are one-off `run_*` functions | Centers had to be exposed as typed capabilities |
| D6 | A second confirmation inside a resumed compound ends the task | Multi-gate flows silently stop |
| D9 | The mail explainer strips HTML **including `href` targets** before anything reads it | Flow B's Maps link (usually inside `<a href>`) was thrown away |
| D10 | A trust label already exists: `memcore::store::Trust { UserSaid, UserOwned, Derived, Untrusted }` | Reused as the taint label |
| D11 | Risk classes already exist: `action_kernel::Risk { Read, Reversible, Destructive, Irreversible, Forbidden }` | Reused to decide where confirmation is needed |
| D12 | The WhatsApp MCP send is already confirmation-gated; read-receipt tools are refused | Kept |
| D13 | Saved routines run as string rewrites ("a then b then c") | Future: routines become flows (F4) |

**Measured baseline (F0, 2026-11-17).** An offline probe ran the deterministic parser over 18 phrases
([../testing/flow-baseline-f0.md](../testing/flow-baseline-f0.md)). None of the 10 combined phrases produced a
correct plan. Three were worse than a failure: the sentence was accepted as a *single* command and the second half
was silently dropped ("add the deadline to my calendar" became a plain calendar add; "order what I ordered last time
and tell Ravi" became an order only; "summarise the PR and message the team" became a PR analysis only). And
"search and rescue training near me" showed that a naive "and" split would break normal sentences.

## 1.3 How other assistants and papers do it

| System / paper | How it works | Lesson we took | What we did not copy |
|---|---|---|---|
| **Rasa CALM** | An LLM sees the conversation plus *flow descriptions* and may only start existing flows; the flows run deterministically | The core model: **the model may choose, fixed code executes** | Running an LLM on every turn |
| **CaMeL** (arXiv 2503.18813) | Control flow comes only from the trusted user query; every value carries provenance; policy is checked at each tool call | The taint design: mail text can fill a slot, never choose a flow, add a step or reach a recipient | Its Python-subset interpreter (our flows are fixed data) |
| **Design patterns for securing LLM agents** (arXiv 2506.08837) | Action-selector and plan-then-execute patterns; parameters can still be poisoned | The cloud chooser is an action-selector (an id from a closed list); sink rules close the poisoned-parameter gap | Assuming plan-then-execute alone is safe |
| **Dual-LLM pattern** (Willison 2023) | A quarantined model handles untrusted text and returns only references | Extraction from mail returns typed values only, and today it is plain parsing, not a model | — |
| **Lethal trifecta** (Willison 2025) | Private data + untrusted content + outbound channel = exfiltration | No mail-derived value may reach a recipient; bodies only with a verbatim read-back | Injection "detectors" as the main defence |
| **OVOS / Mycroft** pipeline | Ordered tiers; a *converse* stage lets an active skill claim the next utterance first | Follow-up answers are checked before anything else on the next turn | Keyword-only matching for open speech |
| **Home Assistant Assist** | Local sentence templates first, LLM fallback | Local first, cloud only when local cannot place it | Silent fallthrough; every layer decision is logged here |
| **Alexa dialog model** | Missing required slots are asked one at a time, then confirmed once | One follow-up question per missing detail, then one confirmation | Fixed routines without data passing |
| **Apple Shortcuts / App Intents** | Actions with typed outputs that later actions reference | Explicit, validated bindings (`$mail.ref`) | Implicit "magic variables" |
| **LangGraph interrupts** | Pause on a checkpoint, resume with a value | Confirmation = pause; the paused state stays on our side (later: one-time token in Rust) | The Python runtime |
| **ReWOO / LLMCompiler** | Plan with placeholders, then execute | Placeholder bindings between steps | Letting a model write plans at run time for actions with side effects |
| **aurelio semantic-router** | Example utterances per route, embedding similarity | Each flow carries example phrases (also our test corpus) | A cloud embedding API (see 02-plan §2.6 for why L2 embeddings were deferred) |
| **Agent Workflow Memory** | Reusable workflows induced from successful runs | Future: routines proposed as flows, never auto-enabled | Auto-enabling |
| **OWASP LLM Top 10 (2025)** | LLM06 Excessive Agency | Each flow declares exactly which capabilities it may call | — |
| **Turn-taking research** (Stivers et al. 2009) | Human turn gaps are about 200 ms | Read steps speak first; slow work happens after the acknowledgement | — |

## 1.4 The independent review (2026-11-21)

After F2, a second Opus pass reviewed the code read-only. It found two blockers and about a dozen smaller issues.
All ranked items were fixed before the first PR (#73). The blockers:

1. **Confirmation could be forged or replayed.** The whole paused flow travelled to the frontend and back, and its
   issue time was inside the payload. A second "yes" (card click plus spoken yes) would send twice.
   → Fixed by keeping the paused flow in Rust under a one-time token.
2. **"Stop" did not stop a running flow.** The cancel flag was dropped. → Fixed with cancel checks between steps
   (and, in F3, also inside the slow mail scan, after the live test caught a gap).

Other findings and how they ended: casing was lost (fixed), "send ravi a message" gave the recipient "ravi a
message" (fixed), the recipient was raw speech (now resolved from the contacts book with the number read back),
an uncertain speaker could start a flow (now refused), the Maps rule let `/maps/../url` through (fixed), the check
before asking only caught a down bridge, not an unpaired one (fixed), `flowsLive` was not in the Hub (fixed), the
"and" kill switch was missing (added), Flow B used only one account and only the most urgent mail (now scans the
newest mail of every connected account). Two scanner false positives found in the same review (manifest-only PWA,
bare "auth0") were also fixed.
