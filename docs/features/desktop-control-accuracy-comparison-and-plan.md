# Desktop-control accuracy: NEXUS and Hitodama

Follow-up evidence: the [installer-inspection investigation](../research/hitodama-installer-inspection-feasibility-2026-11-06.md) accessed the official requirements page. It confirms a stated Windows-accessibility dependency for browser control and acknowledges unsupported controls. Download requires account access; no binary has been inspected.

Date: 2026-11-06. Research/design only. No application source, runtime, settings or build changed.

## Evidence boundary

The owner reports Hitodama desktop controls are very accurate. That is useful user-observed evidence for their experience; the exact product version, tasks, attempts and failures have not been recorded here. Hitodama's [product page](https://hitodama.in/) describes voice-driven desktop actions. Its [privacy policy](https://hitodama.in/legal/privacy/) says screenshots can be sent when a requested click cannot be located by name. This supports a documented name/visual fallback behavior, not a claim about its exact locator implementation, private code or measured success rate.

NEXUS's current source can be inspected. Its shipped accuracy cannot be inferred from code or unit-test counts. No same-device comparison has been run, so neither a numerical accuracy rating nor a claim of universal superiority is justified.

## Comparison focused on desktop control

| Area | Hitodama evidence | NEXUS current source | Gap / improvement |
|---|---|---|---|
| Perceived desktop accuracy | Owner reports strong results | No matched real-task benchmark | Record the same tasks and all attempts before assigning a winner |
| Voice to command | Public voice-control product | Separate STT, normalization, parsing, authorization and dispatch paths | Measure each stage; a transcription error is not a mouse error |
| Named controls | Policy describes locating by name, without implementation details | `ghost_center::native::act` requires one exact case-insensitive accessible name | Resolve role, app, container, label aliases and stable identity; ask when still ambiguous |
| Missing accessible controls | Policy describes screenshot use for a name lookup miss | Current Ghost centre explicitly has no vision/OCR path; `vision.rs` exists separately | Coverage gap in this route; any visual fallback must be a separately gated proposal, not an automatic restoration of old unsafe clicks |
| Keyboard actions | No route-level evidence inspected | `shortcuts/router.rs` checks foreground identity, protection, elevation and stop before sending | Preserve it; add app-specific expected results and measure coverage |
| Native semantic actions | Not publicly established | Toggle, select and invoke patterns already used in Ghost centre | Add per-action postconditions rather than replacing this foundation |
| Result verification | Public implementation details/benchmarks unavailable | Toggle/select can be checked; generic Invoke deliberately returns Unconfirmed | Observe expected dialogs, selected tabs, values or window state within a deadline |
| Wrong-window protection | Public route details unavailable | Foreground checks, runtime-ID hit tests, protected/elevated-window guards exist | Test real focus switches, moved controls, occlusion and stale elements |
| Web-app automation | Requirements page states Windows-accessibility dependency and unavailable controls; DOM automation not established | Browser commands and desktop accessibility paths exist; this audit did not establish a general semantic DOM controller | Shared accessibility foundation; consider an explicitly connected browser adapter using stable semantic locators |
| Multi-step work | Advertised task/routine surface, no measured benchmark | Multiple native routes and a strict Ghost grammar exist | Carry verified state between steps; abort or clarify on uncertainty, not blind replay |
| Latency | No comparable measured data | PhaseTimer exists in the mouse route | Capture per-stage median/p95 and failure reasons before choosing an optimization |

The defensible conclusion is that Hitodama's documented visual fallback and the owner's reported experience identify areas worth studying. It is not established that NEXUS lacks accessibility automation, safety, keyboard control or every form of vision: those components already exist. Their route integration, supported coverage and live validation are the issue.

## Current source anchors

- [Ghost centre](../../src-tauri/src/ghost_center.rs): exact-name/eligible-control filtering, ambiguous refusal, native patterns, outcome receipts and no visual path.
- [Mouse route](../../src-tauri/src/live/commands/mouse.rs): legacy entry now delegates to Ghost centre; point hit-testing and focus/cancellation guards; phase timing.
- [Shortcut router](../../src-tauri/src/shortcuts/router.rs): app/window gates, bindings and optional effect verification.
- [Vision module](../../src-tauri/src/vision.rs): separate providers and screenshot grounding helpers; its existence does not establish use by the current Ghost centre.
- [File action kernel](../../src-tauri/src/action_kernel/mod.rs): typed file operations with policy/confirmation/verification; this is not a universal desktop-control executor.

## Why “use a better model” is insufficient

