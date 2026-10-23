# Playback timeline for speech, orb, and captions (2026-10-17)

## Purpose

NEXUS previously started caption animation when a `tts:caption` event arrived, while the orb followed separate `tts-started`/`tts-ended` and turn signals. Audio could still be queued or interrupted at that moment. This change moves both visuals onto the active audio playback position and gives spoken intents a shared frontend arbitration path.

## Implemented

- `src-tauri/src/speech_sync.rs` owns an utterance ID, monotonically increasing frame sequence, request/source/kind metadata, a bounded caption snapshot, and a stage channel. Only the `stage` WebviewWindow can register, acknowledge, or read the snapshot. The player publishes `prepared`, `chunk`, `playing`, `finished`, `cancelled`, and `failed`. Channel IPC runs outside the shared state lock.
- Before appending samples to rodio, the player sends `prepared` and waits up to six seconds for the stage to confirm that its orb and caption have rendered. If the stage channel is missing, the prepared snapshot remains available while the existing stage watchdog recreates its window. If the stage does not confirm, the utterance fails without audible playback and the frontend opens a text-only error in the sidebar. This covers normal, cached, streamed, and screen-tour narration paths. Playback updates come from `Sink::get_pos()`; streamed chunks add the durations of completed queue entries.
- `frontend/src/audio/speechStage.ts` retries initial registration up to six times, restores a snapshot after stage registration, rejects old utterance IDs and sequences, and renders caption progress and the orb envelope in one animation-frame loop. A restart does not revive an already expired caption. Edge boundaries reveal words as playback advances. Estimated local Kokoro boundaries show the current clause. A completed caption has a two-second reading dwell; cancellation clears it immediately.
- `frontend/src/audio/speechArbiter.ts` and `ttsPlayer.ts` route normal and cached speech through a typed intent with source center, request ID, and kind. A result can supersede an acknowledgement, a late acknowledgement cannot interrupt a result, and alerts can wait behind active speech. The pending queue is bounded to eight intents with a 30-second expiry. The main orchestrator passes its source center in emitted events and uses it when requesting speech.
- The stage logs one content-free playback receipt with utterance ID, source, kind, position, completion reason, displayed clause index, orb visibility, event-lag p95, and sequence gaps. Spoken words are not logged by this receipt.
- Set `NEXUS_SPEECH_SYNC_V2=0` **before launch** to use the previous caption and TTS activity path for rollback.

## Verification performed

- TypeScript compile and full frontend Vitest suite passed (41 files, 334 tests, including arbitration, caption-position, stale-frame, expired-caption restoration, and new-subcenter cases). The production Vite build passed; its remaining `lottie-web` eval notice originates in the dependency. The caption-scheduler import notice was removed after that build.
- `cargo check --features custom-protocol,admin-brain` passed after the Rust changes. `cargo test --lib tts:: --features custom-protocol,admin-brain` passed 31 tests (one existing hardware-dependent test ignored).
- The rodio 0.19 source confirms that `Sink::len()` counts queued sounds and `Sink::get_pos()` reports the current sound's position, which supports the cumulative chunk-offset calculation.

## Release gates still requiring a live Windows run

This code does **not** establish the plan's 100 ms p95 / 200 ms worst audible alignment target. `Sink::get_pos()` is the software source position; its offset from the actual Windows render clock and speaker output must be measured. If that offset misses the target, use the Windows audio render clock for the Windows player. Exercise short and long replies, 10+ streamed chunks, cache hits, Edge/local fallback, low memory, heavy CPU/GPU load, stage restart, sleep/wake, device changes, meeting suppression, and rapid barge-in. Confirm no audible speech with a hidden orb or absent caption, no stale caption after cancellation, bounded memory, and no sustained stage freeze over 250 ms. Keep the rollback flag until these gates pass.

## Research basis

- [rodio 0.19 `Sink` implementation](https://docs.rs/rodio/0.19.0/rodio/struct.Sink.html): playback position and queue semantics. The installed crate source was inspected during implementation.
- [Tauri v2 channels](https://v2.tauri.app/develop/calling-rust/): ordered Rust-to-frontend updates and stage registration.
- [MDN `requestAnimationFrame`](https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame): display-frame scheduling and its limits under a blocked WebView.
- [Microsoft `IAudioClock::GetPosition`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclock-getposition): prospective hardware render position for the Windows acceptance measurement.
- [Azure Speech word boundaries](https://learn.microsoft.com/azure/ai-services/speech-service/how-to-speech-synthesis?pivots=programming-language-python): conceptual boundary offsets; this project uses its own Edge TTS boundary output.
