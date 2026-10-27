# P1 native capture accounting — live checklist

Status: not run. Requires a fresh release from the assessed P1 source and a matching provenance record. Earlier owner tests cannot be reused as this phase's live proof.

## Prepare

1. Record source revision, source hash, executable identity, launch UTC time and unique log paths. Use that executable for the entire session.
   Explicitly launch with `RUST_LOG=info,capture_audit=info`; do not inherit a warn-only shell filter. Confirm actual `[CAPTURE]` records before accepting lifecycle proof. A log with no audit records is unavailable evidence, even when ordinary stdout traces exist. Wait for startup speech to finish and identify automatic captures separately from the labelled physical start.
2. Start from ordinary idle mode, with automatic hot-mic sessions inactive for this batch. Record the effective native Voice Lock state; a visual draft toggle is insufficient.
3. Keep enrollment/calibration clips outside this acceptance sample. Keyboard recovery is Ctrl+Shift+S. A keyboard capture still requires owner verification when the lock is on.
4. Save a trial ID and condition before each physical start. Do not speak the trial ID as part of the command. After each turn, identify its [CAPTURE] request, started session and terminal outcome before continuing. If no native request appears, retain a missing-start failure.

## Owner batch

Use ten fresh, single-utterance commands, one physical Ctrl+Space start per trial. Suggested harmless command: Open Notepad. Five trials at normal position, five about one metre away. Wait for the previous turn to finish before the next; record retries as new trials.

| Trial | Condition | Physical start time | Run/attempt/session | Owner verification | Transcript faithful? | Requested action observed? |
|---|---|---|---|---|---|---|
| O01 | normal position | pending | pending | pending | pending | pending |
| O02 | normal position | pending | pending | pending | pending | pending |
| O03 | normal position | pending | pending | pending | pending | pending |
| O04 | normal position | pending | pending | pending | pending | pending |
| O05 | normal position | pending | pending | pending | pending | pending |
| O06 | about one metre | pending | pending | pending | pending | pending |
| O07 | about one metre | pending | pending | pending | pending | pending |
| O08 | about one metre | pending | pending | pending | pending | pending |
| O09 | about one metre | pending | pending | pending | pending | pending |
| O10 | about one metre | pending | pending | pending | pending | pending |

These are empty protocol rows, not evidence of attempts or outcomes.

## Lifecycle cases — separate from accuracy counts

- Cancel before speech, then start another capture: old attempt closes, new session differs, no stale transcript/action.
- Duplicate native start during the duplicate window: duplicate request has its own terminal, active capture keeps its original buffer/session. The physical key binding may cancel rather than issue a duplicate; do not infer a duplicate test from a cancellation.
- Start and stay silent: no-speech/phantom outcome is present and no executable action occurs.
- Start enrollment then attempt a normal capture: microphone-held outcome, no assistant capture, and recording/wake ownership recovers afterward.
- Cancel while transcription is in flight: stale/cancelled result, no later transcript delivery, new capture receives a different session.
- Receiver/model fault cases require a controlled test harness; do not corrupt installed assets merely to produce a transport error.

## Reconcile and cross-check

```text
python scripts/capture_audit_summary.py <saved-native-log> --run-id <run-id> --expected-starts 10
```

Require a zero exit status and no orphaned, duplicate, reused-session, late-stage or unexpected-dispatch records. Separate native requests from actual starts: blocked and duplicate requests close without starting a new capture. Compare the ten physical starts to the captured native evidence; extra hot-mic requests or missing starts need explanation.

Inspect routing receipt by run/session, plus actual visible behavior. Delivered means native transcript emission, not action success. A complete lifecycle report is not a speaker-verification acceptance result.

Keep source/build identity, anonymized trial rows, the summary, ordered log excerpts, and every failure in the live-proof ledger. Do not raise the score, move to P2, or advance the feature queue on incomplete evidence.
