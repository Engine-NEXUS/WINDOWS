# Loading-state ownership in NEXUS (read-only code + log audit, 2026-10-29)

Source convention: all "sources" are local files (path:line). No web sources were needed. Log = `C:\Users\Chitkul Lakshya\AppData\Roaming\com.nexus.assistant\nexus_unified.log`.

## Q1. Which Rust code emits/changes the loading state, and what triggers each site?

### Takeaway
The only primitives are `show_loading`/`hide_loading` (orchestrator.rs:9997/10022), which just emit `stage:loading_visible` (+`stage:loading_rect` on show). They are called by hand-written per-subsystem blocks inside `process_transcript` and a few `run_*` runners, each paired with a manually emitted `OrchestratorEvent::Loading`. `center::direct_ui(UiDirective::Loading)` exists but has ZERO callers.

### Cited Findings
- Primitives: `show_loading` guards on `has_active_request()` and `ghost::session_active()`, then `emit_loading_rect` + emits `stage:loading_visible=true` — orchestrator.rs:9997-10014. `hide_loading` only emits `stage:loading_visible=false` and logs; it has NO guard and no request-id check — orchestrator.rs:10022-10025.
- IPC wrappers `orchestrator_show_loading` / `orchestrator_hide_loading` call the same primitives — orchestrator.rs:10029-10043 (registered lib.rs:1113-1114).
- Main-center "single choke point" `center::direct_ui` + pure predicate `loading_should_show(has_active_request, ghost_session)` — center.rs:733-759. The doc comment claims legacy callers are covered because the guard also lives in `show_loading`. Grep of `direct_ui(` / `UiDirective::Loading` shows callers ONLY for `UiDirective::Session` (ghost.rs:532,648,689,700); `Loading` is never directed through it, and `loading_should_show` is `#[allow(dead_code)]` (center.rs:725,732) and only used in tests (center.rs:1027-1030).
- orchestrator.rs:775-779 documents that there is deliberately no `is_long_running` helper: "the live loading decision is owned by the Ack/Loading event flow ... Decide loading per turn from events, not enums."
- Call sites (each = emit `OrchestratorEvent::Loading` + `show_loading`/`hide_loading`):
  - Subsystem::WorkerBackend (general Q&A / 9Router): Ack at 2231-2238, show at 2241-2248; hide at 2284-2291 (after `dispatch_to_worker` returns, BEFORE the Result is emitted); extra `hide_loading` on Err at 2401.
  - Subsystem::Architect: show 2445-2459 — NO hide in the Rust branch; comment 2461-2471 says the frontend emits "done" when the architect window opens (frontend `recorder.ts:929-934` also sets loading true for `open_architect`).
  - Subsystem::GitHub: show 2484-2498; hide 2525-2532 (fallback-to-worker path), 2553 (err), 2584-2591 (normal path, after `github_cmd::execute_command`).
  - Subsystem::Mcp: show/hide only for non-write intents (`!is_write_intent`) 2785-2802 / 2834-2843. Confirmed-write resume path (`orchestrator_mcp_confirm` area) has its own show 9834-9841 and hide 9878-9885.
  - Compound plans (`run_command_center`): Ack 9397-9404, show 9405-9412, hide 9439-9446 around `command_center::execute_plan`. command_center.rs contains no "oading" matches (grep returned none), so individual compound steps never touch loading.
  - Screen tour (`run_screen_tour`): State Thinking + Ack + `speak_line` + show at 4752-4778, only when `spoken` (not in a meeting); hide via `end_tour_fetch` (4696-4707), called on fallback paths (4786, `tour_fallback` 4711) and, per grep, from the narration path.
  - Reverse-engineer: show at reverse_engineer.rs:721-726; hide at reverse_engineer.rs:1107.
  - Counsel turn: only `hide_loading` on error (orchestrator.rs:7624); no show.
  - Calibration (companion HUD) drives `stage:loading_rect`/`stage:loading_visible` directly for previews: calibration.rs:158-160, 251, 346.
