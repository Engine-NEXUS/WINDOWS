# NEXUS connected assistant improvement plan

Date: 2026-11-06. Status: research-backed proposal only. No feature implementation or runtime change is authorized by this document itself.

Context: [Hitodama research and current-source comparison](../research/hitodama-nexus-comparison-2026-11-06.md). The intended outcome is a coherent NEXUS experience across voice, selected-screen questions, dictation, routines, reminders and notifications.

## 1. Programme boundaries

Preserve Windows 1.0, free/BYOK access, the 600–700 MB local-memory goal, personal/contributor isolation and current action/Voice Lock policies. The owner controls application builds, launches, stops, settings and device trials. Offline design and tests do not establish live readiness.

The existing weakest-first production queue remains authoritative. These phases describe dependencies and future work cards, not permission to skip the active feature or its live gate. Existing features use their current scorecard entries; genuinely new scope receives an evidence-based baseline before queue insertion. Do not inflate an old row by adding untested scope.

User-specified completion threshold remains raw score at least 90% (4.5/5), with B≥90, T≥70, L≥70, S≥80 and I≥60, plus mutation-tested safety and exact-release/device proof. The older Feature 149 text describing 80% must not override the later user instruction.

## 2. Intended user journeys

| Journey | Expected NEXUS flow | Completion evidence |
|---|---|---|
| Explain something on screen | Select → ask → see source-bound answer in sidebar | Correct selected area; no unrelated pixels sent |
| Type into another app | Hold/press dictation → speak → release/stop → insert or preview | Text reaches the intended field; no automatic submission |
| Run a routine | Voice or click → same routine → visible step results | Actual outcomes, with partial failure and cancellation visible |
| Schedule a routine/search | Review parsed time/action → save → see next occurrence | One durable occurrence result; honest missed/offline status |
| Follow an alert | Compact alert → exact event/task detail | No stale click result, wrong account or duplicate completion |
| Correct memory | View fact/source → edit/delete/share choice | All relevant local views update; cloud use obeys the choice |

Do not combine these into one new always-listening executor. Each capability retains its own execution and privacy boundaries.

## 3. Shared contracts, reusing existing authorities

### Interaction context

Propose an ephemeral context envelope containing interaction ID, entry point, typed task/event references, app/window identity, optional screen selection, capture timestamp, display scaling and permission state. Screen and microphone consent are separate. A content snapshot is evidence, not authority.

Voice, hotkeys and clicks may supply context through this envelope, but a trusted UI gesture must never manufacture a verified-owner voice result. Cancellation invalidates outstanding capture/model replies. Context expires on timeout or relevant focus change.

### Typed task requests

Use validated requests such as ExplainSelection, DictateText, OpenApp, RunRoutine, ScheduleSearch and ShowEvent. They adapt to existing intent routing and `action_kernel` gates. Do not store a raw transcript and later execute it as a command.

Each request declares required capabilities, whether it changes state, the affected app/account and its confirmation policy. Remote content and retrieved calendar/email text remain untrusted data. An explanation cannot implicitly become a click, file operation or message send.

### One owner per kind of state

Reuse the existing notification ledger and opening tokens for alerts. Reuse the agent lifecycle for provider task completion. Adapt the existing background-task chip registry for long operations, retaining its bounded lifetime. Build mappings between their identifiers instead of adding another completion publisher or duplicating the same durable job in three stores.

A user-written to-do is different from a running job. Give it an explicit model only if the new scope is approved, and link it to jobs/events when useful. A “possibly ready” result must not appear as verified success.

### UI and settings

The Hub remains detailed configuration. A compact quick panel exposes current state and frequent actions; it does not maintain independent settings. The sidebar remains the detailed result surface. A notification click keeps the existing generation token so late result A cannot overwrite newer result B.

“Synced” means those views read authoritative state and reconcile after reconnect. It does not mean automatic cloud backup or copying the owner's personal profile into contributor builds.

