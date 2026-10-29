# NEXUS notification-to-sidebar: research and feasibility

Date: **2026-10-28**. Status: **research/proposal only; not implemented or device-tested**.

The owner requested a NEXUS notification for a failed PR/repository/build, important Gmail message or similar event, followed by a click that opens smoothly from the top into a correctly sized sidebar containing the relevant analysis.

## Documents

- [Detailed implementation plan](../../features/notification-to-sidebar-implementation-plan.md).
- [Reusable implementation prompt](../../features/notification-to-sidebar-implementation-prompt.md).
- Existing context: [notification brain plan](../../features/151-notification-brain-event-key-cooldown-and-short-notifications-plan.md), [notification research/testing](../../testing/notification-brain-testing-and-research.md), [broader Focus/GitHub programme](../../features/153-focus-mode-github-resolution-and-notification-programme.md).

This proposal does not change the current production queue or declare another feature ready. Existing Voice Lock acceptance gates remain separate. The owner continues to control all NEXUS builds, starts, stops, restarts and runtime settings manually.

## 1. Product interpretation

The intended interaction is:

```text
An event happens
    ↓
A compact NEXUS alert appears near the top
    ↓ user clicks
The alert expands/unfolds toward the sidebar
    ↓
The exact event's context appears immediately
    ↓
Current status and deeper analysis load without losing that context
```

Example: “Checks failed in zync · PR #34” opens the exact account/repository/PR/check run, rather than a generic feed or whichever PR happens to be newest. An important-mail alert opens the exact connected account/thread. A recovered build retains its original failure context and shows the updated status.

The recommendation is a **NEXUS-owned top-right card with a coordinated handoff into the existing sidebar**, plus a separately engineered optional native-notification fallback. The first version should preserve a common top/right origin; cross-screen diagonal motion and elaborate notch styling are later experiments.

## 2. Platform research

### Windows

Microsoft's supported notification flow passes activation arguments back to the app. A body click can display relevant content; an action can be handled separately. The app must handle activation correctly for an already-running process and the registered launch path. Native toast content uses defined structures and shell-controlled attribution.

This supports “system notification click → exact NEXUS view.” It does not provide a shared animation surface between the system toast and a NEXUS WebView. The continuous expansion must be implemented in NEXUS-owned UI.

