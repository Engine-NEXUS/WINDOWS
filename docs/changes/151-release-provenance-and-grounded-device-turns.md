# Release provenance and grounded device turns (2026-10-20)

## Observed failure

The supplied `nexus start` log used a release executable built at 12:57. The later debug build already allowed Brave screen analysis when its URL was unavailable, as long as the foreground app, title, region, and unchanged focus were verified. The old release produced the refusal. Groq returned `analyze my screen` on two turns with voiced microphone audio; the client changed the spelling to `analyse`. Those two turns do not establish an STT hallucination. Speech recognition can still fail on silence, background media, or unfamiliar names.

## Changes

- `scripts/build-provenance.mjs` writes `src-tauri/target/release/nexus-build.json` only after a successful release build. It fingerprints build inputs, `frontend/dist`, and the release executable. `nexus.mjs` and `scripts/run.ps1` verify the same record before launch; the PowerShell check precedes its process cleanup. Both print identity, build time, and executable path. `nexus start --build` and the build helpers explicitly rebuild with `custom-protocol,admin-brain` and refresh the record. The record is generated output, not a source file.
- Native microphone turns now carry the raw Groq transcript, display correction, and capture session to Rust. Native voice bypasses the frontend local executor. Rust asks for a repeat if a correction changes an executable phrase or target, and unknown imperative speech cannot become a cloud-backed action. Enrolled owners retain command vocabulary; unenrolled and uncertain speakers use neutral STT decoding. The frontend's hardcoded repository-name substitutions were removed; Rust resolves relevant names against local sources where supported.
- Device questions are claimed before general chat. Screen and visible-file questions use the existing screenshot/tour path with its sensitive-window block and focus recheck. The vision context carries the OS probe source and capture time. Foreground-app answers come from the verified OS window and include its title. A foreground GitHub repository is named only when the current verified URL contains `github.com/owner/repo`. A GitHub account name is read live from `/user` with the connected token kept out of the answer model. Requests with no exact file or account source say what cannot be verified and ask for a narrower source; they do not trigger a broad search.
- The Audio/Privacy settings expose **Temporary STT diagnostics**, off by default. A native capture saves 16 kHz PCM16, raw STT text, turn statistics, corrected text, and selected intent in one AES-256-GCM sealed local record. The key comes from the OS credential store; capture fails closed if that key is unavailable. A sweep at startup and every minute removes records at the 24-hour retention boundary and caps the set at 100 clips. Both settings views offer immediate deletion. The list IPC returns text/statistics only, not the raw clip.

## Verification and release bar

`node --test scripts/build-provenance.test.mjs` tests missing records and source, bundle, and executable changes. `node nexus.mjs start` and `pwsh -File scripts/run.ps1` both refused the currently unrecorded release. `cargo check`, TypeScript, targeted tests, and a fresh release build must be assessed against concurrent working-tree changes. A labelled hardware replay and spoken Windows walkthrough are still required before calling speech recognition, visual callouts, or release behavior production ready. The pass condition is correct intent **or clarification**, with no wrong action; recognition and clarification rates must be reported separately. Groq `no_speech_prob` is not used as an action gate.

## Manual validation

1. Finish editing, run `nexus build`, then `nexus start`. Confirm the printed identity/path matches `src-tauri/target/release/nexus-build.json`.
2. Change a source file after the build. `nexus start` must refuse before killing a running app. Restore it and explicitly rebuild with `nexus start --build`.
3. Open Brave to a normal page whose address bar is unavailable to NEXUS. Say “analyse my screen.” Verify screenshot, sidebar, callout positions, captions, orb, and speech timing. Switch focus during capture and verify a specific retry response. Try a blocked banking/password/wallet window and verify denial.
4. Turn diagnostics on, record labelled speech, quiet-room silence, background media, NEXUS playback, and difficult installed app/repository names. Check `stt_diagnostics_list` for the raw/corrected/intent trace and `stt_diagnostics_delete` for immediate removal. Turn diagnostics back off.
5. Ask “Which app am I using?”, “What is my current repo?”, a device file question, and a general knowledge question. Confirm only current evidence supports device claims and an unavailable source produces an explicit inability to verify.

This change does not claim universal 100% STT accuracy or completed hardware validation.