- `stage:loading_rect` source: `window_manager::emit_loading_rect` (window_manager.rs:264-271) and `get_pending_loading_rect` (window_manager.rs:55) for race-free pull on mount.

### Inferences
- The guard story in center.rs is aspirational: the real enforcement is the two checks inside `show_loading` (active request, not ghost). Nothing in `hide_loading` is guarded, so a hide for request A can hide request B's spinner (inference from absence of request id in `hide_loading`).
- Ownership is effectively split per sub-center branch inside orchestrator.rs; the main center (center.rs) only validates/routes.

### Gaps
- Did not exhaustively read every `end_tour_fetch` caller in the narration path or all of reverse_engineer.rs (lines 700-1107).

## Q2. Which frontend code sets/clears loading?

### Takeaway
Frontend has its own state machine (store + `loadingMachine.ts`) and ~30 `setLoadingVisible` writers; it ALSO re-invokes the Rust show/hide via an effect in OrbFrame, producing duplicate events. The visible spinner is rendered by `LoadingIndicator.tsx`, which listens only to the raw Rust events, not the store.

### Cited Findings
- Store setter runs `loadingMachine`: min dwell 800 ms, failsafe `LOADING_FAILSAFE_MS = 120_000` (comment says earlier 30 s), hide-when-hidden is a no-op — store/assistant.ts:186-213, store/loadingMachine.ts:16-19, 36-57.
- `net/orchestrator.ts` `case "loading"` (line 533-): sets `setLoadingVisible(isGhost ? false : ev.visible)`, and on show calls `hideOrbAfterSpeech(600)` + `armOrbSilentHideObserver()` (lines 537-548). Other clears at net/orchestrator.ts:382, 596, 639, 720, 731, 760, 798, 825.
- recorder.ts: sets loading TRUE locally at 934 (open_architect), 1262 and 1634 (after `waitForTtsIdle` + fixed 1200 ms delay, only if not ghost); ~20 clears (897, 943, 961, 982, 995, 1013, 1019, 1042, 1126, 1276, 1298, 1321, 1336, 1356, 1429, 1648, 1670, 1677, 1692, 1712, 1768).
- orbRuntime.ts clears at 169, 249.
- OrbFrame.tsx effect: whenever store `loadingVisible` changes it calls `orchestrator_show_loading` / `orchestrator_hide_loading` back into Rust (OrbFrame.tsx:656-665). Idle watchdog calls `setLoadingVisible(false)` + `reset()` on stuck (OrbFrame.tsx:574-579); speaking failsafe (60 s silent) clears it (OrbFrame.tsx:629-633); 8 s no-input timeout is listening-only (585-610).
- Watchdog constants: THINKING_STUCK_MS 60 000, LISTENING_STUCK_MS 30 000, IDLE_HIDE_MS 2 500 — net/orbIdleWatchdog.ts:37-41. Both idle and busy watchdog branches are SKIPPED while `loadingVisible` is true (orbIdleWatchdog.ts:53-60, 65-74), so a visible spinner suppresses the 60 s thinking guard; only the 120 s loadingMachine failsafe bounds it.
- Rendering: `LoadingIndicator.tsx` listens to `stage:loading_rect` and `stage:loading_visible` directly (lines 71-119) and renders only if `rect && (visible || calibrating)` (line 239); it does not read the Zustand store or request id.

### Inferences
- Because the store's 120 s failsafe and the stage component's raw `visible` state are independent, the spinner on screen can outlive the store's `loadingVisible=false` (e.g. Rust never sends hide) — the failsafe resets the store but I found no code that emits `stage:loading_visible=false` from it, except the OrbFrame effect, which does call `orchestrator_hide_loading` on store change (OrbFrame.tsx:662-663). So the failsafe does eventually reach the stage via that round trip (inference).

### Gaps
- Did not read every one of the ~30 frontend clear sites' context; ghost guards beyond `net/orchestrator.ts:538` and recorder 932/1260/1632 were not enumerated.

## Q3. Who decides: main center, sub-center runners, or frontend? Does everything use one guard? Which paths bypass?