Sources: [Microsoft activation quickstart](https://learn.microsoft.com/windows/apps/develop/notifications/app-notifications/app-notifications-quickstart), [notification content](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content), [Windows notifications and Do Not Disturb](https://support.microsoft.com/en-us/windows/experience/notifications-and-do-not-disturb-in-windows).

### macOS / MacBook

Apple's notification guidance expects a notification interaction to lead to related content and encourages concise, useful, non-repetitive notifications with privacy-conscious previews. User Notifications provides response handling. AppKit exposes safe screen regions relevant to displays with camera cutouts.

An app-owned card/overlay can deliver a consistent NEXUS visual interaction. This proposal does not assume a general Mac Dynamic Island API or attempt to modify Apple's Notification Center animation. A notch-inspired appearance is a visual option, not a cross-platform system capability.

Sources: [Apple notification guidelines](https://developer.apple.com/design/human-interface-guidelines/notifications/), [notification response handling](https://developer.apple.com/library/archive/documentation/NetworkingInternet/Conceptual/RemoteNotificationsPG/SchedulingandHandlingLocalNotifications.html), [safe-area API](https://developer.apple.com/documentation/appkit/nsscreen/safeareainsets), [auxiliary top screen region](https://developer.apple.com/documentation/appkit/nsscreen/auxiliarytopleftarea-uglc?language=_1).

### Linux laptops

The freedesktop notification protocol advertises capabilities. An implementation can omit/ignore actions, and presentation is controlled by the desktop notification server. Therefore Linux needs capability checks and a useful fallback rather than a blanket promise that every desktop behaves identically.

A native overlay's placement, activation and stacking also require target-desktop/window-system verification. Cross-platform Tauri does not itself prove identical behavior under every compositor.

Source: [Desktop Notifications Specification](https://specifications.freedesktop.org/notification/latest-single/).

### Tauri-specific findings

Tauri documents native notifications on desktop, with a Windows installed-app qualification. Its documented notification **Actions API is mobile-only**. A desktop plan cannot assume the shown `onAction` example solves Windows/macOS notification click routing. Prove a suitable desktop adapter/activation implementation against the project's actual dependencies and packaging.

Tauri also documents physical monitor position, scale factor and work area. Work areas exclude taskbars/docks; physical coordinates must be converted consistently before logical layout.

Sources: [Tauri notification plugin](https://v2.tauri.app/plugin/notification/), [Tauri window/monitor API](https://v2.tauri.app/reference/javascript/api/namespacewindow/).

These are sources consulted for the research, not a claim that NEXUS already includes their integrations or that every API is present in the pinned dependency version.

## 3. Feasibility comparison

| Concept | Assessment | Engineering qualification |
|---|---|---|
| Native notification opens relevant NEXUS sidebar | Feasible | Platform activation, installed package and running/cold paths must be implemented/tested |
| Native OS toast itself morphs into a NEXUS window | Not the supported native handoff model | Use an app-owned card/visual surface for the effect |
| App-owned top card unfolds into sidebar | Feasible | State, geometry, hit regions, focus and renderer handoff need explicit design |
| One consistent appearance on Windows/macOS | Feasible at the NEXUS UI layer | Native focus, stacking, permissions and safe regions remain platform-specific |
| Every laptop brand needs unique UI code | Usually unnecessary | OS, work area, DPI, display layout and cutouts matter more than manufacturer |
| Exact identical behavior on every Linux desktop | Unproven | Capability/compositor-specific tests and fallbacks required |
| Alerts detected while the whole NEXUS process is stopped | Not supplied by current in-process polling alone | Would require a separate authorized background/push architecture; outside version1 |
| Reading/taking over every other app's OS notifications | Separate product scope | Not needed for GitHub/Gmail events already available through connected services |

No numeric feasibility percentage, performance result or delivery schedule was measured.

## 4. Source investigation and effort already performed

This work consisted of read-only source inspection and official-documentation research. It did not create a prototype, run UI interactions, install a notification plugin, authenticate to a provider, fetch private mail/PR data or test a live notification.

| Source area inspected | What was found | Implication |
|---|---|---|
| [notify_brain.rs](../../../src-tauri/src/notify_brain.rs) | Failed/still-failing/fixed entity state; speech event submission; generic project/kind/run reference | Reuse state semantics, but add typed exact navigation context |
| [notify.rs](../../../src-tauri/src/notify.rs) | Stable event identity/cooldown support | Preserve deduplication; do not create a second conflicting policy |
| [proactive_policy.rs](../../../src-tauri/src/proactive_policy.rs) | Speech/card decisions, busy/meeting/focus and rate limits | Delivery policy must coordinate visual, speech and native channels |
| [github_watch.rs](../../../src-tauri/src/memcore/github_watch.rs) | Provider observations feed the notification brain | Preserve full repository/account/run identity before reducing it to display wording |
| [deploy_watch.rs](../../../src-tauri/src/memcore/deploy_watch.rs) | Deployment observations use the shared state machine | Extend typed targets for provider/project/deployment identity |
| [Sentinel store](../../../frontend/src/store/sentinel.ts) | Tracking-first incoming/docked/collapsed/dismissed lifecycle | Existing tracking does not establish a completed interactive presentation layer |
| [Google Sentinel](../../../src-tauri/src/google/sentinel.rs) | Thread-oriented alert context and landing metadata | Useful mail context; needs consistent cross-source action/view contract |
| [commands.rs](../../../src-tauri/src/commands.rs) | Unified sidebar preparation/show/view switching and pending-content storage | Reuse one sidebar; protect the cold-mount/warm-window race paths |
| [UnifiedSidebar.tsx](../../../frontend/src/sidebar/UnifiedSidebar.tsx) | Shared panel host | Notification opening must cooperate with existing views |
| [PR list state](../../../frontend/src/pr-list/prListStore.ts) | Existing PR context/pinning behavior | Arrival or late view events must not overwrite active work |
| [Cargo manifest](../../../src-tauri/Cargo.toml) / deep-link registration | Existing deep-link integration is present for other flows | Presence of OAuth/deep-link support does not prove notification activation support |

The inspected sidebar uses view-specific logical widths, including 400 and 520, and screen-size-based placement. The inspected path computes zero-based positions/clamps from monitor size. It needs explicit monitor-origin and work-area review before claiming correct placement on secondary/negative-coordinate monitors or around taskbars.

The notification brain's generic fact is not yet a durable GitHub/Gmail/deployment navigation contract. A display project name is insufficient when two accounts/owners have a repository of the same name.

## 5. Architecture conclusion

Use one canonical alert/event record, then make speech, compact cards, activity history and native fallback consumers of it. A click resolves an opaque event ID to a validated local target, loads the corresponding sidebar context and coordinates the transition. It must not parse the visible sentence or execute an arbitrary URL/command supplied by remote content.

Prefer reusing the existing stage/sidebar surfaces. A new permanently resident WebView for each alert or integration would add resource and lifecycle cost. Whether the stage can safely own the compact card must be proven with its hit-region and visibility behavior.

A real single-window morph requires the compact and expanded states to share a native window/renderer. If the card and sidebar are separate WebViews, their DOM elements cannot share a CSS layout animation; use an explicit matched visual handoff with a ready acknowledgement and timeout fallback. Neither approach is declared tested here.

## 6. Important unknowns

- Which surface should own the compact card without interfering with the orb, captions or pointer behavior?
- Does the pinned Tauri/native stack provide the required desktop click activation paths, or is an adapter required?
- How should an alert open while a PR list or analysis is already pinned? Proposed: preserve it and provide a reversible event-detail route.
- Which monitor should own passive arrival, and what happens after docking/monitor removal? Proposed policy needs owner agreement and hardware tests.
- Can the existing provider observations preserve full target identity without changing deduplication/recovery semantics?
- What is the actual CPU/GPU/RAM/frame-time impact of each animation/window strategy?
- How are OS quiet modes and locked/fullscreen contexts observed reliably on each platform?
- How should unavailable/deleted/private targets appear without opening unrelated fallback content?

## 7. Recommended delivery order and effort assessment

Relative effort is an engineering assessment, not recorded hours or a promised deadline.

| Work package | Relative effort | Why |
|---|---|---|
| Canonical event and typed target contract | Medium | Identity migration, persistence and provider adapters |
| Correct click-to-context routing | Medium–high | Accounts, stale events, cold/warm mounting, pinned state and cancellation |
| Compact card and dismissal/accessibility | Medium | Stage integration, hit regions and non-focus-stealing behavior |
| Coordinated expansion and geometry | High | Native/WebView boundary, DPI, monitor origin, safe regions and frame stability |
| Native desktop fallback | High | Platform activation/packaging and lifecycle-specific behavior |
| Cross-platform device proof | High | Real macOS/Windows/Linux conditions cannot be settled by shared React tests |

Start with a correct Windows click-to-context flow and quiet compact card. Add motion after correctness. Add native fallback after desktop activation is proven. macOS/Linux adapters follow without claiming Windows evidence establishes parity.

The detailed phase contracts and tests are in the linked implementation plan. This research creates no runtime change and grants no release acceptance.
