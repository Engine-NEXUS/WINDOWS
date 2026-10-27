# Command-center alignment (2026-10-19)

## Current ownership

`orchestrator::process_transcript` owns the turn: origin, owner/evidence policy,
validation, request ID, cancellation, acknowledgement, result, and completion.
`center::action_spec` is the typed per-intent contract: center identity, existing
execution route, risk, existing confirmation gate, and compound eligibility.
`center::prepare_action` applies the same owner/evidence and slot checks to a
single intent and to each step that `command_center` executes.

```text
captured turn / text input
  -> deterministic parser (then existing fallback)
  -> prepare_action -> ActionSpec
  -> single intent: orchestrator dispatch -> existing executor
  -> compound: build_plan -> ordered execute_step -> existing executor
  -> orchestrator:event -> frontend speech arbiter -> speech_sync playback timeline
```

The 17 `CenterId` values are action families. `Subsystem` remains an execution
transport (`LocalCommand`, `WorkerBackend`, `Mcp`, `GitHub`, `Architect`) during
the adapter migration. A center is not a second owner of the turn, audio,
caption, or orb. `CenterId::as_str` supplies the legacy display string;
`CenterId` is the policy identity.

BrowserCenter supplies local browser validation to the shared gate. The
KnowledgeCenter route delegates to the existing Worker/9Router dispatcher,
and MessageCenter delegates to the existing MCP dispatcher and its write
confirmation flow. Other action families use explicit legacy dispatch paths;
their names do not imply that their execution has moved to a standalone module.

## Safety and ordering

- The owner/evidence check runs before validation; rejected or ambient Ghost
  turns never execute. Unsupported NLU labels are refused by an exact allowlist.
- A command with `then` must produce an approved plan for every step. If it
  cannot, NEXUS asks for separate commands; it does not execute a partial
  parse. File, power, close-app, GitHub and other standalone-only actions
  cannot enter a plan. GitHub remains standalone until its confirmation
  resume path can continue a compound without dropping later steps.
- Every compound step repeats policy checks before execution, including after
  confirmation resume. The existing GitHub/MCP/file confirmation handlers
  remain the actual gates and are not duplicated by `ActionSpec`.
- A `depends_on` edge prevents parallel execution. Only an explicitly
  dependency-free search step is currently eligible for a future read batch;
  the present `then` planner links each step to its predecessor.
- Existing `orchestrator:event` request IDs and the speech playback timeline
  remain the turn and audiovisual completion authority. No new audio engine or
  network hop is introduced by the alignment.

## Migration boundary

The historical five-bucket proposal is a research design, not the present
runtime topology. New actions must add exhaustive `ParsedIntent` routing,
center identity, action risk, slot validation, compound eligibility, and an
executor/confirmation path. After a center's execution is moved, its legacy
orchestrator arm can be removed with route and turn-end regression tests.
The larger feature should target this contract rather than adding another
orchestrator or UI authority.
