# 171 — Notification event contract and click-to-context core (notification-to-sidebar N0–N5)

Date: 2026-10-28. Scope: N0 and N1 only. Source + pure tests; no NEXUS build/start/stop/settings change, no provider or credential access.

## N0 findings (source-backed)

- `notify_brain::Fact` carries only `service`, bare `project` name, `kind` and `run_ref`; its entity key is `service:project:kind` (no owner, host or account). Two owners with a repo of the same name share one entity today.
- `github_watch` knows the owner (it polls `owner/repo`) but reduces it to the bare name before `observe_fact`; the PR number is not in the fact (needs the run's `pull_requests`).
- `commands.rs` opens the unified sidebar with `prepare_sidebar` + `PENDING_SIDEBAR` (cold mount) and `sidebar:analysis` (warm). Geometry is zero-based from `primary_monitor` size; no work-area/origin use found. Tauri 2.11.5; deep-link plugin present, notification plugin absent.
- Files N2 will edit: `notify_brain.rs` (full-key facts), `memcore/github_watch.rs`, `memcore/deploy_watch.rs`, `memcore/mailwatch.rs`/`google/sentinel.rs` (producers), `commands.rs` (pending payload), `frontend/src/sidebar/*`.

## N1 delivered

- `src-tauri/src/notify_event.rs`: versioned `NotifyEvent`, tagged `Target` (GitHub/Gmail/Deployment/Repo), full-identity `entity_key`, deterministic opaque `event_id`, strict `parse_event`, bounded `Ledger` (cap 200) with `record` (Created/Duplicate/Grouped/Recovered), `resolve` (NotFound/Expired/AccountDisconnected, no fallback), separate delivery and NEXUS-only read state.
- `notify_brain::Book::rekey`: moves an entity to a new key with its whole state; refuses to overwrite; keeps quiet-baseline semantics (a rekeyed failure stays silent on the same run).
- Migration design (not wired): legacy `github:<project>:<kind>` records are re-keyed to the full key only when the owner/account is known from the producer; unknown ones stay under the legacy key and age out (30-day retention). A rekeyed record keeps `last_seen_ms`/`spoken_failure`, so nothing replays as a fresh alert.

## Tests and mutations

- `cargo test --lib notify_event`: 15 pass. `notify_brain`: 16 pass (3 new).
- 8 mutations, all killed by test failures (not compile errors): entity-key check, event-id check, provider/target check, expiry, disconnected account, account in key, redaction, `rekey` overwrite guard.

## Coding arrangement

`opencode/glm-5.3-flash-free` wrote a first `notify_event.rs`. It had a syntax error, tests with mismatched entity keys (recovery could never match) and no forged-id/provider checks. The supervisor repaired and extended it; the final file is substantially the supervisor's. No other model was used.

## Ratchet note

The module and `Book::rekey` are compiled under `cfg(test)` only: the dead-code ratchet (145 known, none new) forbids unused production code and N1 has no producer yet. N2 removes the gates when it wires them.

## N2 delivered (owner said "proceed" after N1)

`src-tauri/src/notify_open.rs` (`cfg(test)` only, like N1): a pure opening transaction. `Opener::begin` resolves an opaque event id through the ledger and returns an `EventDetail` (typed target, original and latest source ref, allowlisted GitHub URL) or a read-only `EventUnavailable` (NotFound/Expired/AccountDisconnected, never fallback content). Each click gets a generation token: a later click supersedes an earlier one, late data/ready/close for the old token is refused, the cold-mount `take_pending` returns only the matching payload once, and a pinned view is remembered and returned on close (a second click keeps the original pinned view). Opening only marks NEXUS's own alert Seen.

Tests: 9 in `notify_open` (exact target vs same-named/newer, every unavailable reason, A to B supersede with late A data, cold-mount token, pinned restore plus stale close, read-only open, recovered context, URL allowlist). Mutations: 8 run; 7 killed first time, 1 survived (re-clicking the SAME alert then using the old token); a test was added and that mutant is now killed. All kills are test failures, not compile errors.

## N3 delivered: wired end to end (owner said "just build it")

- Producer: `memcore/github_watch.rs` — when a failure/recovery is spoken, and the bare project name resolves to exactly ONE `owner/repo` (`github_repo_index`), `notify_hub::publish_github` records a `NotifyEvent` (full owner, run id, account binding `gh-primary`). Ambiguous or unresolvable repos get speech only, no clickable card. Only `Created`/`Recovered` outcomes show a card; `Grouped`/`Duplicate` never do.
- `notify_hub.rs`: process-wide ledger + opener; commands `notify_open_event`, `notify_close_event`, `notify_dismiss_event` (registered, called by the UI); pure `github_event` and `detail_json` (4 tests).
- Stage: `NotifyCard.tsx` (+ `notifyCardModel.ts`, `notify-card.css`, solid, registered in the opacity registry) — compact top-right card, never takes focus, click/Enter/Space opens, Escape or × dismisses, auto tucks away after 14 s, hit region registered under source `notify` and cleared when hidden, reduced-motion respected.
- Sidebar: `commands::show_event_detail` shows `EventDetailView` in the existing right-docked sidebar (cold mount via `PENDING_SIDEBAR`, warm via `sidebar:analysis`). The view shows status, repository, run, first/latest run, failing-run count, an "Open on GitHub" button (URL built in Rust from the typed target), and Back (restores a pinned view such as the PR list). Unavailable/expired/disconnected alerts show a read-only message. Closing the sidebar ends the transaction (stale tokens ignored).
- Ratchet: unused-by-production pieces are `cfg(test)` (`parse_event`, `Ledger::len`, `Opener::ready/accept_update/take_pending/is_open`, `Accept`, `Book::rekey`); they belong to the N4 ready-handshake and persistence work. Dead-code ratchet 145 known, none new.

Checks run: Rust `notify*`/`github_watch`/`commands`/`coderabbit`/`sidebar` tests 165 pass; frontend `tsc` clean, vitest 722 pass; IPC contract (242/182), hygiene, turn-end, window-opacity, dead-code gates pass.

## N4 delivered: work-area geometry, bounded ready handoff, opening state

- `work_area.rs` (pure): `sidebar_rect` places a sidebar view/dock inside a monitor's WORK AREA (origin included, so monitors left of/above the primary and taskbars on any edge are respected), converts logical to physical once, and always clamps inside the work area even on small displays; `insets` finds a taskbar on any edge. 7 tests (negative x and y origin, 150% scale, left/top/right/bottom taskbar, 500x400 display for four views and three docks, bad scale).
- `prepare_sidebar` (all sidebar views, not just notifications) now uses it: it reads `Monitor::work_area()` and sets size/position in physical pixels; the backdrop capture rectangle uses the same rectangle. Behaviour change to watch live: on a single monitor the panel is now bounded by the work area instead of `monitor height - 40`, and the old zero-based physical clamp is gone.
- Card position: `notify:card` carries the primary monitor's top/right insets; the card sits 16px plus those insets from the edge.
- Ready handshake: `notify_open_event` returns the opening token; `EventDetailView` calls `notify_event_ready(token)` on mount; Rust accepts only the live token and emits `notify:ready`. The card stays as a cover in an `opening` state ("Opening..."), leaves when the matching token is ready (either arrival order), or after 1.5 s regardless; an open failure shows "Could not open this alert" for 4 s. Leaving fades 180 ms toward the sidebar; reduced motion removes the motion.
- Tests: `work_area` 7, `notify_open` 9, card model 6; mutations: 7 more killed (origin ignored, no clamp, no width clamp, scale not applied, negative inset, scale sanity, ready token check), all by test failures.

## N4 second cross-check (independent reviewer) and fixes

The reviewer (read-only) found no off-screen/wrong-monitor placement and no path to another account/repo. It found, and I fixed (each with a test or mutation):
- Two quick clicks could interleave across the sidebar awaits and show alert A while B was live: opens are now serialized by an async gate (`notify_hub::open_gate`; reviewed, no unit test for the async interleaving).
- A late `.then` of an earlier open could finish the card for a newer click: opens carry a sequence number and stale results are ignored.
- A repeat `notify:card` for the alert being opened brought the card back for 14 s: ignored while opening/leaving.
- A dead transaction (sidebar closed without the close call) leaked its restore view into a later alert: inherited only while the assistant view is still showing (test + mutation).
- Recovery/grouped events kept the old title and the first run link: both now refresh title/preview/target to the latest run (tests + mutations).
- No cleanup cleared the `notify` hit region if the card unmounted: added.
- `notify:card` / `notify:ready` now go to the stage window only (`emit_to`), not every webview.
Left as noted, not changed: the panel lands on the window's current monitor (not necessarily where the owner is working); opening an alert replaces a non-assistant sidebar view (e.g. settings with an unsaved draft), Back re-shows it fresh.

## N5 delivered (desktop activation routing) and what is NOT delivered

- `notify_activation.rs`: strict parser for `nexus://notify/open?event=ev_<24 hex>`; wired into BOTH existing paths (single-instance second launch and `on_open_url`) to `notify_hub::open_event`, the same function the card click uses. No other host/path, extra parameter, fragment, encoded, upper-case, short or prefixed-twice id is accepted (4 mutations killed; the one initial survivor, prefix not required, got two new test cases). An unknown/expired/forged activation ends in the read-only "unavailable" view. The single-instance log line no longer prints the deep-link URL (it could contain an OAuth code).
- Native toast fallback (written, NOT verified on a device): `notify_native.rs` builds toast XML whose only link is the activation URL (`activationType="protocol"`, id validated, title/body XML-escaped and bounded) and shows it on Windows through `Windows.UI.Notifications` (added `UI_Notifications` and `Data_Xml_Dom` features to the existing `windows` 0.36 dependency; other platforms return an error). `choose_channel` picks exactly one channel: the card when the stage can show it, else a toast only if `notifyNative` is true (new setting, default false, set it in settings.json; there is no Hub switch yet), else history only. Tests 4, mutations 5 killed (toast on top of a card, `<` and quote not escaped, controls kept, title not bounded).
- Still unverified and required before N5 can be called working: the toast needs the AppUserModelID `com.nexus.assistant` tied to an installed Start-menu shortcut (otherwise Windows may drop it silently), the `nexus://` scheme registered by the installer, and the protocol launch must reach the running instance (single-instance path). A cold launch from a toast has an empty ledger and shows "no longer available". macOS/Linux: not implemented.

## `nexus notify <scenario>`: trigger each notification on demand

With NEXUS running (`nexus start`), run in another terminal. The command makes a second launch of the built exe with `--notify-test <name>`; the running instance receives it through its single-instance callback and runs the scenario through the REAL pipeline (state machine, ledger, card, sidebar, optional toast). A launch flag cannot be sent from a web page (unlike a `nexus://` URL). The command refuses if NEXUS is not running and never starts, stops or rebuilds anything. Watch the `nexus start` console for `[NOTIFY-TEST]` lines (added to the run.ps1 whitelist).

| Command | What it does | What you should see |
|---|---|---|
| `nexus notify pr-failed` | Failed PR fact, new run id | spoken "PR failed in selftest", card; click opens the detail (repo `nexus-test/selftest`, run, "Open on GitHub") |
| `nexus notify build-failed` | Failed build fact | spoken "Build failed in selftest-build", card |
| `nexus notify still-failing` | another failing run of the same PR entity | no new card or speech; opening the existing alert shows 2 failing runs and the newest run link |
| `nexus notify recovered` | Fixed fact | spoken "selftest is green again", card; detail shows Recovered and the original first run |
| `nexus notify expired-soon` | alert that lives 15 s | card now; clicking after 15 s shows "This alert has expired." |
| `nexus notify same-name` | `alice/selftest` and `bob/selftest` | two distinct alerts; each click shows its own owner |
| `nexus notify ambiguous` | speech only | a spoken line and no card |
| `nexus notify native` | forces the Windows toast | a toast (not the card); its click must open the same detail (checks AppUserModelID and the `nexus://` scheme) |

Full cycle: `pr-failed`, `still-failing`, `recovered`, then it can be repeated. Not simulable safely: a disconnected GitHub account; pinned-view restore is manual (open the PR list, run `pr-failed`, click, Back). Test alerts leave a few small `selftest*` entity rows in the sealed store (pruned after 30 days).

Tests: `notify_selftest` 6 (strict flag parsing, fresh run ids and test-only names, scenario flags, the state-machine outcomes in run order, ledger grouping/recovery/same-name, and a check that `nexus.mjs` lists exactly the app's scenarios). Mutations: 7 killed (flag prefix match, ambiguous publishes, no TTL, still-failing never reaches the ledger, same-name speaks twice, native not forced; the last survived first and was fixed by moving the channel decision into the pure `channel_for`). The CLI was not run against the live app (NEXUS was running; I did not send it test alerts).

## Redesign after the owner's first live look (supersedes the in-app card)

Owner feedback: the sidebar showed a black border with a CodeRabbit button and dock buttons instead of the CodeRabbit interface; NEXUS should not have its own notification centre; the alert must come from the laptop's notification centre and, when clicked, a full-screen overlay animation must enlarge the notification into the sidebar with the review in detail.

What changed:
- **Channel:** the alert is a Windows notification-center toast (`notify_native.rs`; `activationType="foreground"`, the click is handled in-process, an HKCU `AppUserModelId\com.nexus.assistant` entry gives it the name "NEXUS"). The in-app card, its hit region, dismiss button and the `notify:card` / `notify_open_event` / `notify_dismiss_event` plumbing are removed. Setting `notifyNative` (default now ON) switches toasts off; then alerts stay in history only.
- **Click:** `notify_hub::activate` emits `notify:expand` to the full-screen stage overlay; `NotifyExpand.tsx` draws a card at the toast position (bottom-right of the work area, `work_area::expand_rects`), grows it (layout, not a scaled snapshot) into the sidebar's real rectangle in 280 ms, keeps it as a cover until the sidebar reports `notify:ready` (1.5 s bound), then fades. After the animation the alert opens.
- **Sidebar:** GitHub alerts now open in the **CodeRabbit view** (595x975, docked right) through `EventDetailView`, restyled with the CodeRabbit panel: the alert (status, repo, run, first/latest run, failing-run count, Open on GitHub) on top, then CodeRabbit's review of the pull request (`coderabbit_review_pr`; the PR is found from the workflow run with `coderabbit::pr_number_for_run`), or a plain note when the run has no PR. The CodeRabbit button and the left/right dock buttons are gone from the assistant header; a thin invisible strip at the top drags the window, and Left/Right arrow keys (click the window first) move it left, centre, right (`dockKeys.ts`, one handler for every sidebar view).
- **Test command:** the `native` scenario is replaced by `nexus notify open`, which simulates clicking the latest toast (animation + sidebar) so the visuals can be checked even if Windows does not show the toast.
- **Scope decision recorded:** PR/failure/analysis content goes to the CodeRabbit interface. Commits, issues and other GitHub items are not routed there; the existing PR-list window with Merge/Analyse actions is unchanged (a later decision).

Checks: Rust library 2,792 tests (one pre-existing env-var flake `google::auth::test_auth_with_env_token` fails when its module runs alone and passes in the full run), frontend 722 tests, IPC (241 commands), hygiene, turn-end, window-opacity and dead-code gates pass. Mutations for the new pieces are listed in the session report. Not verified on a device: the toast actually appearing and its click reaching NEXUS (Windows notification settings, Focus assist), the animation's look, and the CodeRabbit review for a real PR (CodeRabbit must be installed on the repo).

## Toast diagnosis (2026-10-29, owner reported "no laptop notification")

- Live log: both `nexus notify pr-failed` runs were handled by the running app and stayed silent in the state machine, so the toast code was never reached (the HKCU `AppUserModelId\com.nexus.assistant` entry did not exist either, which confirms it). Cause of the unexpected silence is not proven; a failing `selftest` entity was apparently already stored. Fix in source: fresh scenarios now use a new `selftest-<tag>` name every time (needs a rebuild).
- Windows side tested by hand: from Windows PowerShell and from the app's own Rust code (`cargo test --lib -- --ignored manual_toast_smoke --nocapture`): the notifier reports Enabled, the toast is accepted and the Action Center history for `com.nexus.assistant` grows (4 toasts after the tests). Whether the banner was visible on screen must be confirmed by the owner. These tests created the registry entry that the app would create itself.
- Known limit: the click handler lives in the running process; clicking a toast in the Action Center after NEXUS was restarted does nothing (no COM activator is registered).

## Do Not Disturb: the `urgent` scenario (2026-10-29)

Finding: Claude's and Codex's toasts carry no `scenario` attribute and have no special per-app registry value, so that is not how they pass Do Not Disturb. The probe (`scripts/notify-dnd-probe.ps1`, one toast per scenario) made Windows show **"Important Notification Request: Allow important notifications from NEXUS? You will get important notifications from NEXUS when do not disturb is on."** while the `urgent` toast was sent. That prompt is Windows' opt-in for passing Do Not Disturb.

Change: failure toasts now carry `scenario="urgent"` (`notify_native::toast_xml(.., urgent)`, `notify_hub::wants_urgent`); recoveries do not. Setting `notifyUrgent` (default on) turns it off. NEXUS cannot grant itself the permission: the owner clicks **Allow** once in the Windows prompt (or per app in Settings, Notifications, NEXUS). Tests: `toast_xml` urgent/normal, `wants_urgent` truth table; 3 mutations killed (recovery also urgent, setting ignored, attribute dropped). Alarm/incomingCall were not used: they stay on screen and loop a sound.

## CodeRabbit report, FREE only (2026-10-29; plan: mossy-floating-sutherland)

- **Sidebar height restored:** alerts open in the `coderabbit` view, which had been capped at 975 px, and panels were sized from the work area (above the taskbar). Now full height again (`min(1040, monitor_h - 40)`, 20 px margins, sized from the whole monitor; only the monitor ORIGIN is new). Test + mutation.
- **What CodeRabbit gives (verified on this PC, CLI 0.9.0):** signed in as the owner; organizations ChitkulLakshya (active), Kryptes-Vault, Leddger-AI, Servx-lab, VMAX-OCEAN, zync-meet; `Engine-NEXUS` is not one. `pullrequest` only works for the ACTIVE organization (error text quoted in the plan); CodeRabbit has commented on 0 PRs in any of the owner's accounts; `usage` shows 10 included reviews per rolling hour, billing inactive; `review --remote owner/repo --base b --source-branch ref --agent` reviews on the server without a checkout. `review --agent` findings carry severity (critical, major, minor, trivial, info, none), file, title, comment, `codegenInstructions`, suggestions, line range, and `complete` carries gate/unreviewed/reused. There is no type field; CodeRabbit's types are Potential issue, Refactor suggestion, Nitpick (comment header).
- **Built:** `coderabbit_review.rs` (pure parsers + thin runners) and the `coderabbit_report` command: (1) what CodeRabbit already posted on GitHub, (2) else, if `coderabbitAutoReview` (default on) and the plan still has free included reviews, one server review per PR head commit (cache, one at a time, 120 s), (3) else the exact reason and fix as notes (wrong active org: run `coderabbit auth org`; owner not a CodeRabbit org: add it; no free reviews left). **Credits are never used**: no `--use-credits`, and `free_review_allowed` needs `includedReviews.available` with `remaining > 0` whatever the billing status says.
- **Categories:** CodeRabbit's own axes only: severity (`critical, major, minor, trivial, info`, else `unknown`, never guessed) and type (from the comment header; for CLI findings derived and always shown as "(inferred)"). The old `warning/suggestion/praise` labels are gone.
- **UI:** the alert view shows the source badge, the free-review budget ("7 of 10 free CodeRabbit reviews left this hour"), gate, summary, notes, severity filter chips with counts, findings grouped by severity with type, file:lines, CodeRabbit's fix instructions, suggestion blocks and a "Copy prompt for an AI agent" button (`reportModel.ts`, `CodeRabbitReport.tsx`).
- **Tests:** Rust 2,805 (new: NDJSON parsing incl. unknown records/errors/skipped, header parsing, access classification, usage and free guard, argument safety, cache), frontend 728. 9 mutations killed (no remaining check, state ignored, credits flag added, flag-like value allowed, sha ignored in cache key, active org not recognised, inferred flag dropped, header match loosened; the last survived first and got a test).
- **Not yet verified:** a REAL server review. `--remote` may not work for `Engine-NEXUS` because CodeRabbit does not know that organization; the code then reports the reason instead of failing. The first real run uses 1 of the 10 free reviews and sends the branch's diff to CodeRabbit, so it needs the owner's go-ahead.

## Owner live checklist (card steps now mean the Windows notification) (after you run `nexus build`)

1. Push a PR/commit that fails a GitHub Action in a repo you own. Expect one spoken line and a card top-right; no focus change.
2. Click the card: the sidebar opens on the exact repo/run with an "Open on GitHub" button that opens that run.
3. Open the PR list first (pinned), then click a card: "Back" returns to the PR list.
4. Fix the run: recovery updates the same alert (status Recovered, original run still shown).
5. Let a second failing run happen: no second card (grouped).
6. Repeat with two repos of the same name under different owners (or a repo with several owners): only a uniquely resolved repo gets a card.
7. Dismiss with × and Escape; check no click-blocking area remains.
8. Multi-monitor/125%/150% display scaling: card position and hit area.

Not done: N5 device verification, N6 acceptance and all device proof, Gmail and deployment producers (only GitHub is wired), persistence across restarts (ledger is in memory), the Hub switch for the card. Animation is a short slide-in, an opening state and a fade-out; there is no shared-element morph (separate WebViews cannot share one).

## Not done / not run

N2–N6 (click routing, card, animation, native adapter, device proof), persistence table, wiring producers to full keys, live provider/Gmail checks, full Rust suite, native build and device gates. Not production ready; no queue advancement.

## CodeRabbit review on request, free-limit aware, plus AI analysis (2026-10-29)

- **Replaces silent auto-spend.** Reading what CodeRabbit already posted stays automatic and free; spending a free review now needs consent. `coderabbitAutoReview` defaults OFF and `coderabbit_report` is read-only.
- **Say or press:** "give me coderabbit review", "show me the coderabbit review", "use coderabbit to review this", "coderabbit this PR", "what does coderabbit say about this" (`coderabbit_phrases.rs`, `ParsedIntent::CodeRabbitReviewThis`). Hub phrases ("open coderabbit", "show coderabbit review") and explicit PR numbers keep their old routes. "This PR" is the PR the sidebar shows (`coderabbit_set_context`).
- **Consent:** NEXUS reads `coderabbit usage`, says "You have 7 of 10 free CodeRabbit reviews left, sir. Use one on PR 34 in zync?" (`Offer::CodeRabbitReview`, 5 min window, not answerable by an uncertain voice) and shows the same question inline. Only Yes (voice) or the Review button starts it (`start_review`; one run per PR, a second request reuses the running one; refused at 0 left with the time until the allowance is back).
- **Allowance display:** "7 of 10 free reviews left · back to 10 in ~50 min" from CodeRabbit's own `fullCapacityInMs`; the UI re-reads usage every 30 s and once when that time elapses, so the count climbs back by itself. The exact per-unit refill is not documented; the figure shown is the CLI's.
- **Long job:** rotating spoken lines (On it / Give me a minute / I'll get back to you), a spinner + sweep + elapsed timer, job polled via `coderabbit_job`, finish spoken through `proactive_policy`.
- **AI analysis (`ai_review.rs`), on request only:** the owner's own Gemini/Groq/Cerebras key via `router`, labelled "AI analysis · not CodeRabbit" with provider/model. Diff and the failing job's log tail are untrusted data: secrets redacted first, lockfiles/generated/binary skipped and listed, at most 3 calls of about 18k chars, random delimiters, no tools, findings validated against real new-side diff lines, patches shown never applied.
- **Tests:** Rust lib green; 25 coderabbit + 8 ai_review + phrase/parser tests; frontend 731. Mutations killed: line validation, redaction, free-review guard (the redaction test found a real gap, `Authorization: Bearer`, fixed). Gates: IPC contract, hygiene, window-opacity, turn-end, dead-code ratchet (145, none new).
- **Not run:** a real review through the new button (spends 1 of 10 and sends the diff to CodeRabbit; needs the owner's go-ahead), a real AI call with a key, anything spoken, the running app (needs rebuild and restart).