A spoken action can fail during capture/transcription, intent interpretation, app targeting, element resolution, dispatch or effect verification. Replacing the model does not fix a changed foreground window, an invisible control or an unobserved result. Likewise, finding a plausible pixel does not prove the requested task completed.

The proposed contract is:

```text
Understand request → identify app/window → resolve one target
→ validate current permissions and state → perform one action
→ observe expected effect → continue, clarify or stop
```

Each stage retains evidence and a bounded deadline. Refusal, uncertain outcome and success are separate results. A permissive coordinate fallback must not inflate apparent success by clicking more often.

## Implementation proposal

### D0 — Measure existing behavior

Define a supported Windows task set: open/focus an app, switch a tab, type into a field, scroll a named pane, select a setting, open a menu and perform a bounded multi-step workflow. Use safe fixture apps/pages where possible. Record both supplied text and real voice to isolate speech errors.

Run the same task descriptions, initial window states and display settings on both assistants if the owner provides the competitor installation and authorizes that trial. Include all failures, refusals, retries and wrong actions; reset between attempts. Do not infer accuracy from a highlight reel or only successful requests.

### D1 — Improve semantic resolution

Extend the existing target descriptor with app/window identity, control role, container, label aliases and automation identity. Keep exact matching first. Multiple plausible controls trigger clarification or a numbered selection, not a guess. Re-resolve immediately before use; do not reuse a stale element/coordinate across a navigation.

Microsoft explains that accessibility clients can inspect/invoke exposed controls and that custom controls require appropriate providers: [UI Automation providers](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-providersoverview). This external dependency prevents a universal UIA-only guarantee.

### D2 — Verify effects per skill

Add reusable effect probes for common actions: foreground app matches, new tab exists, expected dialog appears, toggle state changes, text value matches or scroll position changes. Wait for observable change with a bounded deadline. Microsoft's [Invoke contract](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-implementinginvoke) describes asynchronous dispatch; returning from Invoke is not completion proof.

On timeout, re-observe before deciding whether retry is safe. Never repeat a state-changing action merely because its acknowledgement was lost. In particular, sending, purchasing, deleting and submitting remain behind their existing restrictions/confirmation requirements.

### D3 — Expand app coverage deliberately

Prefer supported app APIs and known keyboard/native patterns. For web controls, evaluate an opt-in browser connection with semantic locators and visibility/stability checks. [Playwright actionability](https://playwright.dev/docs/actionability) is a useful design reference, not evidence that Hitodama uses Playwright or that browser automation works on native desktop apps.

For inaccessible visual controls, first expose the gap honestly. A later explicit visual-grounding mode may propose a target in a fresh cropped screenshot. It needs protected-content checks, monitor/scale transforms, unique-target evidence, revalidation and low-risk boundaries. Do not silently bypass the current accessibility-only Ghost design or rely on an uncalibrated model confidence number as a safety guarantee.

### D4 — Recover and learn with evidence

Use app-specific skills for repeated workflows. Save stable app/role/container/automation descriptors and user-approved aliases, not absolute click coordinates. Invalidate learned targets when layout/app identity changes. Stop cleanly on focus loss, user takeover, cancellation, missing capability or ambiguous targets.

### D5 — Qualification

Proposed initial matrix: 30 task variants repeated ten times, distributed across at least six representative Windows apps/pages; add a separate held-out layout/task set. Cover 100–200% scaling, dual monitors, delayed dialogs, duplicate labels, moving/covered controls, permission denial and interruption. Keep whole-task success, target precision, supported coverage, speech fidelity, wrong-target actions and latency separate.

A proposed release objective is at least 99% observed completion on the declared supported task set, zero observed unauthorized/wrong-target actions and zero actions after acknowledged cancellation. Report sample sizes, uncertainty and unconfirmed/refused attempts; these are proposed targets, not present performance or proof of perfection. Preserve the programme's separate weighted readiness score and live-evidence floors.

Repair failures with regressions, mutation-check critical rules, and repeat focused tests twice on unchanged source. Run full native/integration gates and owner-operated fresh-release/device trials before promotion. The existing production queue remains authoritative; these packages are desktop-control work cards, not permission to skip active features awaiting proof.

## Definition of a credible “very accurate” assistant

Very reliable behavior is achievable for a declared set of supported tasks and apps. Perfect unrestricted control of every desktop interface is not a defensible promise. Unknown or dangerous situations should stop or ask, while supported tasks complete through observed state changes. The release claim should state what was tested, how often it succeeded, and what remains unsupported.