### Takeaway
Decision is made by hand-coded blocks in orchestrator.rs per subsystem plus frontend local writers; `loading_should_show`/`direct_ui` (the main-center guard) is not used for Loading. The only shared enforcement is the check inside `show_loading`. Many long-running voice commands and all background jobs bypass loading entirely.

### Cited Findings
- No `direct_ui(Loading)` caller anywhere (grep over src-tauri/src) — see Q1.
- `show_loading`'s guard requires an active request (orchestrator.rs:10002) — any path that shows loading after `clear_active_request` is silently dropped (log "Loading(true) ignored — no active request").
- Paths WITHOUT any loading handling (verified by absence in grep of `Loading|show_loading|hide_loading`): every `run_*` runner not listed in Q1, including `run_foreground_review` (6176), `run_recreate_last_review` (6366), `run_coding_agent_task` (6424), `run_watch_then_power` (6496), `run_watch_agent_task` (6827), `run_hook_all`, `run_scan_internship_emails` (6956), `run_analyse_chat_docs` (7078), `run_screen_analysis` (7640), `run_youtube_search/journal`, `run_directions`, `run_places_search`, `run_open_coderabbit_hub` (8436), `run_coderabbit_review_this` (8457), `run_ai_review_this` (8480), `run_file_*`, `run_hand_mode`. (Inference: some of these may emit their own State/Ack; I did not check each.)
- CodeRabbit voice path: `run_coderabbit_review_this` calls `install_new_request`, resolves repo/PR, checks usage, sets an Offer, emits `coderabbit:consent`, and `say_and_finish` — no Ack/Loading (orchestrator.rs:8457-8477). `run_ai_review_this` speaks "On it, sir..." via `say_and_finish`, no Loading (8480-8490).
- Compound plans use the `run_command_center` wrapper block (9397-9446), not per-step loading.
- Frontend writers (recorder.ts 934/1262/1634) show loading on their own, independent of Rust events, for local flows; ghost guards: `net/orchestrator.ts:538`, `recorder.ts:932,1260,1632`, `OrbFrame.tsx:660`; `show_loading` ghost check at orchestrator.rs:10006.

### Inferences
- Because every Rust show is bracketed by "emit Loading event" + `show_loading` with identical conditions, double-sourcing is the norm: Rust shows, frontend re-shows via OrbFrame effect. The result is the duplicated log lines in Q5.
- "No single guard" — an owner would have to be introduced; `direct_ui` is the obvious existing seat but is unused for Loading.

### Gaps
- Not verified: whether `run_screen_analysis`, `run_foreground_review`, `run_coding_agent_task` or `run_watch_agent_task` emit Ack/State elsewhere or depend on frontend `recorder.ts` to show loading.

## Q4. What happens to loading for work that outlives the turn (background jobs)?

### Takeaway
Nothing owns it. The CodeRabbit review (2-15 min) and the AI review are spawned background jobs with NO loading/"still working" signal; the turn ends (`say_and_finish`), `ACTIVE_REQUEST` is cleared, and any later `show_loading` would be refused by its own guard. Completion is announced only via `proactive_policy::submit`.

### Cited Findings
- `coderabbit_review::start_review` inserts a `Job{state: Running}` in a static map and `tauri::async_runtime::spawn`s `report_for_pr`; on finish it updates the job and calls `proactive_policy::submit(..., Medium)` with `finish_line` — coderabbit_review.rs:461-490. No Loading event, no `show_loading`, no progress emit within lines 455-490.
- Voice "already running" reply: `"CodeRabbit is already reviewing PR {pr}, sir. I'll tell you when it finishes."` — orchestrator.rs:8463-8465. State is queryable via `coderabbit_job` (coderabbit_review.rs:455) / `is_running` (419).
- `ai_review::start` same pattern: spawn + job map + proactive Medium line at the end — ai_review.rs:444-478.
- Start is triggered from `Offer::CodeRabbitReview` yes or the on-screen button (`coderabbit_review_start` IPC) — coderabbit_review.rs:492-499; `WORKING_LINES` constant (357) holds spoken acks ("Running CodeRabbit now, sir...").
- `run_open_coderabbit_hub` clears the request immediately (8447); `clear_active_request` only clears if the id matches (orchestrator.rs:444-452); `has_active_request` is a single global slot (457-459).
- Frontend watchdog: orb spinner state is suppressed from idle/stuck handling only while `loadingVisible`; a background job leaves no such state, so the orb hides normally after the turn (orbIdleWatchdog.ts:53-75).
- Frontend sidebar panel `CodeRabbitPanel.tsx` shows per-PR "Loading…" text local to the panel (line 197); grep found no job-state polling or `running` indicator in that file (no matches for coderabbit_job/running/setInterval).

