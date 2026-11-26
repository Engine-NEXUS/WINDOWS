# 199 - Sidebar design family: implementation record (2026-11-22)

Plan: `198-sidebar-design-family-analysis-and-plan.md`. Owner direction: every sidebar gets the interface that suits its own task, without conflicts, and the three missing sidebars (WhatsApp messages, Gmail review, screen time) are added.

## What shipped

| Sidebar | Identity | Where |
|---|---|---|
| WhatsApp messages (new) | "Chat": avatar header, day separators, bubbles (mine = ink, theirs = raised card), runs of one sender joined, read-only footer | `frontend/src/sidebar/views/whatsapp/` |
| Gmail review (new) | "Inbox briefing": two counters, mail grouped by topic (exams first), deadline pill and left rule on mail with a deadline | `views/gmail/` |
| Screen time (new) | "Ledger": today in large type, versus-usual line, seven day bars, top apps and sites | `views/screentime/` |
| Research / answers | "Briefing": editorial reading column, small-caps section rules | `views/research/research.css` |
| CodeRabbit, repo analysis, spatial, confirmation, desktop choices | shared kit header and cards from the first batch | unchanged in this step |

Rules every view follows: no dock arrows and no close button in the header (arrow keys dock, Esc closes), solid surfaces, no scrollbars, comfortable density, colours only from `theme/tokens.css` (`.view-whatsapp`, `.view-gmail`, `.view-screentime` token blocks, dark and light), one root class `.view-<name>`, a pure model with tests.

The GitHub dashboard (`frontend/src/pr-list/*`, `github-dashboard.html`, its spec) was read, never edited.

## Conflict gate

`scripts/check-sidebar-scopes.mjs` (CI step "Sidebar view scope gate"): in `views/<name>/*.css` every selector must start with `.view-<name>` and every `@keyframes` must be named `<name>-...`. `--selftest` proves it catches unscoped rules, unprefixed keyframes, rules inside `@media`, and look-alike roots.

## Wiring

- `assistantMeta.ts`: new kinds `whatsapp | mail | screentime`, chosen from `analysis.kind`; `OWN_VIEW_KINDS`.
- `SidebarApp.tsx`: those kinds render their own view (with its own header) instead of the generic header and response column.
- Rust: `run_whatsapp_read` and `run_mail_digest` now call `show_sidebar_with_analysis` with a structured payload (`wa::card_payload`, `briefing::mail_payload`); the markdown stays as fallback text. New intent `ParsedIntent::ScreenTime` ("show my screen time", "how much time have I spent on my laptop"), `run_screen_time`, and `device_activity::screen_time_card()` (payload plus one spoken line). The data is the encrypted local Activity Manager; when it is off the view and the spoken line say how to turn it on.
- No new IPC commands or events; the IPC contract gate is unchanged.

## Verification

- Frontend: `tsc` clean; vitest 1281 passed (new: WhatsApp 7, Gmail 7, screen time 8 model tests); colour, opacity, contrast, IPC, turn-end, hygiene, feature-registry and scope gates pass.
- `cargo check --lib` passes. New Rust tests: `card_payload`, `mail_payload`, four `screen_time_card` cases, `screen_time_phrases_parse_and_neighbours_do_not`.
- Browser review: `frontend/dev-sidebar-family.html` (dev only) renders the three real views with sample data; checked in dark and light.

## Not verified

- Live in the packaged app (needs a rebuild): the three views inside the real sidebar window, the screen time phrase by voice, WhatsApp and Gmail with real data.
- Rust: `cargo test --lib --offline` linked and passed here (the seven new tests plus the 408 related ones). A packaged release build with these changes was deployed to `C:\NEXUS-Dev` but the views were not exercised by voice yet.
- Restyle of CodeRabbit / repo analysis beyond the shared kit is not done in this step.
