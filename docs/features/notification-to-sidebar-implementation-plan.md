# NEXUS notification-to-sidebar implementation plan

Date: **2026-10-28**. Status: **proposed; source implementation not started by this documentation task**.

[Research and source investigation](../research/notification-to-sidebar/README.md) · [Reusable coding prompt](notification-to-sidebar-implementation-prompt.md)

## 1. Goal and boundaries

Present a concise NEXUS alert for a meaningful provider event. After an explicit click or keyboard activation, unfold/open the exact related context in the existing sidebar with correct size, placement and current status.

Windows is the first implementation target. macOS/Linux behavior is researched but remains a separate adaptation/verification phase. This plan does not replace the active Voice Lock work card, authorize queue advancement or assign a production-readiness score. Scheduling implementation requires the owner's instruction.

The owner controls NEXUS builds, starts, stops, restarts and runtime settings manually. Source tests and isolated mocks may be prepared; do not invoke a production application build, provider mutation or live test without the applicable authorization. A blocked real-device gate remains open.

Version1 exclusions:

- Taking over the entire operating system's notification center or reading arbitrary other-app notifications.
- Automatic PR merge, deployment rerun, email reply/send, archive or mark-read on notification arrival/click.
- New background daemon or promise of detecting new events while all NEXUS processes are stopped.
- A permanent WebView per provider/alert or unrelated voice/authorization refactor.
- A claim that a native OS notification can literally share a DOM transition with the NEXUS sidebar.

## 2. Proposed interaction contract

### Arrival

- A short top-right card uses a NEXUS identity, provider icon, concise fact and age/status.
- Arrival does not take keyboard focus, move the user's cursor or replace current sidebar content.
- Low/batched events can stay in activity history. Meeting/quiet/fullscreen/lock policies decide visibility and speech.
- Default preview avoids sensitive email subjects/snippets where privacy settings/context require redaction.
- A burst of related failures updates/groups one card. Recovery updates that event/entity instead of creating misleading duplicate failures.

### Activation

- Click, Enter or Space resolves the event ID to a validated target.
- Show known local context immediately; fetch/refine status without waiting for a model before presenting the shell.
- The user sees the exact PR/check run/thread/deployment. A deleted/unavailable target shows an honest error and safe provider-opening option, not an unrelated newest item.
- If the event recovered, retain its original context and display the recovered status/time.
- A pinned/active sidebar view is preserved and recoverable. Passive events cannot replace it.
- Escape closes/collapses the notification detail consistently and restores the appropriate previous view/focus.

### Motion

- Initial proposal: a top-right card unfolds down from the same top/right anchor into the sidebar.
- Illustrative design ranges: compact width 340–420 logical units, height 58–80; expanded preferred width 400–520. These are prototype hypotheses, not measured universal sizes.
- A roughly 220–320ms transition is a design starting point, not a performance guarantee.
- Reduced-motion mode uses a direct reveal/minimal fade. Keyboard activation and screen-reader state must work independently of animation.
- Content must not squash/stretch as a screenshot during expansion; reveal/reflow using measured layout.

## 3. Event, entity and target identity

Keep related concepts separate:

| Concept | Responsibility |
|---|---|
| Entity key | Long-lived thing whose failed/fixed state is tracked |
| Occurrence/event ID | A particular failure, message or recovery occurrence |
| Target | Exact authorized account/provider object to display |
| Delivery state | Whether card/speech/native/history delivery happened |
| Read/dismissed state | User's handling of the NEXUS alert; not automatically the provider's unread state |

Proposed canonical record fields:

| Field | Requirement |
|---|---|
| `schema_version`, `event_id`, `entity_key` | Versioned, bounded, stable identifiers |
| `provider`, `account_id`, `kind` | Provider and opaque locally bound account identity |
| `occurred_at`, `observed_at`, `updated_at` | Distinguish event time, detection delay and latest status |
| `status`, `severity` | Typed values; failure/recovery and urgency are distinct |
| `title`, `preview`, `privacy_class` | Safe bounded display fields; remote text is untrusted content |
| `target` | Tagged union with exact routing fields, not a command string |
| `source_ref`, `source_revision` | Trace provider occurrence; GitHub commit SHA where applicable |
| `delivery`, `read_state`, `expires_at` | Explicit lifecycle; no silent provider mutations |

Target variants should include:

- GitHub: account binding, provider host, full owner/repository, PR number if present, workflow/check/run ID and relevant commit.
- Gmail: account binding, thread ID and message ID when needed. Store sensitive content only under existing privacy/storage policy.
- Deployment: provider, account/team/project binding, deployment ID and source revision when available.
- Repository health: full owner/repository and an explicit observation scope; do not guess a PR when the alert was repository-wide.

Human-readable project names are display metadata. The resolver must not parse the notification sentence to identify a repository or use a hidden default repository.

