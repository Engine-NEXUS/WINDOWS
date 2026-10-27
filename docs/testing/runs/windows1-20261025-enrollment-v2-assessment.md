# Enrollment wake isolation and pending-permission cancellation — 2026-10-25

Status: **in progress**. Speaker verification remains 41% (2.05/5). Replacement 031caf5848621e18 is running as PID 28248; no new owner accuracy proof exists.

## Source

- Snapshot: `50dcda219018d3bbfff8be388b77610bee135749`.
- Build-input SHA-256: `086e534ae829a7abbdf1e793f584297e81b8f2c8c16625bcfeee03ea505572c4`.
- Ref: `refs/codex/readiness/windows-1.0-enrollment-fix`.
- Checkout: `C:\Users\Chitkul Lakshya\.codex\worktrees\windows-readiness\ULTRON`.
- Primary D: checkout, branch/index and unrelated edits remain preserved.

## Failures retained

- Owner reported that enrollment on release `812a5b3333669e02` also woke NEXUS. The wake listener had never been paused by the recording UI.
- The first fix passed 655 frontend and 2,524 library + 19 integration tests, but a new pending-permission regression then failed: [red test output](windows1-20261025-enrollment-fix-permission-red.log). Navigation could wait indefinitely on getUserMedia before restoring wake listening. This was a controlled unit reproduction, not a real Windows permission-dialog walkthrough.
- The optimized release attempt for that revision was stopped before completion and never launched. Its build output is retained separately. Those earlier passes do not count as final proof of this revised snapshot.

## Revised behavior

The ten-clip workflow awaits native microphone pause before countdowns, suppresses wake/capture dispatch, records Nexus / Hey Nexus / OK Nexus in both five-clip halves, stops browser tracks before native resume, and reports device errors. Cancellation now races the recording task so pending browser permission cannot hold the native baton. The existing stream-acquisition check stops any stream granted after cancellation. Promise handlers consume late completion/errors; cancelled recording cannot continue to profile submission. Native pause/resume operations run off the UI event loop.

## Checks

| Check | Result | Evidence |
|---|---|---|
| Frontend TypeScript / build / tests | Pass: 656 tests, 67 files; TypeScript/build pass | [Frontend log](windows1-20261025-enrollment-v2-frontend.log) |
| Worker TypeScript / tests | Pass: 145 tests and TypeScript | [Worker log](windows1-20261025-enrollment-v2-worker.log) |
| Python / NLU / provenance regression | Pass: 48 tests, 512 eval rows / 8 sources and provenance regression | [Python/NLU log](windows1-20261025-enrollment-v2-python-nlu.log) |
| Safety mutations | Pass: 15/15 killed, including pending-permission cancellation | [Mutation log](windows1-20261025-enrollment-v2-mutations.log) |
| Complete Rust library / integration | Pass: 2,524 library + 19 integration; zero failures; 38 ignored | [Native log](windows1-20261025-enrollment-v2-rust.log) |
| IPC / turn-end / hygiene / opacity / whitespace | Pass: 236 commands /176 invoked; 60+6 known IPC items, 132 turn-end files, 1,574 hygiene files, 12 CSS surfaces; whitespace pass | Repository guards |
| Dead-code / isolated latency / OS keyring | Pass: 148 known findings, none new; 1,193 ms /5,000 ms budget; reserved probe persisted and deleted across two processes | [Dead-code](windows1-20261025-enrollment-v2-dead-code.log), [latency](windows1-20261025-enrollment-v2-latency.log), [keyring](windows1-20261025-enrollment-v2-keyring.log), [test executable identity](windows1-20261025-enrollment-v2-test-binary.json) |
| Fresh release / provenance | Pass: optimized release 031caf5848621e18; provenance before and after startup | [Release build](windows1-20261025-enrollment-v2-release-build.log) |
| Owner live re-enrollment / accuracy | Not run | Required before score or queue advancement |

The frontend bundle is built before native compilation begins. No additional frontend build is scheduled during native compilation. The isolated latency and key-store probes use the assessed test executable directly to avoid a redundant Cargo recompilation.

## First owner recheck

1. Open Command Hub with Ctrl+Shift+S, then Audio. Keep Voice Lock off for enrollment; choose Re-enroll Voice and complete ten clips.
2. For Nexus / Hey Nexus / OK Nexus in both halves: no wake cue, orb, ordinary capture, reply or executable action during enrollment. Report any interruption and clip number.
3. Wait two seconds after completion, then check ordinary waking with each phrase separately. Cancel the resulting turns without asking for executable actions.
4. Start enrollment again and navigate away during a countdown and during recording; browser recording must stop and normal listening must recover. Also check navigating away while permission is pending, if a real permission prompt can be exercised safely through the normal UI.
5. Preserve all errors and failed cases. Repeat the existing held-out owner/non-owner/TV and restart/profile-recovery protocol only after enrollment interaction passes. Calibration clips do not count as acceptance trials.

Full acceptance still requires at least 95% owner acceptance and zero observed executable non-owner actions in at least 300 measured negatives, with uncertainty and source identity recorded. The queue stays on speaker verification.

The v2 library suite completed in 233.24 seconds after compilation. Both native enrollment regressions passed. Three test-target warnings (`WINDOW_STYLE`, `facts`, and `still_missed`) are retained in the native log; no zero-warning claim is made.

The initial release command waited on the shared D: Cargo artifact lock held by a separate `nexus build`. Only this assessment's waiting command was stopped. Immutable non-NEXUS dependency artifacts are being copied into the managed checkout's own target directory; the other build is left running. The replacement release will use that isolated cache with one Cargo job. The latency probe ran as a separate test process while the other build was active; it is not claimed as a quiescent-hardware benchmark.

