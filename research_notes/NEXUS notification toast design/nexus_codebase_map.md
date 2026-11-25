# NEXUS codebase map for a notification-toast design (local files only, repo D:\ULTRON, branch feat/plain-windows-maps-panel-repo-from-screen)

Citation convention: sources are local files, written `path:line` (line numbers read on 2026-11-04; paths relative to `D:\ULTRON\src-tauri\src` unless prefixed `frontend/`, `scripts/`, `docs/`). Nothing here was run; this is a read-only code survey. No web sources were used, as instructed.

HEADLINE (read this first): a notification pipeline with a Windows-native toast, an event ledger, a click-to-sidebar transaction and a full-screen expand animation ALREADY EXISTS and is committed (commit 82a71cc "Notification hub, CodeRabbit on-request review..."). It is wired for GitHub Actions failures only. `proactive_policy`'s `proactive:card` / `proactive:nudge` events have NO listener anywhere. So the toast design is mostly "extend producers + (maybe) replace/augment the native channel", not "build from zero". Owner history matters: the owner explicitly rejected an in-app card in favour of the Windows notification centre (docs/changes/171, "Redesign after the owner's first live look").

---

## 1. Alert pipeline: what exists, and what each source knows at alert time

### Takeaway
All spoken/proactive alerts funnel through `proactive_policy::submit` / `submit_event`, which carries only `{id, text, urgency, event_key}` (no URL, no ids beyond the string id). Only the GitHub-watch path additionally feeds the typed ledger (`notify_hub::publish_github`) that can drive a clickable toast. Every other source (mail, deploys, WhatsApp, calendar, agents) drops its typed identifiers before `submit`, but most of them still hold those identifiers one or two stack frames earlier, so adding a typed side-channel is small plumbing.

### Cited Findings

#### 1a. Core engine and event plumbing
- `Alert { id, text, urgency, event_key }` is the only payload the engine sees — `proactive_policy.rs:92-100`. `Action::Speak{id,text,lead_in,override_meeting}` and `Action::Card{id,reason}` — `proactive_policy.rs:103-108`.
- `Action::Card` emits `proactive:card` with `{id, reason}` only (no text!) — `proactive_policy.rs:455-458`. `Action::Speak` emits `proactive:nudge` `{id, critical}` then sleeps `HEADS_UP_MS=300` and calls `orchestrator::speak_line` — `proactive_policy.rs:459-491`.
- NOBODY listens: `grep proactive:` in `frontend/src` returns nothing; `scripts/baselines/events-unmatched.txt:7-8` lists `emitted-never-listened proactive:card` and `proactive:nudge`. The `proactive_snooze` command is also unused by the frontend (`scripts/baselines/ipc-unused-commands.txt:46`; registered `lib.rs:1359`).
- Urgency tiers and rules (module doc, `proactive_policy.rs:14-31`): Low = card only; Medium = speak only if idle >= 30 s (`MEDIUM_IDLE_MS`, line 38) and not busy, expires 15 min; High waits for breakpoint, in meeting waits up to 30 min, expires 10 min; Critical speaks within ~8 s, even in meetings if `proactiveCriticalInMeeting` (default true); rate limit >= 20 s gap, <= 6 per hour (lines 46-48); event-key cooldown Medium 5 min rolling (line 51), High 2 min fixed (line 57), ring of 200 keys (line 53). `evaluate()` Medium branch: `busy = meeting || focus || user_speaking || nexus_speaking || working` — `proactive_policy.rs:124-133`.
- `Signals { user_speaking, nexus_speaking, working, meeting, focus, idle_ms }` and `collect_signals` — `proactive_policy.rs:63-78, 436-450`. Sources: `wakeword_oww::stt_user_speaking()`, `MeetingState::should_suppress_tts()` + `tts_playing`, `ghost::drill_running() || orchestrator::has_active_request()`, `focus_mode::paused()`, `LAST_USER_ACTIVITY_MS` (NEXUS interaction only, via `note_user_activity`, line 419). NOTE: the task brief mentions a `someone_is_here` helper — `grep someone_is_here` finds nothing in `src-tauri/src`. The nearest real presence signal is `memcore::resume::user_idle_secs()` (OS-level keyboard/mouse idle, `memcore/resume.rs:194` Windows / `:208` other) used by `mailwatch.rs` (user_active < 300 s) and `agent_attention.rs:idle_ms()`.
- Config is re-read from `settings.json` on every submit (`read_cfg`, `proactive_policy.rs:423-434`); only `proactiveCriticalInMeeting` is read there.
- `notify.rs`: pure `event_key(source, thread_id, sender, subject)` and `stem(subject)` — `notify.rs:26,51`. Key shapes: `mail:gh:<owner/repo>:<stem>`, `mail:thread:<id>`, `mail:<localpart>:<stem>` (doc `notify.rs:8-17`).
- `notify_brain.rs` (1067 lines): per-entity state machine. `Fact { service, project (bare repo name), kind (Pr|Build|Deploy), obs (Failed|Fixed), run_ref }` — `notify_brain.rs:49-56`. Entity id = `{service}:{project_lower}:{kind}` — `:59-61`. `say()` builds `id = nb_{eid}_{now_ms}`, `event_key = nb:{eid}:{tag}`, always `AlertUrgency::Medium`, then `proactive_policy::submit_event` — `notify_brain.rs:405-435`. Timers: `FAIL_FORGET_MS=12h`, `STILL_FAILING_MS=6h`, `RETENTION_MS=30d`, `BOOK_CAP=300` — `:13-23`.