### Inferences
- Who "shows still working" for a 2-15 min job: nobody in the global loading channel; the only places are the job map (pull-based) and the completion announcement. Who clears it: not applicable — it is never shown.
- If someone called `show_loading` from the spawned task it would be dropped by the `has_active_request()` guard (orchestrator.rs:10002) after the turn ended; a separate background-activity indicator, not the turn spinner, would be needed. The frontend 120 s failsafe would also contradict a 15-minute spinner.

### Gaps
- Did not read what the CodeRabbit sidebar renders for a Running job beyond the grep above, nor `report_for_pr` internals.

## Q5. Observed log sequence

### Takeaway
Observed turns show: State(Thinking) -> Ack -> Loading(true) -> `stage:loading_rect`/`visible` -> "loading indicator shown" TWICE -> (work) -> Loading(false) -> "hidden" -> Result -> cleared active request -> TTS of the Ack; plus a SECOND "hidden" ~170 ms later. Loading is hidden BEFORE the Result and before the Ack is even spoken, i.e. the spinner lived ~0.64 s and ~0.93 s.

### Cited Findings
Turn 1, request `adf342d98b46` (WorkerBackend, "i'll shut down my laptop."), log lines 7208-7246:
- 20:20:00.276 new request; .276 State Thinking; .277 Ack "One moment sir."; .278 `emitted Loading{visible:true}`; .280 `emit: stage:loading_rect`; .281 `emit: stage:loading_visible`; .282 "orchestrator: loading indicator shown".
- 20:20:00.293 a SECOND `stage:loading_rect`, `stage:loading_visible`, "loading indicator shown" (15 ms later; consistent with the frontend `orchestrator_show_loading` from the OrbFrame effect or net/orchestrator `loading` handler).
- .815 9Router (Groq) answered in 502 ms.
- .919 `emitted Loading{visible:false}` -> .920 `stage:loading_visible` -> .921 "indicator hidden"; .922 `emitted Result`; .923 `cleared active request`.
- .943 TTS starts speaking the cached "One moment sir." (line 7235) — the Ack audio starts 665 ms after the Ack event and 20 ms after loading was hidden; the Result text itself is spoken afterwards.
- 20:20:01.093 a SECOND `stage:loading_visible` + "indicator hidden" (the frontend's echo hide).
Turn 2, request `86998aeb866e` (log 7356-7387): Loading true 20:20:10.526, shown at .529 and again .535; Loading false 20:20:11.456, hidden .457; echo hidden at 20:20:11.471 (~0.93 s of spinner).
- Also visible in turn 1: an STT "filtered repetition loop hallucination" message at .292 and `tts: stop requested` at .907 right before hide (line 7216, 7225).

### Inferences
- The spinner duration equals the model round trip; fast 9Router replies yield sub-second flashes (store dwell 800 ms exists only on the store, not the stage component, which is driven by raw events).
- The duplicate show/hide pairs are the Rust-direct call plus the frontend effect round trip (OrbFrame.tsx:659-665), harmless but evidence of double ownership.

### Gaps
- Only two turns inspected; no log sample of a GitHub/MCP/compound/tour turn or of a CodeRabbit background job (none found in the 12 matched lines — all matches were from the 20:20 window plus a lone `hidden` at line 50, 19:19:47).
