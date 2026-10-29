# 172 — Voice flow audit, speech continuity, and one owner for the loading state

Date: 2026-10-29. Branch work after PR #37 (CodeRabbit on request, notification hub). Source and tests only: nothing was
rebuilt, started or heard live, and nothing was run against the real app. The owner controls builds and runtime.

This record covers four things done in one session, in the order they happened:

1. [CodeRabbit review findings](#1-coderabbit-review-findings) — seven findings checked against the code, six fixed.
2. [Workflow audit](#2-workflow-audit-is-the-main-center-in-charge) — is the main command center really first, is anything called twice.
3. [Speech continuity](#3-speech-continuity-voice-changes-and-cut-off-lines) — the third voice mid-reply and one line cutting another.
4. [Loading state and the orb](#4-loading-state-and-the-orb-at-an-acknowledgement) — who owns the loading state, why the orb misbehaves at "On it, sir", and the six-step fix.

The full research behind part 4 is in
[`docs/research/loading-state-research/reports/Loading state and orb control.md`](../research/loading-state-research/reports/Loading%20state%20and%20orb%20control.md)
with the three note files next to it (`research_notes/Loading state and orb control/`).

---

## 1. CodeRabbit review findings

One real free CodeRabbit CLI review of the branch returned seven findings. Each was checked against the code before any
change; six were real.

| # | Finding | Cause | Fix |
|---|---|---|---|
| 1 | critical, `commands.rs` | The committed `0e886bd` had a stray `memcore_account_seed:` line inside the voice list in `list_tts_voices`, so that commit did not compile. The working tree already had it only in `NexusSettings::default`. | Nothing to change; committing the working tree fixes it. |
| 2 | `stt.rs` | The "closing sentence repeated" hallucination filter blanked the whole turn, including a real request before the repeat. | `without_repeated_ending` drops only the duplicate ending and keeps the rest when a remaining sentence is a known command; real hallucinations are still blocked. |
| 3 | `net/orchestrator.ts` | A result waiting for the acknowledgement to finish could fire after a newer turn started and cut into it. | The timer checks it is still the current turn. |
| 4 | `net/orchestrator.ts` | A pending acknowledgement (600 ms delay) was not cancelled when a clarification or confirmation prompt arrived. | Both prompt handlers cancel it first. |
| 5 | `memcore/briefing.rs` | The short "Build failed in X" wording was applied to any watched mail whose subject looked like a CI failure, stripping the real sender and deadline. | Applied only to mail filed as GitHub (decided from the sender's domain). |
| 6 | `memcore/account_seed.rs` | The first name stayed as the seeded one after the owner said their own name. | The stored first name is used only when it belongs to the seeded full name. |
| 7 | `intent_parser.rs` | "Explain the difference between these options" counted as a stray statement while a yes/no question was open, so NEXUS asked again. | Common request openers are not stray statements. |

---

## 2. Workflow audit: is the main center in charge

**Question:** does a sub-center take the call before the main center verifies it, and is anything called twice?

**Findings (code and the live log):**

- For every microphone turn the order is right. The frontend returns right after `processViaOrchestrator`
  (`recorder.ts`), so the Rust orchestrator is the only owner. Inside `process_transcript`: offer reply, routines, admin,
  device questions, parse, owner check, `prepare_action` (the main center's verdict), and only then the `run_*` runner.
- Exceptions that skip the main center's verdict: early intercepts before the parse (power cancel, yes/no answers,
  routines, admin, device questions), `skip_validation` during dictation / Ghostwriter / a ghost drill, the frontend's
  old local-execute branch (typed, session-less turns only) and the Tier-3 acoustic listener (dormant).
- No command ran twice: each logged turn has exactly one `new request`.
- Wasted work: the capture thread parses the transcript, the frontend ignores that for mic turns, and the orchestrator
  parses it again, on the corrected text, so the two can disagree. **Not changed.**
- Only 4 of the 17 centers are real code units (Browser, System, YouTube, Google implement `SubCenter`, which only checks
  required slots). The rest are labels in `CenterId`; `orchestrator.rs` is ~11,000 lines with 83 `run_*` functions.
- Misfiled intents: Memory holds 24 intents of six unrelated jobs; Architect holds 17 of three; calendar and mail sit in
  Memory while Google has 4; block/unblock and WhatsApp read sit in Memory not Message; `PowerAction` is in Memory.
  **Recommendation only, nothing re-filed yet:** re-file those, split Memory (Memory / Planner / Contacts) and Architect
  (Review / Agents / Watch), then move runners out of `orchestrator.rs` one center at a time.

---

## 3. Speech continuity: voice changes and cut-off lines

**Reported:** local and cloud voice differ; the voice changes mid-reply; NEXUS sometimes stops partway and says something else.

**Causes found:**

- Three voices can speak: Edge `en-US-AvaNeural`, the Kokoro `af_heart` twin (a different model, so never identical), and
  the Windows voice. In the log at 19:19:57 a 5-sentence boot briefing hit an Edge failure ("no audio was received") and
  its tail was read by the **Windows voice**; the log line even said "same voice as the first sentence".
- A retry was immediate, into the same congestion (the burst of connections right after boot).
- The speech arbiter only queues `alert`, `narration` and `preview`; every background line (boot hello, briefing,
  alerts, reminder questions) reaches the frontend as a `Result` event, so it counted as an answer and could cut one.
- One hotkey press ran `stop_tts` 5–6 times within 20 ms. A line that started inside that window was killed by a later stop.
- Checked and **dropped:** the short acknowledgement being clipped by its reply. It is cut at 1.05 s but its last word
  ends at 0.96 s in the cached clip.

**Fixes** (`tts.rs`, `speechArbiter.ts`, `net/orchestrator.ts`):

- A failed cloud sentence pauses 450 ms and retries once, then the rest of the reply is finished in the **local twin**;
  the Windows voice is the last resort. One switch at most per reply (`ChunkEngine`).
- `speechKindForResult`: ids starting `sentinel_` or `boot_` are `alert`, so they queue behind a playing answer.
- `stop_tts` is idempotent: a second stop with no new line since the last one does nothing (`stop_has_work`).

---

## 4. Loading state and the orb at an acknowledgement

**Reported:** while a task runs in the background there is no loading state, and the wake-up orb disappears when NEXUS
says "On it, sir / Ok sir / Give me a minute, sir". Which center controls loading, and is a new one needed?

### Research cause

Nobody owns the loading state. The purpose-built guard `center::direct_ui(UiDirective::Loading)` had **zero callers** and
`loading_should_show` was dead code. In practice:

- about six hand-written Ack + Loading blocks in `orchestrator.rs`, and many runners with none at all;
- a second loading state machine in the frontend, plus an `OrbFrame` effect that echoed every store change back into
  Rust (the log showed two "shown" and two "hidden" lines per turn);
- `hide_loading` had no request id, so a late hide from one turn could hide the next turn's spinner;
- the 2–15 minute CodeRabbit and AI reviews had no loading signal at all: the request is cleared when the spoken line
  ends the turn, `show_loading` then refuses an "orphan spinner", and the frontend failsafe gives up after 120 s.

Why the orb misbehaved (from the code; the supplied log did **not** reproduce it, both logged ack turns kept the orb up):

- **Plain acks (Worker, MCP, GitHub):** `hideOrbAfterSpeech` only hid at `idle`, and after the ack nothing returned the
  state to `thinking`, so the orb sat in a silent "speaking" pose beside the spinner. The comments said "the orb hides
  when the loading indicator takes over"; the code never did.
- **Result-style acks (screen tour, reverse engineer, "Give me a minute, sir."):** the runner emitted an `Ack` event **and**
  a `speak_line` Result for the same words. The frontend treats a Result as a finished turn: it reset to idle, cleared
  the request and the spinner, and the orb idle-hid within about 0.5–2.5 s. That is the real "orb disappears at the ack".
- **Inert guards:** `speechBecameAudible` was never reset, so after the first spoken line of a session the 4 s silent-hide
  observer and the ack-deadline guard never acted again; one stuck-speaking counter was shared by every live hide chain.
- **Hotkey:** in the log, a second Ctrl+Space press ended both ack turns and hid the orb (by design: it cancels).

**Is a new center needed? No.** Home Assistant Assist, LiveKit Agents and Pipecat each have one lifecycle owner and the UI
subscribes; a separate manager would add a second writer, which is the existing defect. Rust is the only writer of "a turn
is pending", the audio player the only signal that speech ended, and orb visibility is one derived function.

### Steps and what changed

| Step | Change | Where |
|---|---|---|
| 1 | Logging only. Every orb hide writes one `[ORB]` line (who, turn, newest turn, state, loading). The store logs the three nearest callers of `visible=false`. The Rust spinner hide logs the active request id. | `net/orchestrator.ts`, `store/assistant.ts`, `orchestrator.rs` |
| 2 | `speechBecameAudible` resets on each turn's ack. Per-chain stuck and caption counters. Hide chains are scoped to their turn and end when a newer user turn exists (boot/sentinel ids do not count). Rust `hide_loading_for` skips a hide when another request owns the spinner; all 11 hides use it. | `net/orchestrator.ts`, `orchestrator.rs`, `reverse_engineer.rs` |
| 3 | Removed the `OrbFrame` echo into Rust. The spinner is hidden by Rust: the branch's own hide, and now `orchestrator_done` (scoped to its request) at the end of any turn. Deleted the two IPC commands that lost their only caller. | `OrbFrame.tsx`, `orchestrator.rs`, `lib.rs` |
| 4 | `decideAfterSpeech` in `net/orbVisibility.ts` is the one answer to "what does the orb do when a line ends". An acknowledgement that ends while loading is up goes back to the thinking pose and **the orb stays**. Policy flag `HAND_OVER_TO_SPINNER = false`; set it true to return to "the spinner carries the wait". An orb waiting for an answer or confirmation is no longer hidden. | `net/orbVisibility.ts`, `net/orchestrator.ts` |
| 5 | Every loading show/hide in `orchestrator.rs` and `reverse_engineer.rs` now goes through `center::direct_ui` (`Loading(true)` and the new scoped `LoadingEnd`), so the guards hold for every subsystem; `loading_should_show` is live. The tour and reverse-engineer no longer send the acknowledgement twice as a Result; the 6 s "Give me a minute, sir." is an interim `Ack`, not a Result. | `center.rs`, `orchestrator.rs`, `reverse_engineer.rs` |
| 6 | New background task registry and a small chip. CodeRabbit and AI reviews register on start and remove themselves when finished or failed; a task nobody finishes expires after 20 minutes. The chip sits under the spinner and is never interactive. | `background_tasks.rs`, `coderabbit_review.rs`, `ai_review.rs`, `stage/BackgroundTaskChip.tsx` |

**Decision to confirm:** while work is pending the orb now **stays** (thinking pose). That follows the report that the orb
"disappears". Flip `HAND_OVER_TO_SPINNER` in `net/orbVisibility.ts` if you want the old hand-over look back.

### Verification

- Rust lib: 2813 passed, 0 failed (27 ignored; `tts_bench` and `tts_kokoro` skipped as before).
- Frontend: TypeScript clean, `vitest` 751 passed (78 files), production build ok.
- Gates: turn-end, hygiene, IPC contract (247 commands), window-opacity registry all pass; dead-code baseline shrank 145 → 143.
- Mutation checks, each killed by a test and restored: shared stuck counter, stale-turn rule, Rust hide rule, alert
  classification, fallback order, stop rule, orb hand-over policy, task expiry.
- New tests: `speechArbiter.test.ts`, `orchestrator.test.ts` (steps 2 and 4), `orbVisibility.test.ts`,
  `backgroundTaskModel.test.ts`, `background_tasks` (3), `loading_hide_scope_tests` (3), `voice_continuity_tests` (3).

### Not verified, owner to check after a rebuild

- Nothing here ran in the real app. In particular: the removed duplicate acknowledgement in the tour and reverse engineer
  (Tauri-bound, no automated test), the chip on screen during a real 2–15 minute CodeRabbit review, the orb staying up
  through a slow Worker turn, and a hotkey press during an acknowledgement.
- The supplied log never showed the orb vanishing at an ack. The step-1 `[ORB] hide by ...` and `setVisible(false) from ...`
  lines are there to settle which cause is real on the next occurrence.
- Long-turn behaviour comes from reading code; no logged turn lasted over about a second.
- Not done: the duplicate parse (section 2), the center re-filing and splits (section 2), and the second `ipc-cancel` that
  arrives 1.2–1.75 s after a hotkey press (caller not traced).

### Live check list

1. Slow Worker turn (over 6 s): hear the ack, orb stays in the thinking pose, spinner top-right, one `[ORB]` hide only at the end.
2. Screen tour: one acknowledgement (not two), orb and spinner stay until the narration starts.
3. "CodeRabbit review of this PR" then yes: the chip appears under the spinner with a running clock and disappears at the finish.
4. Press Ctrl+Space during an acknowledgement: it cancels, and the log names the hide.

---

## 5. Local CI script and a warning-free build

`node scripts/ci-local.mjs` runs every check `.github/workflows/ci.yml` runs (frontend, worker, voice tools, python, rust),
sequentially, and exits non-zero if any fails. The Rust steps also fail on **any compiler warning**. Options: `--only
frontend,worker,voice,python,rust`, `--fast` (skip the slow Rust test run), `--list`, `--ci` (clean `npm ci` first),
`--installer` (the NSIS build of `build-windows.yml`, about 15 minutes, off by default).

Warnings removed so the checks are clean: test-only helpers marked `#[cfg(test)]` (`parse_settings_content`,
`migrate_api_keys_to_keychain`), a duplicated `#[test]`, an unused import and variable, an unused test method, a
camel-case test name, an unfulfilled `#[expect]`, the `mock-wake` build-script warning, and the dead code that
`--features mock-wake` leaves by design (one crate-level `expect(dead_code)` active only under that feature). The
dead-code baseline shrank to 143. The wake-data gate now validates like a clean checkout (`--clean-checkout`), because
`wake_word_data/` is git-ignored and absent in CI; the local manifest's health (420 rows point at missing audio files on
this PC) is reported by the script as a note, never as a failure, and nothing in that data was touched.

Result on this PC: all 33 checks pass (13 minutes), and `nexus build` finished with no warning or error lines
(release binary 100.5 MB, build identity `89eb4c11ebe7b2a6`). Not run locally: the installer build (CI ran it on the last PR).