## 4. Work packages and acceptance

### P0 — Close reliability blockers and freeze a baseline

Repair the three [agent-watch audit gaps](../changes/2026-11-06-agent-watch-cross-check.md): older busy siblings, definitive process-exit evidence and permission-resume activity. Retain existing Voice Lock/device gates and other active feature work cards. Inventory notification-to-sidebar code already present.

Acceptance: regressions fail before repair; removed safety rules fail tests; focused checks pass twice on unchanged source. Full native/release/device requirements remain owner-operated. No new feature can disguise a failed existing journey.

### P1 — Define consistent state and visible capability status

Specify the envelopes/adapters above, existing-store ownership and versioned UI payloads. Give every visible capability an honest state: available, setup required, unsupported platform, temporarily unavailable or disabled. Add operation IDs to diagnostic events without logging raw audio, screenshots, tokens or private content.

Acceptance: stale results and duplicate terminal observations cannot overwrite current UI or create another completion; a failed service yields one actionable reason. UI reconnect reads current records. Contributor and personal identifiers/data never cross.

### P2 — Direct dictation through existing audio and insertion code

Add a configurable dedicated shortcut and a small non-activating recording indicator. Use existing capture arbitration, decoding, cleanup, dictionary, cancellation and insertion safety. Resolve shortcut conflicts; do not blindly copy Ctrl+Win/Ctrl+Shift bindings. For hold mode, release modifier keys before injecting text.

Bind insertion and undo to a stable target window/control identity. After cloud latency, revalidate the target; if focus changed, keep the transcript in a preview rather than paste into the new window. Keep deterministic cleanup as the default. Optional rewriting is explicit and previewable.

Acceptance: a declared 50-case matrix across Notepad, browser fields, an editor and chat apps; every wrong-target, protected-field, elevated-app and cancellation case must insert zero characters. Include names, URLs, numbers, code, spoken corrections, Unicode, focus switches and clipboard restoration where used. No auto-Enter, send or command execution from dictated text. Report transcription fidelity separately from insertion success. Owner judges actual speech and text quality.

### P3 — Select an area and ask

Start with a rectangular selection using existing screen capture/context and sidebar analysis. A circle can be optional drawing feedback later. Specify exactly which pixels leave the device: a rectangular crop is not a pixel mask around a lasso. If mask behavior is promised, pixels outside it must be excluded before upload.

Store monitor identity and physical/CSS coordinate transforms with the selection. Show a visible cancel path. Recheck protected content and context immediately before capture. Never silently fall back to uploading the entire monitor when a selection fails. Expire the snapshot after navigation/focus change; a follow-up action requires fresh grounding and normal authorization.

Acceptance: 100/125/150/175/200% scaling; mixed-DPI dual monitors and negative monitor coordinates; edge selections, cancelled selections, protected windows and navigation during inference. Assert the actual uploaded crop/mask, not just the drawn outline. Zero capture/upload on cancellation/protected-content cases. Windows device walkthrough required.

### P4 — Editable routines and durable schedules

Extend current named routines rather than creating an unrestricted macro interpreter. Begin with the existing safe app/site/music allowlist, plus an explicitly reviewed search action where added. Support create, preview, edit, run and delete through voice and Hub using the same stored definition. Show step-level partial failure and stop at cancellation.

Create a typed schedule record with ID, revision, timezone, recurrence, task/routine reference, permission policy and missed-run policy. Keep durable occurrence IDs and record outcomes so restarts and duplicate ticks cannot repeat a due action. Define DST gaps/folds, clock changes, sleep/resume, offline provider failure and revision/cancellation races.

Saving a schedule is explicit authorization only for its declared bounded actions. Recheck current policy at execution; revoked permissions win. Do not autonomously send messages, purchase, delete, run arbitrary shell code or perform power actions through the new scheduling surface. Connected calendars can supply reminders, never executable instructions.

