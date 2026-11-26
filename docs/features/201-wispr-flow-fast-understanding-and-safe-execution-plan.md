# 201 — Wispr Flow-Grade Understanding & Zero-Mistake Fast Execution Plan

> **Status:** Planning / Analysis (2026-11-26)  
> **Goal:** Combine **Wispr Flow**-level speech understanding (app-aware context, filler/self-correction cleanup, proper-noun accuracy) with **Jev / Andy Gao**-style low-latency execution—while enforcing **zero accidental deletions, zero blind overwrites, and verified working-state execution**.

---

## 1. What Makes Wispr Flow + Jev Feel "Instant and Perfect"

| Capability | How Wispr Flow / Jev Does It | What NEXUS Already Has | What Is Missing in NEXUS |
|---|---|---|---|
| **Turn-start & end speed** | Immediate capture; low-latency endpointing | `SileroVad` (`vad.rs`) + `Smart Turn v3.2` (`turn_detect.rs`) | Both flags are **OFF** by default; `turn_detect.rs` uses naive DFT (~129 ms) instead of FFT (~12 ms). |
| **Mid-sentence responsiveness** | Streaming partial STT + non-autoregressive decision pass (`Jev`) | `stt_stream.rs` + `<5 ms` `intent_parser.rs` + `flows/` clause split | `orchestrator.rs` waits for full end-of-turn before parsing or dispatching safe clauses. |
| **Context-aware accuracy** | Reads active app/tab category + visible proper nouns; cleans fillers & self-corrections | `stt_learning.rs` (auto-learned word fixes) + `dictation.rs` + `desktop_native.rs` | No active-window **style/vocabulary priming** (Slack vs VS Code vs Gmail) or self-correction rewriter (*"send at 5, actually 6"*). |
| **Zero-error execution** | Typed schemas (`0%` syntax error) + app-scoped action harness | `center::ActionSpec` + `action_kernel` (`.nexus-trash` + undo) + `desktop_control.rs` effect checks | Speculative/fast path is not yet wired to a strict **risk-tier gate** + **target-window lock**. |

---

## 2. Possibilities Analysis: Fast Understanding vs. Safe Execution

### Possibility A — Speculative Mid-Sentence Execution (Safe Actions Only)
Instead of waiting 800–1200 ms after the user stops speaking:
1. `stt_stream.rs` emits partial transcripts every **~150 ms**.
2. `orchestrator.rs` slices from `committed_cursor..` and runs `parse_deterministic()` looking for a complete clause followed by a boundary (`"and"`, `"then"`, or 200 ms micro-pause).
3. **Stability Filter:** The parsed `(intent, target)` must match across **2 consecutive partial ticks** so STT word flicker (`"open no..."` → `"open notes"`) never misfires.
4. **Strict Risk Gate (`ActionSpec`):**
   - **Allowed mid-sentence (`Safe / Reversible`):** `OpenApp`, `FocusWindow`, `OpenUrl`, `NewTab`, `Scroll`, `MediaControl`.
   - **Blocked mid-sentence (`Write / Destructive / External`):** `Delete`, `CloseApp`, `SendMessage`, `GitHubWrite`, `Shell`, `Power`. These **always** wait for full end-of-turn + confirmation.

### Possibility B — Wispr Flow-Style Context Priming & Self-Correction Cleanup
Raw STT transcripts often fail on project names, contacts, or natural human mid-sentence edits:
1. **Active-App Context Tagging:** Before/during transcription, read the foreground exe + window title (`desktop_native.rs` / `browser_url.rs`) and classify into `Code` (VS Code, terminal), `Mail` (Gmail, Outlook), `Chat` (WhatsApp, Slack, Discord), `Browser`, or `Docs`.
2. **Dynamic Vocabulary / Proper-Noun Priming:** Extract visible UIA labels/tab titles (e.g., repo names, contact names, open file names) and feed them as a lightweight bias list to `stt_groq.rs` (`prompt` parameter) and `stt_learning.rs`—without sending full screen contents.
3. **Lightweight Cleanup Pass (Dictation & Command):**
   - Strip hesitation fillers (`uh`, `um`, `like`) and resolve inline self-corrections (*"meet at 4, no wait, 5 pm"* → *"meet at 5 pm"*).
   - Match formatting to the active app category (e.g., formal punctuation in `Mail`, code-casing/identifiers in `Code`, casual in `Chat`).

