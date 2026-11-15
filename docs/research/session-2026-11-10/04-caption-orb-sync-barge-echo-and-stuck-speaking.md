# Caption, orb and voice out of step: barge-in on its own echo, stale tts-ended, stuck speaking orb (2026-11-10/11)

## 1. The reports

1. "The voice, caption and orb are not in sync; it seems broken."
2. After the first fixes: "it is working but the orb's speaking shape is shown when it is silent and it doesn't
   disappear when it stopped speaking."

## 2. Method

The frontend console is not in `nexus.log` when the exe is started directly (the unified console capture only runs
through `nexus start`). So the stage page was driven over CDP: NEXUS started with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`, a Node script (global `WebSocket` of Node 24) hooked
`Runtime.consoleAPICalled`, polled the DOM every 40 ms (caption text, orb capsule), invoked a real speech command, and
printed a timeline next to the Rust log. Two speech paths were used: a direct `speak_text` (no orchestrator) and a real
turn through `orchestrator_process`.

## 3. Findings

### 3.1 The caption was in step with the voice

The caption typed word by word with the audio and cleared 2.5 s after the last word (the designed dwell). The orb was the
part that went wrong.

### 3.2 NEXUS mistook its own voice for the owner talking over it (the main cause)

Timeline of a two-sentence test line (log time):

1. orb `speaking`, caption typing: correct;
2. about 6 s in, during sentence two: `[TURN] barge onset ref=0.082 ratio=0.050`;
3. Rust ducked the audio and opened a listening capture; the orb flipped `speaking -> listening` mid-sentence while the
   caption kept going;
4. the capture transcribed `"it has just a trick."` (the echo of "It has two sentences") and classified it `Resume`;
5. after the reply the orb went to `thinking` instead of idle.

Why (`barge_gate.rs`): the gate compares the mic level to the level of speech it is playing and learns the speaker-to-mic
echo gain as an EMA. Three defects:

- the gain was learned **only from chunks not flagged as onsets**, so when the real echo was louder than 2.5x the guess
  those chunks were never learned from: the estimate drifted low (0.05 observed against a default of 0.1) and the echo
  kept being flagged;
- the reference level is read at the current playback position, but the mic hears the speaker later: in the pause after a
  word the mic is briefly louder than the *current* reference, which the relative threshold reads as someone speaking;
- near a silent reference (between words, the tail of a reply) the relative threshold fell to the 0.01 absolute floor, a
  level the room's reverb of the voice itself exceeds. A laptop with the mic beside the speakers does this on most
  replies.

### 3.3 Stale `tts-ended` lifted the speaking flag mid-reply

Log of a 3.2 s line: `tts: speaking` 18:11:58.04, `meeting: TTS ended - resuming wake detection` 18:11:58.51,
`tts: audio playback completed` 18:12:01.29. Stopping a previous utterance sends a stray `tts-ended` just before the
next `tts-started`; Rust acted on it 500 ms later and cleared `tts_playing` while the new line was playing. Effects: the
wake-word block was lifted during speech (self-trigger risk) and `set_tts_playing(false)` raced the real end.

### 3.4 `barge:resume` ignored the thinking state

The duck puts the orb in `listening`; when the capture stops to be transcribed the recorder moves it to `thinking`; the
resume handler only acted on `listening`, so the orb stayed in `thinking`.

### 3.5 No watchdog for a stuck speaking orb

`net/orbIdleWatchdog.ts` hid an idle orb and recovered a stuck `thinking` (60 s) or `listening` (30 s), but never looked
at `speaking`. A speaking orb with nothing audible waited for a 60 s failsafe in `OrbFrame.tsx`. That fits the second
report. It was **not reproduced**: on two real-path turns the orb stayed in `speaking` for the whole reply, lingered
about 0.8 s after the voice ended, then went to the 6 s follow-up `listening` ring and hid. The follow-up listening ring
is designed behaviour and the owner may be perceiving it; they were asked which moment they saw.

## 4. Changes

| Change | File |
|---|---|
| learn echo gain from every chunk of the first 0.8 s (`LEARN_ALPHA` 0.2, nothing can fire), 3x instead of 2.5x (`ONSET_MULT`), threshold against the loudest reference of the last 400 ms (`REF_WINDOW` 5), absolute onset floor 0.03 (`ONSET_FLOOR`), learned gain never below 0.05 (`MIN_RATIO`) | `src-tauri/src/barge_gate.rs` |
| `stateAfterBargeResume(current, ttsPlaying)`: listening or thinking -> speaking if still talking else idle; other states untouched | `frontend/src/stage/bargeOrb.ts`, used by `orbRuntime.ts` |
| a `tts-ended` is ignored when a newer `tts-started` arrived in its grace period (`tts_end_applies`) | `src-tauri/src/lib.rs`, `tts.rs` |
| `isSpeakingStale`: `speaking` with no audio, caption, loading, pending question or ghost session for 4 s (`SPEAKING_STALE_MS`) -> `reset_and_hide` | `frontend/src/net/orbIdleWatchdog.ts` |

## 5. Verification

- Barge gate: 12 tests, including a steady loud echo (30% of the reference) learned and never firing, a late echo
  (three chunks behind the reference) never firing, reverb in the reply tail never firing while a real voice over a
  quiet moment still does. Mutation check: setting `REF_WINDOW` to 1 makes the late-echo test fail.
- 236 related Rust tests, 3,178 lib tests on the combined tree, 1,129 frontend tests, tsc, IPC / turn-end / hygiene /
  dead-code gates.
- Live: four test lines on the rebuilt app with the debug port, **no orb flip** (before the fix: a flip on every run).
  An intermediate build still showed false onsets at the tail of a reply (`ref=0.005 ratio=0.012`); the absolute floor
  and minimum gain were added because of that.
- Watchdog: three unit tests (closes when silent, never touches the orb while audio/caption/pending, restarts its clock
  when audio resumes).

## 6. Not verified

- The owner's real use: their speakers and volume, a person talking over a reply. The gate may now be too strict and
  miss real interruptions; this needs the owner speaking over a reply while the log is watched.
- Barge-ins in the test log with `ref=none` (the reference is not playing yet, or the room had speech): these are still
  possible and could not be told apart from room speech.
- That the 4 s speaking watchdog changes what the owner saw (the symptom was not reproduced).
- Caption timing re-measured on the final build (only the orb was checked).

## 7. Follow-ups

- A guided test: owner speaks over a reply at normal volume; pass rule "ducks within 400 ms and the capture carries the
  owner's words".
- Decide whether the 6 s follow-up listening ring should hide sooner when no one speaks.
- Log the frontend `[ORB]` / `[SPEECH]` lines into `nexus.log` so these investigations do not need a debug port.