Acceptance: restart immediately before/after claim, repeated ticks, two schedule revisions, timezone change, sleep through a due time, cancellation while queued, missing app and expired provider access. Start with one occurrence of a scheduled search and one safe morning routine. Say “runs while NEXUS is running and the device is awake”; do not promise execution while powered off or override manual sleep. Future timers do not hold the laptop awake for hours.

### P5 — Connected quick panel and notification-to-sidebar polish

Reuse the orb/stage/Hub, media and notification code. Expose microphone state, current operation, media controls, routines and recent alerts in a small panel. Expand into the existing sidebar when detail is required. Exact event/task/account references drive navigation, not notification wording or the latest generic result.

Reuse `notify_open` transactions and expansion geometry helpers; qualify their current behavior first. Native OS notifications require an activation handoff, not a promised continuous animation of the OS-owned toast. NEXUS-owned cards can animate directly. Preserve pinned views, keyboard recovery, reduced motion, high contrast and correct input hitboxes.

Acceptance: rapid clicks A→B, cold sidebar, duplicate activation, stale/deleted event, failed account connection, focus preservation, multi-monitor geometry and display changes. No click alone merges a PR, sends mail or triggers another state-changing action.

### P6 — Memory control and optional snippets

Keep editable memory but display origin, last correction and whether a fact may be sent to a provider. Separate explicit facts from suggestions. Deletion must invalidate relevant caches. Confirm consistency across the profile, Memory page, routine suggestions and injected context; do not claim all keys or settings are already confined to the OS keychain without auditing existing direct readers.

If snippets are approved, start with text-only entries invoked deliberately inside dictation. Templates are data, not executable commands. Secret placeholders require deliberate selection; no automatic credential or clipboard harvesting. A general clipboard archive is deferred.

Acceptance: sensitive fact exclusion, stale cached fact deletion, correction without duplicating the old value, per-feature disablement, export preview and contributor/personal isolation. Test snippet text resembling commands as literal text, including “delete files” and shell syntax.

### Later proposal — Selective backup and cross-device sync

Begin with versioned encrypted local export/restore and explicit scope preview. Keep credentials, Voice Lock recordings/profiles, encryption keys, logs and contributor data outside default exports. Cloud sync requires its own threat model, key recovery, conflict/tombstone rules, key rotation and user consent; encryption-at-rest is not sufficient to claim end-to-end secrecy. Never let syncing schedules authorize execution on an additional device automatically.

## 5. Testing and rollout rules

For each approved feature, create one work card with the observed gap, test matrix and measurable claim. Add a failing regression, make the narrow change, mutation-check authorization/cancellation/privacy/deduplication rules, then run focused checks twice against a recorded unchanged source snapshot. Two green passes alone do not replace the full native library and integration gates.

Before any production-ready claim, record fresh release provenance and live Windows evidence: voice, ears, focus, real apps, notification navigation and power behavior where relevant. Record failures as well as passes. An unavailable capability, unknown task result or rejected action must remain visible and truthful.

Measure end-to-end success, wrong-target actions, duplicate completion deliveries, cancellation latency, request-to-result latency and resource use separately. Compare latency percentiles against the same-device baseline before setting promises. Measure total NEXUS-owned process-tree memory cold and warm, including sidecars, during dictation, screen analysis, alerts and concurrent work; retain the existing RAM goal rather than judging only the main process.

Do not assign speculative completion percentages, stars or competitor superiority scores. Update scorecard data only when the programme's evidence supports it.

## 6. Recommended next action

First repair and qualify the three reproduced agent-watch gaps within the active work programme. Then use the current scorecard queue to select the next feature; attach the relevant package above to its work card. Direct dictation and selected-screen analysis are strong product candidates because they reuse existing code, but they do not jump ahead of an active feature awaiting required live proof.

This plan adds no requirement to switch STT/TTS providers, introduce a subscription, use a larger local model, copy Hitodama's branding/assets, or contact its private services. Its purpose is to make NEXUS's existing capabilities operate coherently and prove their results.