The isolated dependency cache copied 7,358 non-NEXUS entries in 51.5 seconds, excluding own-package artifacts and the stage executable/resources. Its first build failed with OS error 32 because the old stage resources junction pointed to the source directory. Only that verified junction was removed with nonrecursive directory deletion; the source model remained present and its hash was unchanged. The full frozen input hash was rechecked afterward and still matched `086e534ae829a7abbdf1e793f584297e81b8f2c8c16625bcfeee03ea505572c4`. Cargo recreated a physical resource folder. The failure output is retained in `windows1-20261025-enrollment-v2-release-stage-junction-error.log`. The active release uses standard `cargo build --release --bin nexus --features custom-protocol,admin-brain -j 1` with this checkout's own target directory.

All seven shipped OWW inputs named by `scripts/build-provenance.mjs` match source and physical stage hashes; see [runtime asset proof](windows1-20261025-enrollment-v2-runtime-models.json). The initial unfiltered comparison also included three non-shipped development backup/training artifacts; their absence is expected and the raw comparison is retained separately.

## Verified replacement

Replacement release 031caf5848621e18, source 50dcda219018d3bbfff8be388b77610bee135749, is provenance-verified before and after launch as PID 28248 at 2026-10-25T11:19:07Z. Optimized build completed in 49m 15s. All shared gates pass: 2,524 Rust library + 19 integration (38 ignored), frontend 656 and TypeScript/build, Worker 145 and TypeScript, Python 48, NLU, repository guards, 15/15 safety mutations. DirectML hash checked. Startup reached stage ready with microphone/wake models detected. Missing optional Tier 3 command models, Groq menu-model, registry and startup wake-suppression warnings are retained. Owner re-enrollment and accuracy remain pending; score stays 41% (2.05/5).

Evidence: [release identity](windows1-20261025-enrollment-v2-release.json), session and filtered startup logs in the same run directory.

## Owner enrollment interaction result

Owner live report on candidate 031caf5848621e18: all ten enrollment clips completed without waking NEXUS. Enrollment wake isolation passes this interaction check. Normal wake recovery, cancellation recovery, owner/non-owner accuracy and profile recovery remain pending. These ten calibration/enrollment clips count as zero acceptance trials. Score remains 41% (2.05/5); queue stays on speaker verification.

### Post-enrollment wake recovery

Owner confirmed normal waking with Nexus, Hey Nexus and OK Nexus after the ten-clip enrollment on release 031caf5848621e18, with Voice Lock off. Enrollment isolation and post-completion wake recovery have passed owner interaction checks. Cancellation recovery and locked owner/non-owner/TV accuracy remain pending. No accuracy trials counted; score stays 41%.

### Cancellation interaction result

Owner reports both enrollment cancellation checks passed on 031caf5848621e18: navigation during countdown and during recording stopped recording and restored normal waking, with Voice Lock off. Pending-permission navigation is not live-verified. Locked owner/non-owner/TV accuracy, restart and profile recovery remain pending. Score stays 41%; no accuracy trials counted.

## Live build attribution correction

Build attribution correction: the owner supplied console logs showing nexus start replaced the assessed candidate with D: release 6f4e1063a38683a3 at 16:54, followed by 3d1932b68b2c0f15 at 17:36. Earlier owner enrollment/wake/cancellation reports remain observations, but exact assessed-build attribution is unconfirmed and must be repeated. The Notepad turn at 17:37:41 was owner=unenrolled with Voice Lock off; it is excluded from locked accuracy counts. Current device setting was confirmed false; a profile file exists. Candidate 031caf5848621e18 was restored with unique logs, PID 28952. No score increase or queue advancement.

### Voice Lock draft/save observation

Owner reported toggle on/no executable result. Readback found speakerVerification=false and the running process was D: build 3d1932b68b2c0f15 (PID 17928). Command Hub toggles are drafts until Save; the assistant's test instructions omitted that step. Exclude this attempt from locked accuracy counts. Save and verify the native setting before another trial; assessed-build attribution must also be resolved. No owner rejection or acceptance rate established.

### Native lock enable readback

Owner reported Saved successfully, but live Tauri IPC voice_lock_enabled and get_settings both returned false; a valid five-clip profile with all three phrases loaded. With existing authorization to enable for controlled trials, settings_patch({speakerVerification:true}) succeeded and both native readbacks returned true. This proves current lock configuration, not speaker accuracy. Current D: release 3d1932b68b2c0f15 / PID17928 is being used for diagnosis only; it does not inherit frozen-source suite proof. The UI/save discrepancy remains unresolved; no user misclick or UI defect is assumed.

### Diagnostic owner acceptance

Diagnostic D: build 3d1932b68b2c0f15 recorded an owner-verified captured turn at 2026-10-25T12:44:35Z (similarity 0.944), after native Voice Lock enable. Intended Open Notepad was transcribed as open over open load pad; requested app-open success is not established. This is one owner-verification acceptance, not broad accuracy or frozen-source proof. Owner is alone; owner trials can continue, other-speaker playback must be labelled separately, and live non-owner evidence remains unavailable. Score stays 41%.

### Solo owner batch

Owner reported ten solo attempts (five normal, five about one metre). In 20:07:00–20:09:00 IST, seven completed captures are identifiable, all owner=verified; three reported attempts remain unmatched and condition-by-condition mapping is unavailable. Do not claim 10/10 or a formal owner acceptance rate. One capture was repeat-loop filtered, several app targets were duplicated, and one transcript became hook up opencode. Requested app-opening success is not established for the batch. D: build3d1932 diagnosis only; zero negative trials; score stays41%.
