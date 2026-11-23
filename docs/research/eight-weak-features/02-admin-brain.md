# 2. Admin brain (Qwen 0.5B, admin build only)

Scorecard before: Built 80, Tested 30, Live 35, Safety 70, Industry 50 (2.5 stars). Out of scope for the family build (compiled out); scored for the admin build.

## What it is

A local Python sidecar on port 39219 runs Qwen2.5-0.5B. When the deterministic parser misses, the orchestrator asks it to classify the sentence; its label can become a command. It also generates phrasings that retrain the small NLU model (`brain_monitor.rs`). Gates: the `admin-brain` cargo feature **and** `admin.json` with `is_admin: true`.

Files: `brain_client.rs`, `lazy_brain.rs`, `brain_monitor.rs`, `admin_config.rs`, `orchestrator.rs::parse_with_ml`, `command_center.rs`.

**`server/admin/brain_server.py` is not in the repository** (only on the owner's disk), and no Python is installed on this machine: the server itself could not be read or run for this work.

## Verification (what was found)

Baseline: 10 brain-client tests, 4 admin-config tests, 6 monitor tests, 0 for `lazy_brain.rs`. All pass.

| # | Finding | Evidence | Severity |
|---|---|---|---|
| 1 | In an "X then Y" plan the brain's label was accepted for each step with **no confidence gate, no slot sanitising and no cap on the labels the model invents**. The orchestrator itself recorded this model labelling "switch to brave" as `start_dictation` at 1.0 and "click submit" as `screen_analysis` at 0.9 | `command_center.rs:513` (before), `orchestrator.rs` comments on `cap_ml_window_open` | High |
| 2 | `admin.json`'s `min_confidence` (default 0.90) is **never used**; the orchestrator compares against a fixed 0.5 | `orchestrator.rs:604`; the only reader of `min_confidence()` was its own test | Medium |
| 3 | Any process listening on port 39219 is trusted as "the brain" (a TCP connect is the only check), and its output is routed as commands | `lazy_brain.rs:156-159` | Medium |
| 4 | Three requests used the constant 39219, one used `admin.json`'s port: a configured port broke phrasing generation and health checks | `brain_client.rs` | Low |
| 5 | `brain_generate_phrasings` had no admin check | `brain_client.rs` | Low |
| 6 | `BRAIN_RUNNING` was load-then-store: two concurrent callers could both spawn a server; and it stayed true if the start failed | `lazy_brain.rs` | Low |

## What was built

* `orchestrator::accept_brain_result` is **one gate for both places** a brain label can become a command: sanitise slots, cap the known-invented labels, then require the confidence floor. `command_center` now uses it for every step; a step that fails the gate abandons the plan and the sentence falls back to the normal single path.
* `orchestrator::brain_floor`: the configured `min_confidence`, never below the old 0.5, garbage falls back to 0.90. **Behaviour change:** with the default `admin.json` the floor rises from 0.5 to 0.90. Set `"min_confidence": 0.5` to get the old behaviour back.
* One port source (`admin_config::brain_port()`) for every URL; admin check on phrasing generation; the start is claimed with `compare_exchange` and released on every failed start.

## Verification of the fix

`orchestrator::brain_gate_tests` (5 tests): floor maths; an ordinary confident label passes and a low one does not; an invented window-open and an invented `system_click_element` are rejected **at confidence 1.0**; source scans that the single path uses the configured floor, the compound path uses the shared gate, every brain URL uses the configured port, phrasing has an admin check, and the start is atomic. Five mutations cover these (see the index).

## Not done, and why

* **Finding 3 (trust handshake)** needs a change in `brain_server.py` (a per-launch token the client must present, or an identity in `/health`). That file is outside this repository. Until then the compound and single gates are the mitigation: a rogue listener can only emit labels that pass sanitising, the cap and the floor, and every destructive intent still asks for confirmation.
* The 0.5B model's confidence is poorly calibrated (see the measured cases above), so a higher floor is a weak protection; the label cap and the confirmations are the real ones. Measuring the model's calibration needs the real server and the owner's phrase set.

## Owner checklist

1. With `admin.json` present: say "open settings then open the architect". Log shows `orchestrator: brain: …` and no step runs below the floor.
2. Say a sentence the brain mislabels ("switch to brave then …"). Expected: no click, no dictation start; the sentence falls back.
3. If brain commands feel stricter than before, lower `min_confidence` in `admin.json`.