#### 1b. THE EXISTING NOTIFICATION HUB (committed, GitHub-only)
- `notify_event.rs`: versioned `NotifyEvent` (event_id `ev_<24 hex>`, entity_key, provider, account_id, kind "pr|build|deploy|mail", status Failed|Recovered|Info, severity, times, title <= 120, preview <= 240, `privacy` Public|Redacted, typed `Target`, `source_ref`, `original_source_ref`, `occurrences`, `delivery`, `read`, `expires_at_ms`) — `notify_event.rs:92-124`. `Provider` already includes `Gmail, Vercel, Render, Netlify, Supabase` (lines 19-26). `Target` variants: `GitHub{host,owner,repo,pr,run_id,sha}`, `Gmail{thread_id,message_id}`, `Deployment{provider,team,project,deployment_id,revision}`, `Repo{...}` — `:62-89`. Ledger cap 200 (`LEDGER_CAP`, line 13); `Ledger::record` returns Created/Duplicate/Grouped/Recovered, groups repeated failures into one event with `occurrences` and refreshes target to the newest run — `notify_event.rs:375-432`. Redacted events drop preview — `:381`. There is NO persistence (in-memory ledger; docs/changes/171 "Not done").
- `notify_hub.rs`: process-wide `Hub{ledger, opener}` (lines 22-31), `github_event(full_name, fact, line, now)` builds the event only if `full_name` has an owner and `run_ref` parses as u64 — `notify_hub.rs:51-86`. `publish_github[_with]` records, marks `delivery`, and shows the native toast — `:95-160`. Only failures are `urgent` (pass Do Not Disturb), gated by setting `notifyUrgent` — `:162-165`. `activate(app,event_id)` = emit `notify:expand` to `stage`, sleep `EXPAND_MS=280`, then `open_event` — `:184-196`. IPC: `notify_event_ready`, `notify_close_event` — `:304-322` and registered `lib.rs:1219-1220`. Detail view is hard-wired to sidebar view `coderabbit`, dock `right` (`DETAIL_VIEW/DETAIL_DOCK`, `notify_hub.rs:25-26`).
- `notify_native.rs`: the toast channel. `Channel::{Toast, HistoryOnly}` ("Exactly one; there is no in-app card", `:14-19`). `toast_xml(event_id, title, body, urgent)`: ToastGeneric with two `<text>` nodes, `activationType="foreground"`, `launch="nexus://notify/open?event=<id>"`, optional `scenario="urgent"`; title <= 120, body <= 240, XML-escaped; no URLs inside — `:50-63`. `APP_ID = "com.nexus.assistant"`, HKCU `Software\Classes\AppUserModelId\com.nexus.assistant` DisplayName "NEXUS" written by `ensure_registered` — `:65-78`. `show()` uses `Windows.UI.Notifications` (ToastNotificationManager.CreateToastNotifierWithId) and `std::mem::forget(toast)` to keep the activation handler alive — `:80-107`. No buttons, no images, no inline reply, no COM activator: "clicking a toast in the Action Center after NEXUS was restarted does nothing" (docs/changes/171, Toast diagnosis).
- `notify_activation.rs`: strict parser for `nexus://notify/open?event=ev_<24 lowercase hex>`; rejects anything else — `notify_activation.rs:8-20`. Wired at `lib.rs:466` (single-instance second launch) and `lib.rs:1033` (`deep_link().on_open_url`).
- `notify_open.rs`: `Opener::begin` token transaction (a later click supersedes an earlier one), `provider_url(&Target)` allowlist: GitHub only (`/pull/<n>`, `/actions/runs/<id>`, repo). **Gmail and deployments deliberately return `None`**: "Gmail needs the signed-in mailbox index and deployments need the provider's dashboard path... neither known safely here" — `notify_open.rs:64-79`.
- `notify_selftest.rs` + `nexus notify <scenario>` CLI (`nexus.mjs:645-690`): `pr-failed, build-failed, still-failing, recovered, expired-soon, same-name, ambiguous, open`.
- Frontend side of the hub: `frontend/src/stage/NotifyExpand.tsx` (full-screen expand overlay on `notify:expand`, fades on `notify:ready`), `frontend/src/sidebar/EventDetailView.tsx` (renders detail; "Open on GitHub" via `@tauri-apps/plugin-shell` `open` at line 120; calls `notify_event_ready(token)` on mount). Stage mounts `<NotifyExpand />` at `frontend/src/stage/main.tsx:377`.
- `work_area.rs`: `TOAST_W=364, TOAST_H=96, TOAST_MARGIN=12` (logical px) — the assumed size/position of the Windows toast (bottom-right of the work area) used as the start rectangle of the expand animation — `work_area.rs:41-44, 52-69`.
- Docs note drift: `docs/changes/171` N3/N4 text describes an in-stage `NotifyCard` ("compact top-right card") that was later REMOVED in the redesign section of the same doc; `frontend/src/stage` contains no NotifyCard (only `NotifyExpand`). Believe the code and the "Redesign" section, not the N3/N4 text.

#### 1c. Per-source tables (field available / where / gap)