Review migration from existing generic entity keys. New full-identity keys must not replay a backlog of old failures or lose an existing failing/recovered state. Preserve quiet first-observation baseline and original event deduplication semantics.

## 4. State machines and renderer handoff

Provider/entity state and presentation state are separate. Proposed presentation states:

```text
queued → visible → opening → detail_open
           ↓          ↓          ↓
       dismissed   open_error  collapsed/closed
```

Requirements:

- One active opening transaction carries an event ID and generation/token.
- A later explicit user activation supersedes an earlier opening request. Late fetches/ready events cannot replace the selected context.
- Warm-window and cold-mount paths receive the same versioned payload, with acknowledgement and idempotency.
- Store pending content before show/view-switch events; clear/consume only the matching transaction.
- Wait for the target content shell to be ready before removing the transition/card cover. Use a bounded timeout and stable fallback, never an indefinite invisible/opening state.
- Dismissal, Escape, app shutdown or navigation cancels presentation work and releases any temporary hit region/focus assumptions.

Architecture spike must decide:

1. Existing stage hosts a compact card and hands off visually to the unified sidebar; or
2. The unified sidebar itself has compact/expanded states when it is idle, with another strategy when it is already occupied.

A true single-window morph requires a shared window/renderer. Separate WebViews require coordinated timing/geometry, not a claimed shared-element CSS animation. Reuse the existing sidebar and avoid creating a full desktop-sized interactive blocker.

## 5. Geometry and window behavior

Resolve geometry from the selected monitor's **work area**, origin, scale factor and safe regions. Convert physical/logical units once at a documented boundary.

Proposed invariants:

- `panel_width ≤ available_width − horizontal_margins`.
- `panel_height ≤ available_height − vertical_margins`.
- Absolute position includes the monitor/work-area origin, including negative coordinates.
- Minimum comfortable width/height is conditional; a minimum must never force the panel off a smaller display.
- Long text and large accessibility fonts scroll/reflow inside the panel.
- Top origin avoids menu bars/camera-cutout areas; right/bottom edges avoid reserved areas.
- Recompute after monitor removal, dock/taskbar change, display rotation and scale changes.
- Passive arrival uses a defined monitor policy. Proposed default: the monitor where the user is active, with a selectable fixed-monitor option; do not silently teleport an already open detail.

Test the actual project's monitor API/version. A current online Tauri API property is not proof that the pinned dependency exposes it.

Use transforms/opacity for renderer-local animation where suitable. Avoid per-frame IPC/native `set_size` loops until measured: they can trigger reflow, flicker, shadow/rounding issues and stale hit regions. If native bounds change, define the small number of resize/position steps and the exact interaction rectangle at every stage.

## 6. Delivery and interruption policy

Retain one policy owner for speech/card/native/history delivery. One provider occurrence must not produce multiple independent spoken lines or duplicate banners through different channels.

| Context | Proposed behavior |
|---|---|
| Normal desktop, meaningful new failure | Compact card; speech only if existing policy permits |
| User typing or speaking | No focus steal; no overlapping speech |
| Meeting, screen sharing or quiet mode | Redacted/held/silent delivery according to explicit policy |
| Fullscreen or locked session | Conservative behavior; no sensitive overlay over protected contexts |
| Existing sidebar active/pinned | Keep its state; offer a reversible event-detail navigation after click |
| Same failure repeated | Update/group rather than spam |
| Recovery before click | Show recovered state with original failure context |
| NEXUS running in background | Custom/native choice follows availability and user preference |
| NEXUS fully stopped | Current in-process watchers cannot detect new events; no version1 promise otherwise |

A custom overlay does not automatically inherit OS notification permission/Do Not Disturb behavior. Implement platform observation/consent policy and conservative fallbacks. Native fallback must not become a second simultaneous delivery of the same alert.

## 7. Read-only context and analysis

Notification activation opens information. It must not authorize a provider mutation or confer owner/admin privileges.

- Show an evidence-backed status shell first, including provider/source time and exact object identity.
- Preserve the event's run/message/deployment reference while refreshing current status.
- Show loading, offline, expired authorization, deleted/private object and rate-limit states honestly.
- Separate factual failing-step/log evidence from generated analysis and hypotheses.
- Do not invoke a model for every repeated passive event. Fetch/enrich only under the agreed policy, preserving the project's free/BYOK/resource constraints.
- Treat GitHub titles, email content, logs and provider URLs as untrusted. They cannot supply shell commands, arbitrary IPC arguments or instructions to the agent.
- Allowlisted provider links derive from validated typed targets, not arbitrary notification-body HTML/URLs.
- A mail preview does not implicitly mark a thread read, archive it or send a response. Make any provider-state change deliberate and separately authorized.
- If an event expires or the account disconnects, show an unavailable state; do not silently resolve to another account.

## 8. Phased execution and review gates

