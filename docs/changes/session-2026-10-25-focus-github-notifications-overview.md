# Session overview (2026-10-25/26): notification brain review, Focus Mode, owner-less GitHub commands, name-free wording, state machine, GitHub Actions watcher, feature work cards

One index over the change records written in this session, what each delivered, what was found in review, and the exact verification state. Plan and decisions: [feature 153](../features/153-focus-mode-github-resolution-and-notification-programme.md). Evidence: [research record](../research/focus-github-notification-research-2026-10-26.md). Nothing is committed; every item below is **unit-tested but not live-proven**.

## 1. Record index

| Record | Subject |
|---|---|
| [157](157-notification-brain-phase-a-b.md) – [159](159-notification-brain-phase-e-funnel.md) | Notification brain phases A, B, D, E, built before this session by another model (event keys, 5-minute Medium cooldown, short one-liners, WhatsApp funnel) |
| [160](160-notification-brain-review-pass.md) | Cross-check of 157–159: gate before wording, key released if the alert expired unspoken, fixed 2-minute High guard, batch key only for one event, cloud e-mail rewrite default OFF, false claim about `merge_agent_alerts_into_return` corrected |
| [161](161-focus-mode-hand-mode-background-gating.md) | Focus Mode lease and own-loop gating |
| [162](162-owner-less-github-repos-and-name-free-notification-lines.md) | Repo index, "analyse zync", "the PR in zync" lists, bare PR number search, pinned sidebar list, self-name scrub and tail-address wording |
| `163-focus-mode-lowers-other-apps.md` | Lowering other heavy apps with exact restore (number 163 is shared with the Voice Lock record `163-voice-capture-accounting.md`) |
| `165-notification-state-machine.md` | Failed / still failing / fixed state machine and e-mail feed (number 165 is shared with `165-voice-lock-live-verification-fixes.md`) |
| [166](166-github-actions-watcher.md) | GitHub Actions watcher |

Refer to colliding numbers by full file name; they were not renamed because other documents link to them.

## 2. What exists now (files)

New Rust modules: `focus_mode.rs`, `focus_throttle.rs`, `github_repo_index.rs`, `notify_brain.rs`, `memcore/selfnames.rs`, `memcore/github_watch.rs` (and, from the earlier model, `notify.rs`, `memcore/mailwording.rs`). New store table `entity_state` (`memcore/store.rs`). New script `scripts/make-work-cards.mjs` with 67 generated files under `docs/testing/work-cards/` and `docs/testing/feature-process-map.md`.

Changed (narrowly): `lib.rs` (module registration, startup, exit restore), `commands.rs` (settings `handFocusMode`, `handFocusLowerApps`, `memcoreGithubWatch`; `memcoreMailPlain` now defaults to false), `hand/mod.rs` (lease hooks), `proactive_policy.rs` (focus signal, High guard, tail-line count-up), `intent_parser.rs`, `github_cmd.rs`, `center.rs`, `orchestrator.rs` (repo resolution, `FindPr`), `memcore/{mailwatch,mailwording,mailtriage,mod,store}.rs`, about 15 loops gated for Focus Mode, `agent_watch.rs` (no raw error text), frontend `prListStore.ts`, `PrListApp.tsx`, `HandModeSection.tsx`, `SettingsSidebarApp.tsx`, `memoryModel.ts`, `MemoryPage.tsx`.

New settings (all in `NexusSettings`, because settings.json is rewritten from that struct): `handFocusMode` (true), `handFocusLowerApps` (true), `memcoreGithubWatch` (true, inert without a GitHub token); changed default `memcoreMailPlain` (false).

## 3. Review findings (what the cross-check and hand review caught)

1. A call to `merge_agent_alerts_into_return()` existed in an uncompiled file while record 157 claimed it was implemented; it was added, then removed because nothing compiled calls it and the ratchet rejects it.
2. The cloud e-mail rewrite was default ON while the docs said "default off, zero credits"; cooldown ran after wording so suppressed repeats could still call a model.
3. A mixed batch of e-mails was keyed by its first mail; keys were marked "heard" before the alert spoke; High repeats had no guard.
4. Antigravity output needed hand repair: rewritten `proactive_policy.rs` glue (restored from git), a dead "superseded" parser copy, a dropped `#[test]`, type errors, tests that did not compile, duplicate module helpers, a clip test off by the ellipsis, and wrappers only tests used.
5. The first watcher draft treated any 403 as a rate limit because GitHub sends the rate-limit header on every response; one unreadable org repo would have stopped the whole poll.
6. A missing TypeScript settings field after each new switch (twice).

## 4. Verification state

- Rust: `cargo test --lib -- --test-threads=1` **2,623 passed, 0 failed**, 31 ignored; `cargo check --lib` with no new warnings (remaining warnings belong to other sessions' files).
- Frontend: `tsc --noEmit` clean; vitest **692 passed**.
- Gates: turn-end, repository hygiene, IPC contract (236 commands), window opacity, **dead-code ratchet 148 known / none new**, work-card drift check.
- Mutations killed by tests: PID-reuse check removed, meeting guard removed, self-name match disabled, Medium cooldown window zero, High guard off, release-on-expiry removed, batch key = first mail, forget window removed, "fixed spoken even if the failure was never heard", "any 403 is a rate limit".

## 5. Not verified

Everything by voice and on hardware: Focus Mode with a real camera and a heavy app (Task Manager before, during, after, and after killing NEXUS); the three GitHub phrases against real accounts, an org with SSO, and a name under two owners; a real failing workflow, its fix, and a restart while failing; the Hub switches in the running app; the full CI workflow itself. The Hand Mode lag claim is untested: no baseline `[HAND]` numbers exist, so the benefit of Focus Mode is unmeasured.

## 6. Next
Vercel, Render and Netlify deploy watchers (designed in feature 153 section 3, brief ready; a first Antigravity run was stopped before it wrote any file), then the open items in feature 153 section 5.