**Gmail (important-mail watcher)** — `memcore/mailwatch.rs`, `mailtriage.rs`, `mailwording.rs`, `google_io.rs`
| Field | Available? | Where | Gap |
|---|---|---|---|
| Gmail message id | yes | `MailMeta.id` (`mailtriage.rs:23-33`), from `messages.get?format=metadata` (`google_io.rs:146-154`) | alert id is `mail_<id>` (`mailwatch.rs:245,270`), typed id not passed on |
| Gmail thread id | yes | `MailMeta.thread_id` | only used inside `notify::event_key`; not passed as a typed target |
| Account email | yes | `acct.email` / `acct.key()` in `poll_account` (`mailwatch.rs:373-387`, `google_io.rs:46-50`) | **dropped at `fire_alerts(app, &fresh)` (`mailwatch.rs:382`)** — the function takes only `&[(Triage, MailMeta)]`, no account. `StoredMail` does store `account` (`mailtriage.rs:445-456`) but `MailMeta` doesn't |
| Sender name/email | yes | `from_name`, `from_email` | — |
| Subject | yes | `MailMeta.subject` | — |
| Snippet | yes (Gmail's ~100-char snippet) | `MailMeta.snippet` | not passed; spoken text is a rewritten one-liner (`mailtriage::alert_text`, `mailwording::plain_alert_text`) |
| Labels, date | yes | `labels`, `date_ms` | — |
| Category/urgency/reason | yes | `Triage{category, urgency, reason}` (`mailtriage.rs:80-88`) | `reason` not surfaced |
| Web URL | **no** | not built anywhere | `provider_url` returns None for Gmail (`notify_open.rs:64-79`). Existing helpers only go the other way: `browser_url::extract_gmail_thread_id_from_url` (`browser_url.rs:713`) and `extract_gmail_account_index_from_url` (`browser_url.rs:737`) parse an already-open tab. The account email -> `/u/N` index mapping is not known to NEXUS; a Gmail URL would need `authuser=<email>` or `/u/<email>/` (UNVERIFIED here: no web access; see Gaps) |
- Submit sites: batch (>= 3 mails) `submit_event(.., "mail_batch_<firstid>", text, top, batch_key)` `mailwatch.rs:222`; single `submit_event(.., "mail_<id>", ..)` `mailwatch.rs:245` and `:270`. GitHub-shaped CI-failure mail is diverted to `notify_brain::observe_fact` -> `say("github")` (`mailwatch.rs:~238-262`) and then loses even the mail id (project parsed from the subject; `run_ref = m.id`, a Gmail message id not a run id, so `github_event` would reject it as a non-numeric run id — `mailwording.rs:79-94` + `notify_hub.rs:55`).
- Other mail submits: `mail_signin` (Medium, text only) `mailwatch.rs:412`. Mail e-mail in sealed store: `StoredMail` kept 7 days (`mailtriage.rs:441-456`).
- Mute: `mute_sender`, `last_alert_sender` (`mailwatch.rs:93,132`) — "that's not important" voice flow; a toast "Mute sender" action could reuse `mute_sender`.

**GitHub Actions watcher** — `memcore/github_watch.rs`
| Field | Available? | Where | Gap |
|---|---|---|---|
| owner/repo (`full_name`) | yes | `pick_repos` returns `(full_name, name)`; loop var `full_name` in `poll_once` (`github_watch.rs:~215-285`) | `facts_for_repo(&name, ...)` reduces to bare `name` (line 86-115), so `Fact.project` has no owner. Recovered at toast time by `github_repo_index::resolve_name` (single owner only; ambiguous -> no card) at `github_watch.rs:~405-410` |
| run id | yes | `Run.id: u64` (`github_watch.rs:20-26`) | becomes `Fact.run_ref` string |
| html_url, run name, head_branch, head_sha, display_title, actor, pull_requests[] | present in the REST JSON | not parsed — `Run` keeps only `id,event,conclusion,status,updated_at_ms` (`parse_runs`, lines 28-69) | extend `Run`/`Fact` (or add a side struct) |
| PR number | indirect | `coderabbit::pr_number_for_run(repo, run_id, token)` GETs `/repos/{repo}/actions/runs/{id}` -> `pull_requests/0/number` (`coderabbit.rs:641-645`); also in `notify_hub::resolve_pr_hint` at open time | one extra API call per open; `pull_requests` is already in the list response so could be read for free |
| token | yes | `github_cmd::get_github_token(worker_url, user_id)` (`github_watch.rs:~371`) | — |
- Submit sites: `note_auth_state` -> `submit("github_signin", .., Medium)` (`github_watch.rs:338`); real failures go `observe_fact -> notify_brain::say(&app,"github",line)` (`:~403-405`) and then `notify_hub::publish_github(&app, &entry.full_name, &fact, &line)` (`github_watch.rs:409`). Failed PR check also auto-opens CodeRabbit via `open_failed_pr_review` when `coderabbitAutoOpen` (default on) — `github_watch.rs:294-304, 410-420`. This is the ONLY producer wired to the typed ledger.
- Toast text today: title = `render_for("github", line, None)` e.g. "PR failed in zync"; preview = `"{full_name} · run {run_id}"` — `notify_hub.rs:80-83`. Provider URL available to the sidebar: `https://github.com/<owner>/<repo>/actions/runs/<run>` or `/pull/<n>`.

**Deploy watchers (Vercel / Render / Netlify / Supabase)** — `memcore/deploy_watch.rs`
| Field | Available? | Where | Gap |
|---|---|---|---|
| project name | yes | `DeployRun.project` (`deploy_watch.rs:19-25`) | — |
| deployment id | yes | `DeployRun.id` -> `Fact.run_ref` (lines 337-364) | — |
| time, state | yes | `at_ms`, `obs` | — |
| Vercel team id | yes at poll time | `parse_teams_vercel` and per-team endpoint loop (`deploy_watch.rs:53-71, ~430-440`) | not stored on `DeployRun` |
| Vercel `url` / `inspectorUrl` / `meta.githubCommitRef` | present in API JSON | `parse_vercel` (`:73-130`) reads only `uid/id, name/project, target, created, state/readyState` | not parsed; would give a direct dashboard link |
| Render service id / dashboard URL, Netlify site id / `admin_url`/`deploy_ssl_url`, Supabase project ref | present in API JSON | `parse_services_render/parse_render/parse_sites_netlify/parse_netlify/parse_supabase` (lines 132-336) keep `(id, name)` pairs | URL fields not parsed |
- Submit: success path calls only `crate::notify_brain::say(&app, service, line)` (`deploy_watch.rs:~771-775`) — it never calls any `notify_hub::publish_*`. `Target::Deployment` and `Provider::{Vercel,Render,Netlify,Supabase}` exist in `notify_event.rs` but have no producer. Sign-in notices: `submit("deploy_signin_<svc>", .., Medium)` `deploy_watch.rs:687`. Intervals 5 min / backoff to 30 min (`next_delay_secs`, line ~370).

**WhatsApp (priority people, OFF by default)** — `memcore/wa.rs`, `people.rs`
| Field | Available? | Where | Gap |
|---|---|---|---|
| chat jid | yes | `WaAlert.jid`, `ChatInfo.jid` (`wa.rs:50-57, 291-298`) | only in the id (`wa_<jid>_<stamp>`) and key (`wa:<jid>`) — `wa.rs:690-703` |
| contact name | yes | `WaAlert.name` | folded into spoken text |
| score/why/gist | yes | `WaAlert.score/why/gist` | gist = message text; privacy: only sender named in alerts by design (docs, Memory Core P6) |
| deep link | existing | `whatsapp://send?phone=` used by `wa_open.rs` / `ghost_drill.rs` when a number is stored (`remember that wa phone <name> is <number>`); core fact `wa_phone_<name>` | no jid->phone mapping in the alert; groups ignored |
- Submit: `fire()` -> `submit_event(app, id, text, urgency, Some("wa:<jid>"))` `wa.rs:690-703`. Settings `memcoreWhatsapp` and `memcoreWhatsappSpeak` default false (`commands.rs:2006,2373`).

**Coding agents needing input / finishing** — `agent_attention.rs`, `agent_hooks.rs`, `agent_watch.rs`, `watch_runner.rs`
| Field | Available? | Where | Gap |
|---|---|---|---|
| agent name/label | yes | `WaitObs{name,label,waiting_since_ms}` (`agent_attention.rs:~38-45`), `Agent::label()` | — |
| cwd of the agent | yes | `ParsedEvent.cwd`, `agent_hooks::last_cwd(agent)` (`agent_hooks.rs:1142-1150, 1304-1315`) | not attached to alerts |
| last-message tail (masked 4 KB) | yes for Stop | `StopInfo`, `record_event_at(.., tail, info)` (`agent_hooks.rs:173, 1417`) | — |
| needs-input timestamp | yes | `agent_hooks::needs_user(agent, since)` / `last_event_ms(agent, Notification)` (`agent_hooks.rs:1449,1503`) | — |
- IMPORTANT: a "needs your input" wait is NOT submitted to `proactive_policy` while the user is present: `agent_attention::start` only calls `remember(...)` into the away log and speaks the summary on return as a single High `submit("agent_away_<ts>", line)` (`agent_attention.rs:~187-220`). Agent-watch completion/error/login alerts are High `submit` calls with text only (`agent_watch.rs:440-469, 569`); watch_runner follow-ups High (`watch_runner.rs:825,915`). No window handle / PID reaches the alert; "open the agent window" would need a new focus helper (existing `live::commands::window::wait_and_focus_app` / `focus_app_by_title`).

**Calendar / reminders / leave-by / scheduler**
| Field | Available? | Where | Gap |
|---|---|---|---|
| event id, summary, start/end ISO, location, status | yes | `CalendarEvent` (`google/types.rs:149-157`) | `htmlLink` is NOT in the struct (grep `htmlLink` = 0 hits) so no calendar URL |
| leave-by reminder | `submit(app, "leaveby_<key>", text, urgency)` | `memcore/leavewatch.rs:286` | event id dropped; urgency from `due_reminders` |
| timetable slot reminder | `submit("slot_<id>" | "offer_slot_<id>")` Medium; DSA slots arm `Offer::Start` ("Shall I start?") | `memcore/scheduler.rs:45-70` | the offer is only armed after the line is spoken (`speak_line` arms `sentinel_offer_*`); a silent toast would not arm it — Yes/No toast buttons would need a new path |
| lease expiry, routines offer, review suggestion, briefing | text-only `submit` calls | `memcore/leases.rs:154`, `routines.rs:424`, `review.rs:419`, `briefing.rs:659,807,842`, `return_review.rs:347`, `ai_review.rs:478`, `coderabbit_review.rs:490`, `orchestrator.rs:9316` (`speak_proactive_alert`) | no typed ids |

**Full list of `proactive_policy::submit*` callers** (grep, outside proactive_policy.rs): agent_attention:218; agent_watch:440,452,462,569; ai_review:478; coderabbit_review:490; memcore/briefing:659,807,842; deploy_watch:687; github_watch:338; leases:154; leavewatch:286; mailwatch:222,245,270,412; review:419; routines:424; scheduler:45,52,70; wa:703; notify_brain:429; orchestrator:9316; return_review:347; watch_runner:825,894,915,921.

### Inferences
- The cheapest design: keep `proactive_policy` as the "should the user be interrupted now" brain and add a second, typed side-channel carrying `{provider, target ids, title, preview, actions}` (generalise `notify_hub` rather than create another ledger). Mail and deploys already have `Provider`/`Target` variants waiting.
- `proactive:card` currently carries too little (`id`, `reason`) to render anything; a toast needs the text and target, so either enrich that event (and add a listener — it would remove two baseline lines from `events-unmatched.txt`, which the ratchet allows) or deliver through the hub.
- The GitHub-mail path is a trap: GitHub CI-failure mail goes to `notify_brain` with `run_ref = Gmail message id`, so it can never create a typed GitHub event (non-numeric run id). A toast design should treat "GitHub mail" as a Gmail event or resolve the run via the API; otherwise duplicates appear (watcher + mail both talk about the same failure; they collapse in `notify_brain` by entity id, which is good).
- Quiet-baseline semantics (first sight after connect/start is silent) live in `notify_brain::observe_fact(.., baseline)` and the Gmail/WA first-poll baselines; a toast sitting on top inherits them for free as long as it hangs off `Decision::Say` and the same triage output.

### Gaps
- Exact Gmail web URL form that opens a given API thread id under a specific account (UNVERIFIED; web research colleagues cover this). Local code only proves NEXUS has no builder today.
- Whether Render/Netlify/Supabase list responses include a stable dashboard URL field — local code does not parse any; fixtures in `deploy_watch.rs` tests were not inspected.
- WhatsApp jid -> deep link: the bridge's message ids are not mapped to a `whatsapp://` target in the alert path (only phone-number-based open exists).
- `memcore/agenda.rs` (calendar read-back/insert) was not read in detail; it does not call `submit` (not in the grep list), so calendar alerts only come via `leavewatch` and `scheduler`.

---

## 2. Window system: recipe for a new toast window

### Takeaway
New webview windows are declared as `WindowConfig` constructors in `dyn_windows.rs`, created on demand via `get_or_create_window`, and registered in a vite input + a capabilities JSON + (if they use panel CSS) the opacity registry. Panel windows are transparent frameless always-on-top with solid opaque card CSS (no `backdrop-filter`, no DWM blur). A toast can alternatively live INSIDE the always-on `stage` overlay using the named hitbox mechanism. There is also a precedent for a fully native (non-WebView) Win32 layered window (`early_orb.rs`).

### Cited Findings
- `WindowConfig` fields: label, title, url, width, height, min sizes, resizable, decorations, transparent, always_on_top, skip_taskbar, shadow, focus, center, hidden_title — `dyn_windows.rs:16-35`. Existing constructors: `setup, settings, sidebar, architect_map, maps_panel, place_card, stage, calibrate_toolbar, hand, hand_hidden, orb_studio` (`dyn_windows.rs:39-171`).
- Closest analogues for a toast: `place_card()` (315x950, `transparent:true, decorations:false, always_on_top:true, skip_taskbar:true, shadow:false, focus:false`, `url sidebar.html?pin=place`) — `dyn_windows.rs:~91-101`; `hand_hidden()` (8x8 click-through transparent page) — `:~152-163`.
- `get_or_create_window`: builds hidden (`.visible(false)`), `inner_size(config.width, config.height)`, caller positions and shows; for labels in `is_glass_window` (`"sidebar" | "architect-map" | "maps-panel" | "place-card" | "calibrate-toolbar" | "settings" | "setup" | "orb-studio"`) it applies `dwm_corners::round_corners` and `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE=17)`; all other labels log "live_glass: skipped" — `dyn_windows.rs:~173-262`. A new toast label must be added to that `matches!` to get rounded corners and capture exclusion (screen-share privacy for a toast carrying mail text).
- ADR-05 rule (comment `dyn_windows.rs:~212-222`): never apply DWM backdrop materials (Acrylic/Mica/blur-behind) to these windows — non-activating windows get a solid opaque fallback and the material call overrides tao transparency, producing the 2026-10-01 black-screen. Blur, where used, is a pre-show screenshot (`sidebar_backdrop.rs`); currently the live blur is disabled (`commands.rs:~1050 DEV_LIVE_BLUR_DISABLED = true`) and panel cards are SOLID.
- `destroy_window` destroys the WebView2 process tree to save RAM (~250 MB per window), except sidebar which is hidden under `DEV_KEEP_WINDOWS_ALIVE = true` — `dyn_windows.rs:~264-285`. A toast window should be created on first alert and destroyed after a linger (RAM budget 600-700 MB, AGENTS.md).
- Registration checklist for a new window label (from AGENTS.md "Frontend windows", code, and the comment in `frontend/vite.config.ts:9-23`): (1) `WindowConfig::toast()` + optional `is_glass_window` entry; (2) a rollup `input` in `frontend/vite.config.ts` (list at lines 61-72: setup, settings, sidebar, stage, companionHud, orbStudio, hand, githubDashboard) — missing input = blank window only in release; (3) an HTML entry in `frontend/` (siblings: `hand.html`, `companion-hud.html`, `sidebar.html`, `stage.html`); (4) a capability file in `src-tauri/capabilities/` whose `windows` array includes the label (existing: `hand-cap.json` `["hand"]`, `sidebar-cap.json` `["sidebar","architect-map","maps-panel","place-card"]`, `stage-cap.json`, `settings-sidebar-cap.json`, `main.json`, `global-shortcut.json`); minimal set used by hand-cap: `core:default` + `core:event:allow-emit/listen/unlisten`; sidebar-cap adds `core:window:allow-start-dragging` and `shell:allow-open`; (5) `tauri.conf.json` has `"windows": []` (`tauri.conf.json:14`), so nothing to add there; (6) `stage` special: CSP drops React inline `style` props (nonce makes 'unsafe-inline' inert) — position with CSSOM (`placeAt`, `frontend/src/stage/main.tsx:32-50`; same note in OrbFrame).
- Sidebar geometry: `commands::sidebar_geometry(view, dock, monitor_w, monitor_h)` (pure, `commands.rs:961-1037`): right dock x = `monitor_w - w - 10`, y = 20, left dock x = 10; heights `min(monitor_h-40, 1040).max(400)`; widths: settings 400, pr-list/github-dashboard 400, architect/maps 960, coderabbit 595, annotate 320x380, assistant default 520. `work_area::sidebar_rect(view, dock, work: PRect, scale)` applies it inside the monitor WORK AREA in physical px with clamping (`work_area.rs:19-27`, tests in the same file). `prepare_sidebar` (`commands.rs:1152-1184`) reads `Monitor::work_area` and sets physical size/position; `finish_sidebar` (`:1188-1215`) shows, runs `panel_layout::apply`, then **`set_focus()` and `SetForegroundWindow`** — i.e. the unified sidebar is NOT actually non-activating today (AGENTS.md describes a non-activating sidebar; no `WS_EX_NOACTIVATE` appears anywhere in src: grep NOACTIVATE only hits `early_orb.rs`, `window_tile.rs`, `reverse_engineer.rs`, `stage.rs` as `SWP_NOACTIVATE/SW_SHOWNOACTIVATE` flags). `WindowConfig::sidebar()` has `focus:true` (`dyn_windows.rs:~75-85`); `place_card/maps_panel/architect_map` use `focus:false`.
- A toast window MUST NOT steal focus. Tauri's `focused(false)` + `.always_on_top(true)` + `.skip_taskbar(true)` is what `place_card` uses; `window.show()` on Windows may still activate unless `WS_EX_NOACTIVATE` is added (code precedent for calling `ShowWindow(.., SW_SHOWNOACTIVATE)` on a raw HWND: `early_orb.rs:~881-882`, `window_tile.rs:~929-971`). No existing helper sets `WS_EX_NOACTIVATE` / `WS_EX_TOOLWINDOW` on a Tauri window except the stage (`WS_EX_TOOLWINDOW`, `stage.rs`).
- Stage overlay as host: `stage` is a fullscreen (primary monitor) transparent always-on-top overlay, created at startup, click-through by default (`ensure_stage` sets `set_ignore_cursor_events(true)`, `stage.rs:~125-132`), `WS_EX_TOOLWINDOW`, not capture-excluded. Interactive regions are punched through by named hitboxes: `stage::set_hitbox_source(source, Vec<StageRect>)` (Rust, `stage.rs:93-100`) or IPC `stage_set_hitboxes(source, rects)` (physical px, `stage.rs:271-276`); a 30 ms cursor poll toggles `set_ignore_cursor_events` around the rects (`spawn_hitbox_loop`, `stage.rs:412`). Existing sources: orb, loading, spatial, tour, legacy, live-glass. Everything stage-hosted is solid and registered in `scripts/window-opacity-registry.json` (e.g. `".nx-expand"` under `frontend/src/stage/notify-expand.css`). Events to the stage use `app.emit_to("stage", ...)`. Stage health: `stage::is_shown()` and `is_ready()` (heartbeat <= `HEARTBEAT_STALE_SECS`) — `stage.rs:291-300`; `notify_hub::activate` already checks these before animating. Downside noted in docs: stage is primary-monitor only and was killed by the kill-switch `Ctrl+Alt+X`; if stage is down an in-stage toast silently disappears (the native toast does not have this weakness).
- `window_manager.rs` (358 lines) = orb/loading placement for the stage (`emit_orb_rect`, `emit_loading_rect`, `set_hitbox_source("orb"...)`); `panel_layout.rs` (284 lines) arranges Architect/Maps/MapsWeb/PlaceCard beside the sidebar using MARGIN=10, GAP=10, TOP=20 (`panel_layout.rs:13-19`) and `Panel::{label,from_label}` — a toast does not need to participate, but a toast near the right edge overlaps the right-docked sidebar (x = monitor_w - 520 - 10) so z-order/positioning rules are needed.
- Native-window precedent: `early_orb.rs` draws a capsule + caption as two layered, click-through, never-focus Win32 windows with per-pixel alpha before the WebView exists ("pure and tested geometry; Windows glue at the bottom", `early_orb.rs:1-12`). Toast cards could use the same idea for a zero-RAM, WebView-independent toast, at the cost of hand-rasterised text.
- Stage layout slots: orb capsule top-centre (301x105 pebble / 200x200 classic); caption pill at x=51.5%, top=89.5% (`early_orb.rs:19-26`, `frontend/src/stage/layoutSlots.ts`). Windows-native toast sits bottom-right above the taskbar (`work_area.rs` constants). A custom toast must avoid the caption slot (bottom-centre) and the sidebar (right edge).
- Theming: panel CSS must use tokens (`frontend/src/theme/tokens.css` is the only file allowed hard-coded colours; gate `scripts/check-hardcoded-colors.mjs` ratchets per-stylesheet counts, a new stylesheet starts at 0); `check-theme-contrast.mjs` also runs in CI. Theme sync via `initThemeSync()` (`frontend/src/stage/main.tsx:17`).

### Inferences
- Two viable hosts: (A) Windows-native toast (exists; survives stage crashes; Action Center history; no DND bypass unless `scenario="urgent"` allowed; limited UI: two text lines today, buttons/images need extra XML) or (B) a custom frameless WebView toast window or stage-hosted card (full design control, but must solve no-focus-steal, multi-monitor, taskbar position, RAM, CSP-inline-style, opacity gate). The owner's explicit instruction (docs/changes/171) favours A, so a design that removes it should be a deliberate owner decision.
- If a custom window is built, copy `place_card()` for the config, add its label to `is_glass_window`, to `sidebar-cap.json` (or a new `toast-cap.json`) and `vite.config.ts` input, and prefer a separate tiny page over `sidebar.html?pin=` (which loads the whole unified sidebar bundle).
- To keep it from stealing focus the implementation will need raw-HWND `WS_EX_NOACTIVATE` (+ `WS_EX_TOOLWINDOW` to stay out of Alt-Tab), applied once after creation; there is precedent for HWND access via `win.hwnd()` in `dyn_windows.rs` (SetWindowDisplayAffinity block).

### Gaps
- Whether `window_manager.rs` has any multi-monitor logic beyond the primary: the stage is primary-only (`stage_target_rect` takes the primary monitor); `prepare_sidebar` uses `current_monitor()` of the sidebar window, "not necessarily where the owner is working" (docs/changes/171 N4 notes).
- Not inspected: `sidebar_backdrop.rs` capture internals, `dwm_corners.rs`, whether `WDA_EXCLUDEFROMCAPTURE` is desirable for a toast (meeting screen-share privacy argues yes).

---

## 3. Open / hand-off code that already exists

### Takeaway
There are two proven "open this in the right place" paths: `open::that(url)` behind an `is_safe_web_url` check (`command_executor.rs:393-416`) and the `@tauri-apps/plugin-shell` `open()` used from the sidebar frontend. There is no Gmail/Calendar/deploy URL builder, no generic `open_url` IPC, and no per-account browser profile handling.

### Cited Findings
- `command_executor::open_url(target, url)`: allows only `http(s)://` URLs with no control/whitespace chars (`is_safe_web_url`, `command_executor.rs:385-391`), then `open::that(url)` (`:393-416`). The `open` crate is the default-browser opener; `open_search` uses the same call (`:419-440`). Cargo has `tauri-plugin-shell = "2"` (`Cargo.toml:37`) and `tauri-plugin-deep-link = "2"` (`:39`); **no `tauri-plugin-notification` and no `tauri-plugin-opener`** (also stated in docs/changes/171 N0).
- Frontend open path: `import { open as openExternal } from "@tauri-apps/plugin-shell"` (`frontend/src/sidebar/EventDetailView.tsx:3`); the sidebar capability grants `shell:allow-open` (`sidebar-cap.json`). `SettingsSidebarApp.tsx:1654` defines a local `openExternal(url)` for the Hub.
- Foreground-browser-aware navigation: `live/commands/browser.rs` search opens `https://www.google.com/search?q=` in the foreground browser's own exe via `browser_url::foreground_process_path` (AGENTS.md "Search by URL"); `browser_url.rs` can read the active tab URL (UI Automation) and window title, handles Chromium schemeless URLs, and has `extract_gmail_thread_id_from_url` (`browser_url.rs:713`) and `extract_gmail_account_index_from_url` (`:737`, `/mail/u/<N>/`).
- Gmail integration in `app_registry.rs:2180`: `("Gmail", "https://mail.google.com", ["gmail","google mail"])` — site-open by name only.
- App launch: `app_registry::launch(&AppEntry)` (`app_registry.rs:741`), `resolve_and_open_app` with 8 s `RECENT_LAUNCH` debounce, `try_focus_existing` live window scan; `live::commands::window::wait_and_focus_app` (150 ms polling up to 2.5 s with Alt-tap to beat foreground lock). WhatsApp: `whatsapp://send?phone=` direct open in `wa_open.rs`/`ghost_drill.rs`.
- GitHub dashboard: `OpenGitHubDashboard` intent / `github-dashboard.html` window; CodeRabbit panel opener `commands::show_coderabbit_pr(app, repo, pr)` (`commands.rs:1410-1435`); event-detail opener `commands::show_event_detail(app, payload)` (`commands.rs:1385-1407`) which uses `prepare_sidebar -> PENDING_SIDEBAR -> finish_sidebar` (cold-mount race-free pattern: store payload in a static Mutex, frontend pulls it on mount; warm path emits `sidebar:analysis`).
- Deep links: scheme `nexus://`; `nexus://notify/open?event=<id>` is the only notification route (`notify_activation.rs`); OAuth uses `nexus://oauth/...` (the single-instance handler logs without printing the URL because it can contain an OAuth code, `lib.rs` near 455-470).
- Install detection: no explicit "is Gmail/Slack app installed" helper found; the app registry (`app_registry.rs`, `cached_app_count()`) resolves installed apps by name; Spotify/Apple Music deep links by protocol (`spotify:`, `applemusic:` in `media_player.rs`). Not applicable to Gmail (web only).
- Safety rules already encoded for opens: provider URLs are derived from typed targets, never from notification text; Gmail/deployment URLs intentionally unbuilt (`notify_open.rs:64-79`). Sidebar-side open is read-only and "marks only NEXUS's own alert Seen" (`notify_open.rs` header).

### Inferences
- For "click toast -> open the mail": Rust-side, resolve event id in the ledger -> build a Gmail URL from `Target::Gmail{thread_id}` + the stored account email -> `is_safe_web_url` -> `open::that`. Doing it in Rust (not the sidebar's shell plugin) avoids needing the sidebar open at all and keeps the allowlist in one place.
- A hybrid for Gmail: NEXUS knows the account email (vault) but not the browser's `/u/N` slot; `authuser=<email>` or `/u/<email>/` style (to verify) avoids the index problem.

### Gaps
- No way to know which browser profile is signed into which Google account; a wrong profile opens a login/chooser page.
- No existing "bring existing Gmail tab to front" logic (only URL parsing of the foreground tab).

---

## 4. Existing settings, switches and presence signals

### Takeaway
Settings are fields on `NexusSettings` (`commands.rs`) persisted in `settings.json`; notification-relevant flags already exist but most have no Hub switch. Presence/DND signals exist for meeting, focus (Hand Mode), user speaking, NEXUS speaking, drill; none read Windows Focus Assist / Do Not Disturb.

### Cited Findings
- Notification-related settings in `NexusSettings`: `notify_native` (default true, `commands.rs:1991, 2369`), `notify_urgent` (default true, `:1994, 2370`), `coderabbit_auto_open` (`:1988`), `coderabbit_auto_review` (default false, `:1997`), `memcore_mail` (`:1979`), `memcore_github_watch` (`:1982`), `memcore_deploy_watch` (`:1985`), `memcore_whatsapp` (default false, `:2006`), `hand_focus_mode` (`:1866`, default true), plus read directly from settings.json: `proactiveCriticalInMeeting` (`proactive_policy.rs:423-434`), `memcoreMailPlain` (cloud rewrite of mail wording, default OFF), `memcoreWhatsappSpeak`, `memcoreLeaveBy`, `memcoreTimetable`, `memcoreRoutines`, `memcoreReview`, `watchPower`. New settings MUST be added to `NexusSettings` or `save_settings` drops them (AGENTS.md pitfall, repeated many times).
- Hub UI: `frontend/src/settings-sidebar/hub/MemoryPage.tsx` has switches for `memcoreMail` (line 404) and `memcoreMailPlain` (414) via `setFlag`; `grep` finds NO frontend reference to `notifyNative` or `notifyUrgent` — they are settings.json-only today ("there is no Hub switch yet" for the card; `notifyNative` default flipped to ON in the redesign).
- Presence/DND signals the toast must respect: `MeetingState` (`meeting_detect.rs:55-...`: `manual_pause` from tray "Pause NEXUS", `meeting_active`, `should_suppress_tts()` line 123, `allow_tts_override(window_ms)` for Critical); `focus_mode::paused()` (`focus_mode.rs:157`, lease with 10 s watchdog; true while Hand Mode runs) with `defer_log(name)` / `wait_until_clear_sync(name)` helpers (`:200,224`) — watchers `github_watch` and `deploy_watch` skip polling while paused; `wakeword_oww::stt_user_speaking()`; `crate::orchestrator::has_active_request()`; `crate::ghost::drill_running()`; `memcore::resume::user_idle_secs()`; exclusive-fullscreen auto-pause in Ghost (doc 41). Snooze: `proactive_snooze(minutes)` (default 15, clamp 1-240) — no UI.
- Windows Focus Assist / Do Not Disturb: `notify_hub` handles it only by asking Windows to allow `scenario="urgent"` toasts; `grep -i "dnd|do not disturb|focus assist"` in src hits `commands.rs`, `notify_hub.rs`, `notify_native.rs` only (no query of the OS DND state). Windows decides whether a banner shows; NEXUS "cannot grant itself the permission" (`notify_native.rs:46-49`).
- Privacy hooks already modelled: `PrivacyClass::Redacted` drops preview (`notify_event.rs:381`) but `github_event` always uses `Public`; `memcoreRedactNames` redacts names to cloud only; mail text is "untrusted" data in the sealed store; `memcoreMailPlain` must stay off by default (cloud). Toast text for mail would contain subject/sender — whether to show it during a meeting/screen share is not decided in code (native toast is shown by Windows regardless of NEXUS' meeting flag; `publish_github_with` does not consult `MeetingState`).
- Persona/address: toast lines use `persona::address` ("sir"/name) for spoken text via `notify_brain::say`; the toast title is rendered with `address=None` (`notify_hub.rs:81`), i.e. no "sir".

### Inferences
- The new toast should add its own flag(s) (e.g. a per-provider set) in `NexusSettings` with serde defaults, and a Hub switch (either in `MemoryPage.tsx`'s notification section or the Connections tab) — otherwise it is a settings.json-only feature again.
- A presence rule set for the toast can reuse `Signals` from `proactive_policy::collect_signals` (it is private; expose or duplicate) and add: hide/queue during `meeting` unless Critical; hold during `focus`; stack/merge by event key (the existing 5 min/2 min cooldowns already coalesce).

### Gaps
- Windows Focus Assist/DND state is not read anywhere; whether to read it (WNF/`SHQueryUserNotificationState` for presentation/fullscreen) is a design choice not covered by existing code. `SHQueryUserNotificationState` was not found in the repo.
- No central "do not disturb until X" user setting besides `proactive_snooze` (no UI/voice phrase found in this survey).

---

## 5. Quality gates a new feature must pass

### Takeaway
CI (`.github/workflows/ci.yml`) and the repo scripts enforce: TypeScript + vitest, turn-end gate, repo hygiene, theme contrast, hard-coded colours ratchet, IPC contract, Rust check (`--features mock-wake`, `--all-targets custom-protocol,admin-brain`), dead-code ratchet, `cargo test`. Window-opacity and work-card gates are repo scripts documented in AGENTS.md. All ratchets only shrink; new violations fail.

### Cited Findings
- CI jobs (frontend): `npx tsc --noEmit`, `npm run build`, `npm test`, `node ../scripts/check-turn-ends.mjs`, `check-repo-hygiene.mjs --selftest` and plain, `check-theme-contrast.mjs`, `check-hardcoded-colors.mjs`, `check-ipc-contract.mjs` — `.github/workflows/ci.yml:28-56`. Rust: `cargo check`, `cargo check --features mock-wake`, `cargo check --all-targets --features custom-protocol,admin-brain`, `node scripts/dead-code-audit.mjs`, `cargo test --tests --features custom-protocol,admin-brain -- --test-threads=1` — `ci.yml:196-218`. Also Python tests, Worker tests, `wake_data_foundation.py validate`.
- IPC gate (`scripts/check-ipc-contract.mjs`): every `#[tauri::command]` must be in `generate_handler!` (`lib.rs`, e.g. notify commands at `lib.rs:1219-1220`); every frontend `invoke` must be registered; registered-but-never-invoked commands must appear in `scripts/baselines/ipc-unused-commands.txt`; events emitted-but-not-heard or heard-but-not-emitted are checked against `scripts/baselines/events-unmatched.txt` — baselines may only shrink. `proactive:card`/`proactive:nudge` currently sit in that baseline; wiring a listener should remove them (`--update` only after real wiring; running `--update` while a NEW item shows defeats the ratchet).
- Dead-code ratchet (`scripts/dead-code-audit.mjs`): rustc `--force-warn dead_code`; new dead item fails; baseline `scripts/baselines/dead-code.txt` (AGENTS.md: 145 known, none new as of 2026-10-29/11-04 notes); `lib.rs` uses `pub(crate) mod` except modules listed in `external-api.txt`; never add `#[allow(dead_code)]` (use `#[expect]` with a reason); a name starting `_` hides from the lint (do not use to dodge). `notify_*` modules are `pub(crate) mod` (`lib.rs:26-37`). Test-only pieces are `cfg(test)`-gated (e.g. `Ledger::len`, `Opener::take_pending`).
- Opacity gate (`scripts/check-window-opacity.mjs`, registry `scripts/window-opacity-registry.json`): every registered panel/surface selector must be solid alpha 1 with no `backdrop-filter`/blurred image layers; a new window surface class must be added to the registry (AGENTS.md pitfall). Registry today covers sidebar cards, `.nx-expand` (notify-expand.css), settings, hub `.hx-card`, `.crp`, `.architect-app`, `.gh-dashboard-shell`, `.hud`, etc. A glassy/blurred toast would FAIL by design (owner's 2026-10-15 liquid-glass directive was superseded for panels by the solid-windows guard).
- Hard-coded colours ratchet: component CSS must use `var(--ui-*)`/`var(--stage-*)` from `frontend/src/theme/tokens.css`; baseline `scripts/baselines/hardcoded-colors.json`; a new stylesheet starts at zero (`check-hardcoded-colors.mjs:1-14`). Contrast gate `check-theme-contrast.mjs` (Light and Dark).
- Turn-end gate (`scripts/check-turn-ends.mjs`): requires every `store.reset()`-style turn-end site to be marked or use the helper (`turn-end:keep-raw <reason>` to exempt); relevant only if the toast touches the voice turn state machine (e.g. a Yes/No offer reply).
- Hygiene gate (`scripts/check-repo-hygiene.mjs`): repo hygiene (no tracked secrets/db/build artefacts etc. — details not re-read here); also whitespace/`git diff --check` is used in the project's own verification lists.
- Work-card drift: `scripts/make-work-cards.mjs --check` fails if `docs/testing/work-cards/*` drift (68 cards as of AGENTS.md); new queue items in `scorecard-data.json` require regeneration.
- Other repeated project rules for any new feature: new settings in `NexusSettings`; any `Tauri` command that creates windows must be `async` (WebviewWindowBuilder::build deadlock otherwise, AGENTS.md "Dynamic window pending-content pattern"); never hold a mutex across `.await` (store lock / `SESSION` lock rule); cold-mount payloads via `PENDING_*` static + `get_pending_*` command; Windows-only code must stay behind `cfg(windows)` (see `notify_native.rs` stubs) because CI also builds mock/linux-ish checks; owner controls builds/runtime ("edits only", AGENTS.md top) — the implementation should not start/stop/rebuild NEXUS.
- Doc discipline: each change has a record in `docs/changes/NNN-*.md` and an AGENTS.md entry; latest numbers seen: 185 (`185-early-hello-when-the-desktop-is-shown.md`), duplicates exist at 182.

### Inferences
- Adding a Rust command + frontend page implies: command in `generate_handler!`, an `invoke` caller (or an `ipc-unused-commands.txt` line), listeners for every emitted event (or baseline), a vite input, capability JSON, opacity registry line, token-only CSS, tests (the project mutation-checks gates and logic), and an AGENTS.md/docs/changes entry.
- If the toast stays native (Windows), most frontend gates are untouched; the Rust-side ratchet (dead code, IPC) is the main constraint.

### Gaps
- `check-repo-hygiene.mjs` rules were not read line by line; the exact forbidden patterns are unknown here.
- The exact current dead-code/IPC baseline counts were not re-run (AGENTS.md cites 145 / IPC 241-242 commands as of late Oct).

---

## 6. Exact files to change (for a toast extended to mail, deploys, WhatsApp, agents, calendar)

Rust (src-tauri/src):
1. `notify_event.rs` — add `Provider::WhatsApp`/`Calendar`/`Agent` and `Target::{WhatsApp{jid}, Calendar{event_id,...}, Agent{name,cwd}}` if those get typed targets; Gmail `Target::Gmail{thread_id,message_id}` already exists but needs an `account` binding (today `account_id` is the only identity field; `entity_key` includes it). Keep `deny_unknown_fields` and `validate_event` rules in sync.
2. `notify_open.rs` — `provider_url` for Gmail (needs account email -> URL scheme) and deployments (needs dashboard URL; derive from typed fields, never from text); keep the allowlist test style.
3. `notify_hub.rs` — add `publish_gmail`, `publish_deploy`, ... next to `publish_github`; generalise `detail_json/target_rows` (Gmail and Deployment arms already exist in `target_rows`); decide channel (native toast vs custom window) in `publish_*_with` (today `channel_for(toast_on)` only returns Toast/HistoryOnly); add meeting/focus/privacy checks (nothing consults `MeetingState` today).
4. `notify_native.rs` — if staying native: richer XML (buttons via `<actions>` with `activationType="foreground"` and an arguments allowlist; keep the "no URL in the toast" invariant or revise it deliberately; update `notify_activation.rs` parser which accepts exactly one shape `nexus://notify/open?event=`; e.g. add an action verb) .
5. `memcore/mailwatch.rs` — thread the account email into `fire_alerts` (signature `fire_alerts(app, items)` called from `poll_account` at line 382 which has `acct`) and publish a Gmail event with `MailMeta.thread_id`, `id`, sender, subject, snippet; also decide handling of the GitHub-mail diversion (`mailwatch.rs:~238-262`).
6. `memcore/github_watch.rs` — optionally parse `html_url`, `head_branch`, `pull_requests[0].number`, `display_title` in `Run`/`parse_runs` (lines 20-69) to skip the extra API call and enrich the preview; keep `full_name` with the Fact instead of re-resolving.
7. `memcore/deploy_watch.rs` — parse dashboard/inspector URL, team id; add `publish_deploy` call next to `notify_brain::say` (`:~771`).
8. `memcore/wa.rs` — `fire()` (`:690-703`) publishes a WhatsApp event carrying the jid; text must remain sender-only unless `memcoreWhatsappSpeak`.
9. `agent_attention.rs` / `agent_watch.rs` / `watch_runner.rs` — optional typed events for "needs input"/"finished" with agent label + cwd.
10. `proactive_policy.rs` — either feed the toast from `Action::Card` (enrich `proactive:card` payload with text + optional event id) or leave and call hub directly; expose `collect_signals` (or a `Signals` getter) so the hub can respect meeting/focus/user_speaking; keep tests (timelines) in sync.
11. `dyn_windows.rs` — `WindowConfig::toast()` + `is_glass_window` entry (only if a custom window is chosen).
12. `commands.rs` — new settings fields in `NexusSettings` (+ `Default` + serde), maybe a position/helper next to `sidebar_geometry`; `work_area.rs` — toast rect helper (it already has `TOAST_W/H/MARGIN`).
13. `lib.rs` — `pub(crate) mod` line, `generate_handler!` entries for any new commands, init/start calls.
14. `stage.rs` — only if the toast lives in the stage (hitbox source name) .

Frontend / config:
15. `frontend/vite.config.ts` input + new `frontend/toast.html` + `frontend/src/toast/*` (custom window route) OR `frontend/src/stage/NotifyToast.tsx` + css (stage route; mount in `main.tsx` near line 377; register selectors in `scripts/window-opacity-registry.json`).
16. `src-tauri/capabilities/*.json` — new label in a capability file.
17. `frontend/src/settings-sidebar/hub/*` — Hub switches (MemoryPage.tsx uses `setFlag`).
18. `frontend/src/sidebar/EventDetailView.tsx` and `commands::show_event_detail` — detail view for mail/deploy events (today GitHub-shaped: "repo/run").
19. `scripts/baselines/events-unmatched.txt`, `ipc-unused-commands.txt`, `dead-code.txt` — shrink only after real wiring; `nexus.mjs` + `notify_selftest.rs` scenarios (a Rust test asserts every scenario name appears in `nexus.mjs`); docs: `docs/changes/NNN-*.md`, AGENTS.md entry.

---

## 7. Risks (code-evidenced)

- **Owner-decision conflict**: the owner rejected an in-app notification card and asked for the OS notification centre + a full-screen expand animation (docs/changes/171 "Redesign after the owner's first live look"). A custom toast window re-opens that decision.
- **Native toast unverified on device**: toast display/click depend on Windows notification settings, Focus Assist, AUMID registration; the diagnosis log shows the app had silent runs (state machine) and no confirmed visible banner (docs/changes/171 "Toast diagnosis"). Click handler lives in-process; Action Center clicks after restart do nothing (no COM activator).
- **Events are dropped when the stage is not ready**: `activate` only animates if `stage::is_shown() && is_ready()`; otherwise it just opens the sidebar. An in-stage toast has the same dependency and a kill-switch (`Ctrl+Alt+X`) and watchdog rebuilds (blackout history).
- **Focus stealing**: unified sidebar calls `set_focus` + `SetForegroundWindow` in `finish_sidebar`; copying that for a toast would pull the user out of what they type (dictation/Ghost/Hand Mode). No `WS_EX_NOACTIVATE` helper exists.
- **Z-order/overlap**: right-docked sidebar (520 wide, 10 px margin) and the Windows toast zone (bottom-right 364x96) both sit on the right; caption slot bottom-centre; orb top-centre. Panels are arranged by `panel_layout` only for four panel kinds.
- **Multi-monitor/DPI**: stage = primary monitor only; sidebar uses the window's current monitor; geometry math is in `work_area.rs` with tests; toast placement must use work area (taskbar on any edge) and 100/125/150 % scaling (a recurring "not run live" item in AGENTS.md).
- **CSP/inline-style trap in stage windows** (React `style` props silently dropped) — use CSSOM.
- **Identity bugs**: bare project names collide across owners (fixed only for GitHub via `github_repo_index` + full-name events; deploys and mail have no owner/account in `Fact`); same-name repos produce no card (speech only) by design; a click must resolve an opaque event id through the ledger, never parse text/URLs (stated invariant, tested).
- **Alert dedupe layers can disagree**: `proactive_policy` id-dedup + event-key cooldown, `notify_brain` state machine, `Ledger` Created/Grouped/Duplicate. A toast fed from only one layer can double-notify or suppress a legit repeat (e.g. WhatsApp's old shared id bug, feature 151).
- **Privacy**: mail subject/sender/snippet and WhatsApp gist are sensitive; they must not be shown on shared screens; `WDA_EXCLUDEFROMCAPTURE` is applied only to glass windows (stage is deliberately NOT capture-excluded); native toasts in the Action Center persist the text system-wide; `PrivacyClass::Redacted` exists but is unused by producers; the `memcoreMailPlain` cloud rewrite is OFF and any toast-wording AI step must not send mail text to a cloud model without that opt-in.
- **Ledger is in-memory only** (lost on restart; expired/unknown clicks show "unavailable"); cap 200; no persistence table yet.
- **Ratchets**: events baseline contains `proactive:card/nudge`; adding emitters without listeners grows the baseline and fails; new `#[tauri::command]` not invoked by the frontend must be added to the unused baseline (allowed only if justified); new modules need `external-api.txt` only if `pub`.
- **Process rule**: AGENTS.md top note — owner controls runtime and builds; edits and docs only; nothing from this survey was executed. Concurrent edits happen in this tree (`orchestrator.rs`, `intent_parser.rs`, `center.rs` often) — keep patches anchored.
- **Doc/code drift**: AGENTS.md says the sidebar is non-activating and that notification state is "N5 not verified"; docs/changes/171 still describes the removed NotifyCard in N3/N4; verify against code before reusing statements.