| Phase | Editing deliverable | Required tests / cross-check | Completion limit |
|---|---|---|---|
| N0 source spike | Confirm actual payloads, panel owner, current geometry and dependency capabilities; record narrow file list | Read-only trace of one GitHub and one Gmail event; no private service fetch needed | Architecture recommendation only |
| N1 event contract | Canonical typed event/target, full identity, resolver, persistence/migration design; mocked producers | Same-name repos/accounts cannot collide; duplicate/recovery/baseline cases; invalid/stale/missing target; schema/IPC contract | No new visuals or live provider action |
| N2 click-to-context | Route mocked card activation to exact existing sidebar view; cold/warm pending handoff; preserve prior/pinned view | First-open race, rapid A→B clicks, late A data, dismiss/cancel, no provider mutation, read-only error views | Correct behavior without fancy animation |
| N3 compact card | Accessible, quiet top card, queue/grouping, dismissal/history and privacy states | No focus steal; keyboard/screen reader; meeting/quiet policy; speech/native deduplication; hit regions | Existing windows reused where proven safe |
| N4 expansion/geometry | Selected renderer strategy, work-area-aware sizing and bounded ready handoff | DPI/negative origin/small screens, reduced motion, timeout, resize/dock changes; measured frame behavior | Hardware proof still required |
| N5 native fallback | Desktop activation adapters and installed-app routing | Already-running and cold activation, invalid arguments, dismissed/expired event, permission denial, no duplicate process/channel | Windows first; macOS/Linux separately verified |
| N6 acceptance | Frozen source, full checks, owner-built verified release, device evidence and second review | Real GitHub/Gmail events, provider outage/recovery, all relevant hardware/accessibility/privacy cases | No production claim until every gate passes |

Each phase follows: inspect → write measurable work card → add a failing meaningful regression → narrow edit → focused tests → safety mutations → relevant broader checks → second review → evidence/documentation update. Do not change unrelated behavior to get tests green.

## 9. Safety mutation targets

At minimum, regressions should fail when these safeguards are removed:

- Full owner/repo/account identity; no text-based target guessing or hidden repository fallback.
- Opening-generation check and cold/warm pending-payload correlation.
- Preservation of pinned/current sidebar state on passive arrival.
- Work-area origin/scale handling and off-screen clamping.
- No passive focus acquisition and no invisible input blocker.
- Privacy redaction and interruption suppression.
- No merge/reply/rerun/mark-read as an activation side effect.
- Deduplication/recovery baseline preservation across producer/channel/migration boundaries.
- Ready timeout/cancellation cleanup and stale-result refusal.
- Untrusted activation argument and provider-content validation.

Do not count a mutation killed by an unrelated syntax/import failure as safeguard proof.

## 10. Real-device acceptance matrix

| Dimension | Minimum cases |
|---|---|
| Windows display scaling |100/125/150/200%; primary and secondary display |
| Desktop layouts |Single display; mixed-DPI dual display; left/above monitor with negative origin; monitor disconnect |
| Reserved areas |Taskbar placements supported by the target OS; auto-hide; app/menu safe regions; Mac notch where applicable |
| Content |Short/long titles, long failure text, no preview permission, large font, keyboard-only and reduced motion |
| Lifecycle |Sidebar cold/warm/hidden/pinned; rapid clicks; close during fetch/animation; app relaunch activation |
| Provider state |Live failure/recovery, stale notification, disconnected/expired account, rate limit, offline/deleted/private target |
| Resource behavior |Memory before/after repeated opens, no WebView per alert, no continuing animation loop after dismissal, frame-time measurements |
| Safety |No unsolicited action/focus, wrong account/repo/thread, sensitive preview leak or click-blocking outside the visible card |

Proposed responsiveness goal: a local shell/selection acknowledgement appears promptly without waiting for remote analysis. Set numerical timing/frame/resource budgets after N0/N2 measurements; do not present an unmeasured 250ms/60fps promise as achieved.

## 11. Documentation and evidence deliverables

For each implemented phase retain the source revision/input fingerprint, files changed, red/green regression evidence, mutation results, test scope/ignored failures, and all remaining cases. Record successful and failed live checks against a fresh release in the existing ledger.

Assign an evidence-based score before entering this proposed interaction in the production queue. Existing notification-brain test counts do not prove a new notification UI. No phase inherits macOS/Linux device proof from Windows tests.

## 12. Definition of done

An alert opens the correct evidence-backed context, the motion/window/hit-region behavior is correct on the tested device matrix, existing work and user focus are preserved, duplicate/quiet/privacy policy is respected, and activation causes no unintended provider/action side effect. All exact-source shared gates, fresh provenance, real-device evidence, second review and applicable readiness-rubric minima must pass. Any wrong-target action, hang, leak or untested required platform gate leaves the relevant scope in progress.
