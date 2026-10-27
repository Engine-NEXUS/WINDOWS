# Feature Plan: 120 — Subcommand Connection Contract (Main Center never breaks)

> **Current implementation note (2026-10-19):** The shared `ActionSpec` now
> carries typed center, route, risk, confirmation-delegation and compound
> eligibility. `prepare_action` checks single and compound steps. The four
> historical `SubCenter::confirm_kind` implementations remain advisory; the
> established executor confirmation flows are still the actual gates. See
> [command-center alignment](../architecture/12-command-center-alignment.md).
> The older H2b discussion below describes the 2026-10-14 baseline; the
> current side-effect policy now defaults newly added actions to gated and
> retains the explicit app/site-opening exception.

## Current add-a-command contract

1. Add the `ParsedIntent` variant, parser cases, and speech examples. Assign
   it in the exhaustive `center_id_for` match. The display name comes from
   `CenterId::as_str`; there is no `center_for` runtime registry now.
2. Review its `ActionSpec` route, risk, confirmation delegation, and compound
   eligibility. Unlisted actions are standalone-only; unknown NLU labels are
   refused until added to `supported_nlu_label` and a real dispatch arm.
3. Add required-slot validation to `center::validate` or the relevant center
   validator. Preserve owner/evidence checks through `prepare_action` for both
   single commands and every compound step.
4. Implement or adapt the executor behind the existing orchestrator route.
   A write that needs approval must use its one existing GitHub, MCP, file,
   or other executor gate; `SubCenter::confirm_kind` is advisory only.
5. Pin the center, route, risk, confirmation and compound behavior in tests.
   Verify request IDs, cancellation, one turn closure, and speech playback
   through the normal `orchestrator:event` path.

The analysis and checklist below document the 2026-10-14 baseline. Their
`center_for`, fail-open side-effect, and partial-compound claims are historical.

**Date**: 2026-10-14
**Status at the time**: H1–H3 built (record:
`docs/changes/119-plan-120-connection-contract-h1-h3.md`).
**Grounding**: two analysis passes over `center.rs` (815 lines), `orchestrator.rs`
`process_transcript` (~828–960) + `route_intent` (575–690), `command_center.rs`.

## 1. What the deeper pass corrected / found (missed items)

1. **Gate order is disposition → validate**, not validate → disposition. Actual:
   ownership-reject → early-ghost-stop → `action_disposition` → `center::validate`
   (skipped in dictation/ghostwriter/drill) → `route_intent` → `run_*` → turn-end.
2. **`command_evidence` fails safe** (`_` arm defaults by source strength) — per-intent
   arms only needed for slot-aware evidence (repo-token checks). Not a gap.
3. **Two-layer validation confusion (real gap).** `center::validate` ends `_ => Ok`,
   and per-center `SubCenter::validate` impls are UNREACHABLE from the main pipeline
   except three NLU prefixes (`youtube_*`, `system_*`/`focus_app`/…, `browser_*` —
   `center.rs:441-449`). A new deterministic subcommand gets **zero slot checks**
   unless arms are added to `center::validate` directly.
4. **`is_side_effect` fails open (safety gap).** Unlisted variants return false, so a
   new state-changing intent defaults to non-side-effect → `Allow` for unenrolled
   voices. The compiler cannot catch this; only a checklist/test can.
5. **`confirm_kind` is dead in production** (4 impls, callers = unit tests only).
   Implementing it on a new center gives false confidence — confirmations must be
   wired in the orchestrator/MCP confirm flow.
6. **`is_long_running` is `#[allow(dead_code)]`** — the loading decision lives
   elsewhere; do not branch new centers on it.
7. Cosmetic: `source_strength` has an unreachable duplicate `deterministic-fuzzy`
   branch. No in-Rust `should_ghost_route` — in-session ghost routing is frontend-side.

## 2. What already protects the Center (verified, not assumed)

- `center_for` and `route_intent` are exhaustive with explicit `Unknown` final arms —
  no wildcards. A new `ParsedIntent` variant without routing arms is a **compile error**.
- Compound fallback is conservative (`command_center.rs:366`): unwired steps degrade
  to the single-intent path, never worse.
- CI ratchets: dead-code audit, IPC contract, turn-end grep gate, `external-api.txt`,
  `test_center_for_routing`.

## 3. The contract (12 steps to add a subcommand)

1. `ParsedIntent` variant + deterministic parser (+ phonetic aliases); order it against
   swallow-patterns; extend the 49/50 EN fixture + e2e rows.
2. `center_for` arm (compiler-forced) + assert in `test_center_for_routing`.
3. `center::validate` arms for every required slot (no per-center dispatch exists
   for deterministic intents — §1.3).
4. `is_side_effect` listing if it changes external state (fail-open otherwise — §1.4).
5. `command_evidence` arm only if slot-aware evidence matters (default is safe).
6. `route_intent` arm (compiler-forced) + `run_*` with turn-end completion + cancel flag.
7. Register any new parse `source` in `source_strength` (unlisted = 0 = ambient drop).
8. Frontend audibility: directed-gate STARTERS/DOMAIN_CUES + in-session ghost routing.
9. NLU labels only if the NLU path is needed (`train.py` ordering + retrain).
10. `NexusSettings` entry for any new knob (else wiped on save).
11. IPC registration if frontend-invoked (contract gate).
12. Boot-audit entry if it is a new center (`ALL_SUB_CENTERS` + diagnostics Layer 4).

## 4. Hardening to build with the next subcommand (small, one session)

- **H1 — spelling test**: every `center_for` output ∈ `ALL_SUB_CENTERS` (coverage is
  compiler-checked; spelling is not).
- **H2 — side-effect audit test**: every variant reaching a state-changing runner
  must appear in `is_side_effect` (turns §1.4 from convention into CI).
- **H3 — dead-policy cleanup**: either call `confirm_kind` in the confirm flow or
  delete it; either wire `is_long_running` or delete it; drop the dead
  `source_strength` branch. Ends the false-confidence APIs.

## 5. Verify

- `cargo test center::` + new H1/H2 tests; full lib serial; vitest + tsc if frontend
  touched; all Q0/Q1 gates; live voice pass of the new command in Normal + Ghost.
- Non-goal: no NLU retrain in this plan; no behavior change to existing centers.