### Possibility C — "No Deletion, Working State" Execution Guarantees
Fast execution is useless if it deletes the wrong file, types into the wrong window, or leaves half-finished state:
1. **Zero Permanent Deletion:** Every file delete routes exclusively through `action_kernel` (`.nexus-trash` quarantine + read-back confirmation + `"undo that"`). No direct `std::fs::remove_file` in any voice/flow handler.
2. **Caret & Window Lock for Dictation (`dictation.rs`):** Capture `(hwnd, process_name, control_id)` at turn start and **re-verify before paste/insertion**. If the user Alt-Tabbed mid-sentence, hold the text in a stage toast/clipboard instead of typing into the wrong app or overwriting a selection.
3. **Effect-Verified Compound Execution (`desktop_control.rs` + `flows/`):** Every step in a multi-step command must observe its expected post-condition (e.g., target window actually became foreground, element state changed) before step $N+1$ runs. On any mismatch, halt cleanly and report what succeeded vs. what stopped.
4. **Sub-300 ms Local `"Stop / Cancel"` Barge-in:** Pair `sonora` (pure-Rust AEC3) with a local `"stop"`/`"cancel"` spotter (`openWakeWord` embedding head or `sherpa-onnx` KWS) so the user can abort any action instantly without a 1–1.5 s cloud STT round-trip.

---

## 3. Phased Implementation Roadmap (W0 – W4)

| Phase | Scope | Key Files | Success Gate |
|---|---|---|---|
| **W0: Latency Foundation** | Replace naive DFT in `turn_detect.rs` with `rustfft`; zero-alloc frame buffer in `vad.rs`; run 30-clip A/B to enable `vadSilero` + `smartTurn` by default. | `src-tauri/src/turn_detect.rs`, `vad.rs`, `wakeword_oww.rs` | Smart Turn feature extraction `<15 ms`; fewer cut-offs than 800 ms baseline. |
| **W1: Context Priming & Cleanup** | Add foreground app-category detection + visible UIA proper-noun bias list for Groq STT + filler/self-correction cleaner for dictation & commands. | `stt_groq.rs`, `stt_learning.rs`, `dictation.rs`, `desktop_native.rs` | Measurable drop in proper-noun/repo-name WER; zero wrong-window pastes. |
| **W2: Speculative Safe-Clause Runner** | Wire streaming partials (`stt_stream.rs`) → 2-tick stability filter → `ActionSpec` risk gate → `committed_cursor` in `orchestrator.rs` + live `✓` pill feedback. | `orchestrator.rs`, `stt_stream.rs`, `center/`, `stage.rs` | Safe app/tab opens start `<350 ms` after clause end; 0% destructive misfires. |
| **W3: Instant AEC3 + Local `"Stop"`** | Prototype `sonora` AEC3 + `<300 ms` local stop-word spotter to replace RMS-only `EchoGate`. | `barge_gate.rs`, `wakeword_oww.rs` | Barge-in `"stop"` halts TTS/actions in `<300 ms` with speakers active. |
| **W4: Live Verification & Graduation** | Run full `desktop-control/work-card.md` + `flows/` live scenarios + 100-utterance mixed command/dictation soak test. | `docs/testing/desktop-control/work-card.md`, `flows/` | Zero unconfirmed deletes, zero wrong-window edits, target end-to-end SLOs met. |

---

## 4. Non-Negotiable Safety Invariants
1. **No mid-sentence destructive actions:** `Delete`, `Send`, `Close`, `Shell`, `GitHubWrite`, and `Power` never execute speculatively.
2. **Re-verify target before mutating:** Every UIA click, keystroke, or dictation paste re-checks `(hwnd, exe)` right before dispatch.
3. **Quarantine-only deletes:** All voice file removals must remain reversible via `.nexus-trash` and `"undo that"`.
4. **Explicit rollback/stop:** Any failed or unconfirmed step halts the remaining chain immediately and leaves the system in a known, working state.
