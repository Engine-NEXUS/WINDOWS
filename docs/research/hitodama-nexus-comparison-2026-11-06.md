# Hitodama research and NEXUS capability comparison

Research date: 2026-11-06. Scope: public product material and current NEXUS source inspection. No competitor application was installed or tested; no NEXUS runtime/build/settings changes were made.

## 1. Product identification and evidence quality

The supplied name was `hitodoma.in`. That address was inaccessible to the research tool. Search identified the likely intended product as **Hitodama**, at [hitodama.in](https://hitodama.in/). This identification is an inference from the matching name and Windows-assistant description, not a confirmed redirect.

Direct fetches of the likely official site returned HTTP 403. Its public pages were readable through search-indexed versions of those same official pages. Consequently this is a public-claims review, not a hands-on usability, accuracy, speed, RAM, security or reliability comparison. No private endpoints or unpublished architecture were investigated.

Primary sources:

- [Official product page](https://hitodama.in/): interface and advertised capabilities; indexed copy reported crawled two days before this research.
- [Official privacy policy](https://hitodama.in/legal/privacy/): stated data flows and controls; policy dated October 31, 2026.
- [Official refund policy](https://hitodama.in/legal/refunds/): describes a Windows product requiring microphone, internet and the user's Google AI Studio key. It refers to a requirements page, which was not independently examined.

Do not treat marketing multipliers or webpage demonstrations as measured performance. Regional billing wording and displayed prices were inconsistent across retrieved page sections; this report makes no price comparison or claim that third-party inference is always free.

## 2. What the public material describes

The product page advertises selected-screen questions, hold-to-talk dictation, to-dos, scheduled searches, timed routines, desktop voice actions, a compact island/control centre, and customizable appearance, personality and voice. These are vendor claims, not independently verified results. [Source: product page](https://hitodama.in/).

The policy describes Gemini-backed voice/context processing, editable memory, local encrypted credentials, calendar links, snippets, clipboard history and encrypted account backup. It distinguishes local storage from information sent to Google, and lists backup exclusions. The cloud-backup key is held by its service in encrypted form; that description alone does not establish operator-inaccessible end-to-end encryption. [Source: privacy policy](https://hitodama.in/legal/privacy/).

The useful design lesson for NEXUS is an inference: make everyday interactions discoverable and consistent. This does not establish that Hitodama implements them more reliably than NEXUS.

## 3. Mapping to current NEXUS source

“Present” below means inspected source exists, not that the current executable or real device has passed acceptance. “Not located” means this audit found no complete corresponding flow; it is not proof that no related code exists elsewhere.

| Capability group | NEXUS evidence | Assessment | Recommended direction |
|---|---|---|---|
| Ask about the screen | `screen_context.rs`, `screen_tour.rs`; orchestrator capture guards and screen-analysis routes | Existing context, crop and narrated-analysis foundations | Add explicit user-selected region entry point; preserve privacy/identity gates |
| System-wide dictation | `dictation.rs`, `dictation_text.rs`; native orchestrator calls both session and one-shot dictation | Existing typing, cleanup, dictionary and undo foundations | Add discoverable dedicated press/hold flow, target-bound insertion and truthful results |
| Desktop action control | `action_kernel/`, `command_executor.rs`, Ghost/orchestrator routes | Broad existing executor surface | Reuse the authorization/execution path; improve grounding and outcome verification |
| Reminders and timed actions | `memcore/scheduler.rs`, `timetable.rs`; `CreateVoiceReminder` parser intent | Real timetable/reminder code; not proof of a general scheduled-command engine | Define durable typed schedules, recurrence and missed-run behavior |
| Named routines | `memcore/routines.rs` and Memory page | Confirmed routine learning/storage path; safe command allowlist | Improve manual creation/editing and link a routine to a schedule without bypassing gates |
| Personal memory | `memcore/profile.rs`, Memory page, sealed store | Existing editable memory and sensitive-key filtering | Add visible provenance, cloud-sharing controls and deletion/export consistency |
| Compact controls | Stage/orb, Hub, media player, volume modules | Existing UI and controls spread across surfaces | Add a small quick panel backed by the same state; keep the Hub as detailed settings |
| Alert opens exact context | `notify_open.rs`, `notify_hub.rs`, `notify_activation.rs`, `notifyExpandModel.ts` | Transaction and animation helpers already exist in source | Qualify and finish current implementation before creating another notification system |
| User task list | `background_tasks.rs`, `memcore/agent_tasks.rs` | Machine-work registries exist; a general user-authored checklist is not established by them | Specify a separate user-task model, linked to jobs rather than pretending they are the same thing |
| Snippets | Dictation dictionary exists; a full user-editable template/snippet flow was not located | Partial adjacency | Start with opt-in text-only snippets through dictation, with no executable expansion |
| Clipboard history | Incidental clipboard read/paste paths exist; no complete general history manager established | New scope | Defer; passive capture introduces retention and sensitive-content work |
| Cross-device synchronization | Contributor/personal identity isolation exists; a complete selective user-data synchronization feature was not established | Distinct future programme | Design encrypted export/restore first; do not automatically copy personal data or keys |

Key source links: [dictation](../../src-tauri/src/dictation.rs), [cleanup](../../src-tauri/src/dictation_text.rs), [screen context](../../src-tauri/src/screen_context.rs), [routines](../../src-tauri/src/memcore/routines.rs), [scheduler](../../src-tauri/src/memcore/scheduler.rs), [notification opening](../../src-tauri/src/notify_open.rs), [existing task chips](../../src-tauri/src/background_tasks.rs).

## 4. Important distinctions

- Screen-context crop helpers currently describe primary-monitor physical pixels. A user-drawn selection across monitors and scaling factors is additional work, not a CSS-only addition.
- Current routine mining permits selected app/site/music actions. It is not a general unattended macro recorder, and should not become one implicitly.
- A timetable reminder and an arbitrary command scheduled for execution have different authorization and recovery requirements.
- The old notification-to-sidebar research is marked proposal-only, but current source includes opening transactions and expansion helpers. The new work must audit that implementation rather than follow the older document's absence claim.
- A prettier island cannot resolve duplicate notifications. The November 6 agent-watch cross-check still has three reproduced edge classes requiring repair and live qualification.
- NEXUS's existing dictation cleanup is conservative and deterministic. Polishing should not silently alter names, numbers, URLs or code, or replace the audio/authorization pipeline solely for visual parity.

## 5. Recommended product strategy

Keep NEXUS free/BYOK, Windows-first for 1.0, with its existing 600–700 MB local-RAM goal. Make provider availability and quota failures visible; do not promise unlimited free cloud inference. Keep other OS feature parity separate from successful compilation.

The highest-value additions are connected entry points into the current system: selection-to-sidebar analysis, direct dictation, editable routines/schedules and consistent completion/status cards. The differentiator can be verified task outcomes and coherent context across voice, screen, alerts and developer workflows. This is a product recommendation, not a proven market advantage.

Avoid treating screenshots, emails, calendar descriptions or model-generated instructions as authority to execute. Retain explicit permissions, cancel/recovery controls and contributor/personal isolation. No background full-screen monitoring, global clipboard archive, mandatory account sync, new paid provider requirement or gamification programme is proposed for the initial work.

## 6. What “synced with NEXUS” means here

This proposal interprets the request as **NEXUS's features sharing context and state**, rather than connecting NEXUS to a Hitodama account. No public integration API for that account was established by this research.

Examples of internal coherence:

1. Select a build error, ask about it, receive a sidebar explanation, then deliberately create a task linked to the same source snapshot.
2. Say a routine name or click it in the quick panel: both run the same validated routine and show the same job status.
3. An agent finishes: its existing task receipt creates one alert; clicking it opens the exact task rather than the latest unrelated result.
4. A scheduled routine is changed in the Hub: its voice query, quick-panel card and next-run time update from one authoritative record.

Cross-device synchronization is a separate later project. The detailed proposal is [the implementation plan](../features/hitodama-inspired-nexus-improvement-plan.md).

## 7. Limits and next evidence

This review did not run an installer, compare voice recordings, observe competitor animations, audit its binary, validate its privacy implementation or verify its current version. It also did not reassess every NEXUS feature or recalculate production scores.

A future authorized hands-on comparison should use the same Windows device and tasks, record product versions, separate user mistakes/provider failures from product failures, and measure completion accuracy, wrong-target actions, cancellation, latency and working-set memory. Until then, labels such as “faster”, “more accurate”, “more private” or “production-ready” are unsupported comparative claims.
