# Owner-operated Windows desktop-control qualification

## Boundaries

Use only a fresh, provenance-checked build after full native library/integration and repository checks. The assistant did not run the application. Do not treat the isolated Rust helper or CI configuration as Windows device evidence. Preserve personal/contributor separation; select the intended profile explicitly. Never copy personal data to a contributor profile for this test.

Keep normal Voice Lock policy, protected-window restrictions and Escape recovery enabled as appropriate. A failure in a speech/identity gate is recorded separately and never bypassed to improve the desktop score. Windows Settings cases navigate only: do not change actual settings. Elevated/password/destructive controls belong in disposable fixtures or safe permission-rejection checks.

## Prepare

1. Record source revision plus the assessment's source-manifest hash, release/build identity, Windows version, profile, architecture, app versions, monitor bounds and scaling. A dirty checkout requires its complete manifest; a commit ID alone does not identify it.
2. Open [fixture.html](fixture.html) locally in Edge. It makes no network requests. Reload it between independent cases so dialog/toggle state is known. Its two Open buttons are under different named panels; activation opens a disposable named Open dialog and identifies the chosen button. Tools reveals a menu after 750 ms.
3. Create a disposable folder containing Alpha and Beta directories and One.txt and Two.txt. Open that folder in Explorer. Use disposable, unsaved Notepad documents. Begin with one intended target window per focus test; multiple windows are deliberately ambiguous rather than an arbitrary first match.
4. For browser tab tests open at least three local tabs and record the starting order. Start the scroll test away from either limit, with the intended pane focused. No scroll change at an edge is Unconfirmed, not success.
5. Keep recovery hotkeys available. Stop immediately on a wrong-target or unauthorized action; preserve the result and investigate before more executable tests.

## Fixed cases

The authoritative list is [matrix.json](matrix.json): Edge 8, Explorer 8, Notepad 8, Settings 6. Execute ten complete rounds, retaining case IDs and attempt numbers 1–10. Do not replace a difficult case with an easier one or delete a missing capability from the denominator.

Use the exact supported Ghost phrasing, such as `click Open button in Toolbar`, `click Open button` followed by `choose two`, and the existing tab/scroll/dictation routes. A sidebar click selects a current token; it does not provide coordinates. Confirm the original app is restored rather than acting on an unrelated foreground window.

For each run record the actual receipt and the intended effect independently. A sent shortcut or an audible “OK” alone is insufficient. If the provider exposes no trustworthy selection/value/menu/scroll state, record Unconfirmed. Do not keep issuing the same action to obtain a pass. Reset manually between independent trials.

Run the separate 20 held-out scenarios after freezing implementation and core cases. Their expected outcomes are declared in the matrix. Verify correct refusals, delayed effects, process restart/handle reuse, duplicate and stale selections, focus loss, cancellation, Unicode replacement and covered/moving controls. Record unavailable physical configurations as blockers. Mixed scaling and a negative-coordinate second display require the actual hardware; simulation alone does not qualify them.

Finally repeat the 30 core tasks once with the owner's voice, using the usual supported wake/hotkey path. Keep the exact recognized command and failure stage in a local restricted evidence record if needed; the structured report stores only an evidence reference. This separates speech from desktop performance. Non-owner Voice Lock qualification remains the independent speaker-verification programme.

## Result format

Save a JSON array locally. Each row uses this shape; the values below are an **example, not trial evidence**:

```json
{
  "sample_id": "core-edge-01-1",
  "split": "core",
  "case_id": "edge-01",
  "attempt": 1,
  "source_sha256": "REPLACE_WITH_64_LOWERCASE_HEX_SOURCE_MANIFEST_HASH",
  "build_identity": "REPLACE_WITH_RELEASE_ID",
  "platform": "windows",
  "input_mode": "text",
  "app_version": "REPLACE_WITH_OBSERVED_APP_VERSION",
  "display_configuration": "REPLACE_WITH_MONITORS_AND_SCALING",
  "outcome": "unconfirmed",
  "effect_observed": false,
  "failure_stage": "effect_verification",
  "evidence_ref": "REPLACE_WITH_LOCAL_LOG_OPERATION_REFERENCE",
  "wrong_target_actions": 0,
  "unauthorized_actions": 0,
  "dispatches_after_cancel": 0,
  "latency_ms": 0
}
```

`split` is core, held_out or voice. Held-out and voice attempts are 1; voice uses input_mode voice, other splits use text. Held-out rows also require `expectation_met: true|false`, assessed against that case's predeclared safe outcome. A blocked safety case can pass its held-out expectation without claiming an executable action succeeded. Other outcomes: verified, blocked, stopped, failed, unavailable. Failure stages: none, speech, interpretation, target_resolution, dispatch, effect_verification.

Do not put document text, passwords, screenshots, window titles or typed text into structured diagnostics. Use operation IDs to associate independently reviewed evidence. Failed and missing runs remain visible. Do not mix sources/builds in one assessment.

From the repository root, after recording actual results:

```text
python scripts/desktop_control_trials.py --results PATH_TO_ACTUAL_ROWS.json --output PATH_TO_REPORT.json
```

The reporter validates provenance, duplicate attempts, finite timing, safety counts, input mode and fixed denominators. It reports observed core completion and a 95% Wilson interval. It always leaves `production_ready` false: full native/release/device/weighted-score gates require separate review. An empty run is [qualification-status.json](qualification-status.json), not a passing test.

## Known limits to examine

- UIA providers may block inside a COM call. Outer choice/Ghost waits expire at 20 seconds and cancel late work; a dispatched OS action is not reversible by cancelling its acknowledgement. Pre-dispatch checks and per-effect deadlines reject late observations, but a hard-preemptible COM process is not part of this release.
- Focus monitoring samples every 250 ms while a clarification is pending. Very brief away-and-back switches between samples need investigation; exact foreground/process identity is rechecked before selection/dispatch. This is not continuous OS-event proof.
- Typing binds the window and field immediately before insertion. It does not yet bind the editor at the beginning of a long speech capture. A user switching editors during transcription remains a qualification concern; do not claim capture-to-insertion target continuity.
- App focus requires an exact executable and one eligible top-level window. Multiple windows ask for a more specific workflow; arbitrary window ranking is not introduced. Older Settings hosted-window variants are unqualified.
- A generic accessible Invoke is verified only when the expected named surface appears. Controls with unsupported state information stay Unconfirmed. Provider support and sidebar visuals have not been device-tested.

Append actual runs to the live-proof ledger only after they happen. Attach failures as well as successes. Update the feature work card and score only after its evidence gates pass; do not advance the production queue on these offline checks.
