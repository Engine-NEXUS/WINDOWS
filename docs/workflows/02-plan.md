# 2. Plan

## 2.1 Architecture in one picture

```
spoken sentence
  │
  ├─ C0  follow-up answer?   a flow asked "What should the message say?" → this reply fills it   (try_fill_slot)
  │
  ├─ L1  local router        split into clauses (then / after that / ; / ", also" / "? " and a verb-gated "and"),
  │                          classify each clause (mail_check, whatsapp_send, maps_from_mail),
  │                          match the sequence of kinds to an enabled flow's signature
  │
  ├─ L3  cloud chooser       only if L1 found nothing AND the sentence touches two workflow areas AND the switch is on:
  │                          your Groq/Gemini key picks one flow id or "none"; every slot it returns must be in your words
  │
  ├─ (none)                  → the normal NEXUS pipeline continues untouched
  │
  └─ start the flow
        owner check → ask for a missing detail → resolve the contact → run steps
        read steps run first and speak; before the first step with a side effect: ONE read-back + confirmation
        the paused flow waits in Rust under a one-time token; "yes" resumes, "no" discards, 10 minutes max
```

## 2.2 Decisions you made (2026-11-17, recommended defaults accepted)

| # | Question | Decision | Where it lives in code |
|---|---|---|---|
| 1 | Split on a plain "and"? | Yes, only when both halves start with a command verb; kill switch | `split_clause_pairs`, setting `flowsAndSplit` |
| 2 | May a recipient come from mail text? | No. Tightened after review: the recipient must also be a **saved contact** | `user_only`, `resolve_contact` |
| 3 | One confirmation per flow or per action? | One before the first side effect; payments would get their own (none exist yet) | `FlowRun::next_action`, `approve` |
| 4 | Flows as data files or Rust? | Data files over Rust capabilities | `flows/builtin/*.json`, `CAPS` |
| 5 | Cloud chooser for other users? | Off by default; you can switch it on | `flowsCloudChooser` |
| 6 | Turn routines into flows automatically? | Propose only, you approve | F4, not built |
| 7 | Which flows first? | Flow A and Flow B | `builtin/` |
| 8 | Guess or ask when unsure? | Ask | follow-up questions; the cloud chooser drops anything you did not say |

## 2.3 Data model

- **Capability** (Rust, `CAPS` in `flows/mod.rs`): an id, a risk class, typed inputs with `accepts_untrusted`, and
  outputs with a trust label.

  | Capability | Risk | Inputs (accepts untrusted?) | Outputs (trust) |
  |---|---|---|---|
  | `mail.check` | Read | — | `summary` (Untrusted), `ref` (Derived) |
  | `mail.extract_place` | Read | `mail` (no) | `place` (Untrusted) |
  | `whatsapp.send` | Irreversible | `recipient` (no, user-only), `number` (no, user-only), `body` (yes) | — |
  | `maps.open_route` | Reversible | `destination` (yes, allowlist) | — |

- **Flow** (JSON): `id`, `description`, `enabled`, `signature` (clause kinds in order), `examples`, `slots`
  (required or not), `steps` (capability + input bindings `$slot.x` / `$step.output`).
- **Value**: text + trust + origin ("you", "an email from Acme").

Validation at load (`validate`): known capabilities, unique step ids, every input bound, bindings only to declared
slots or *earlier* outputs, no untrusted output into an input that refuses it, user-only inputs bound to slots only,
non-empty description and examples. A broken file is skipped with a warning, never fatal.

## 2.4 Safety rules (enforced twice: when a flow file loads, and again right before each step runs)

1. Control comes only from your words and the shipped flow files. Mail text never chooses a flow or adds a step.
2. A message recipient and number come only from your words, and the number from your contacts book
   (`contacts.json` or a saved `wa_phone_<name>` fact). Ambiguous or unknown = a spoken reason, nothing sent.
3. Mail text may become a Maps destination only as short plain text (≤120 chars, no markup) or an https link on
   `maps.google.com`, `maps.app.goo.gl`, or `www.google.com/maps…`. Lookalike hosts, credentials in the URL, odd
   ports, `..` / `%2e` / backslash path tricks are refused.
4. One confirmation before the first side effect, read back with where each value came from. The paused flow stays
   in Rust; the frontend gets only a random 128-bit token. A token works once and for 10 minutes.
5. A stop cancels the flow between steps and inside the mail scan; nothing is spoken over a stop.
6. Only owner-verified or unenrolled turns start a flow; uncertain or rejected voices do not.
7. The cloud chooser sees only the sentence and the flow descriptions, never mail text. Any contact or message it
   returns is dropped unless it appears verbatim in what you said.

## 2.5 Phases as delivered

| Phase | Planned | Delivered |
|---|---|---|
| F0 | Measure today's behaviour; corpus | Done 2026-11-17: probe + 18-phrase corpus + baseline doc |
| F1 | Registry + both flows end to end | Done: registry, validation, L1 router, executor, shadow mode |
| F1b | Real adapters + confirmation | Done: mail check, place from mail, WhatsApp send, Maps open; confirmation through the existing confirm path |
| F2 | Data passing + taint, expiry | Done: trust labels, sink rules, tighter Maps rule, 10-minute expiry, attack tests |
| Review fixes | Advisor's ranked list | Done: one-time token held in Rust, cancel checks, casing, recipient parsing, contacts, owner gate, multi-account Flow B, Hub switches |
| F3 | L2 embeddings, L3 cloud chooser, L4 clarify, C0 converse | Done differently, see §2.6: wider L1 rules + paraphrase corpora, C0 follow-up questions, L3 cloud chooser with grounding. L2 and a general L4 not built |
| F4 | Learning from routines + Workflows page | Not started |
| F5 | Hardening, adversarial v2, CI ratchet | Not started |

## 2.6 Why F3 changed shape

The original plan put a BERT-Mini embedding layer (L2) between the rules and the cloud. While building F3 that
choice was revisited:

- The shipped BERT-Mini sidecar is a **classifier**, not a sentence encoder. Using it for similarity needs a new
  `/embed` endpoint on the Python sidecar, which is lazy (a cold start takes seconds) and adds RAM under the
  project's ceiling.
- The baseline showed the real gaps were **vocabulary** ("drop a whatsapp", "ping", "look at my inbox", "take me
  to the place in that email", "it mentions") and **connectors** (", also", "? then"). Those are cheap and exact
  to fix in the rules, and every fix is covered by a corpus line.
- For phrasings beyond the rules, a closed-list cloud chooser is more accurate than nearest-neighbour similarity on
  4–5 examples per flow, and it only runs on sentences that touch two workflow areas.

Result: the development corpus went from 16/30 to 30/30 with the rules alone (04-testing §2), and the cloud
chooser handled a phrasing the rules do not ("peek at my inbox real quick and ping X on WhatsApp with ...") in the
live test. L2 can still be added later if the corpus shows misses the rules cannot cover.

## 2.7 Constraints kept

Free and BYOK only (Groq/Gemini keys are yours, optional); no new resident model or sidecar; idle RAM unchanged;
everything new is wired and covered (dead-code, IPC and turn-end gates pass); settings live in `NexusSettings` so a
Hub save never drops them; you control builds and runs.
