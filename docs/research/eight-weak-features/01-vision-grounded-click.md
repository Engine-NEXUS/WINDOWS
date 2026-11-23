# 1. Vision-grounded click ("click the Submit button")

Scorecard before: Built 85, Tested 66, Live 25, Safety 70, Industry 40 (2.5 stars).

## What it is

"Click X" first looks X up through Windows UI Automation (exact, free, a few ms). Only when UI Automation cannot see it, NEXUS takes a screenshot of the **whole primary screen**, draws a coordinate grid on it, sends it to Gemini or Groq, and clicks the point the model names. Files: `vision.rs` (capture, grid, providers, quota), `orchestrator.rs::run_click_element` (the voice path), `live/commands/mouse.rs` (UIA resolver and the click).

## Verification (what was found)

Baseline: 81 vision tests pass. They cover grid maths, coordinate parsing, quota and provider order. **None** covered what is sent or when the click lands.

| # | Finding | Evidence | Severity |
|---|---|---|---|
| 1 | A picture of the whole screen is sent to a cloud model with **no check of which window is showing**. A bank, password manager or wallet window in front is uploaded like any other. The screen tour has such a gate (`screen_context::capture_block_reason`); this path did not | `vision.rs:1007` captures, `orchestrator.rs` `run_click_element` had no gate | High, privacy |
| 2 | In "race" mode (`visionRace: speed`, an opt-in) the same image goes to **both** providers | `vision.rs:1011-1020` | Medium, privacy (owner's choice, but undocumented) |
| 3 | The cloud call takes seconds, then the click is sent **without looking at the foreground again**. If the window changed meanwhile, the coordinates belong to another window | `orchestrator.rs` before the fix | High, wrong action |
| 4 | "Clicked X, sir." was spoken when `SendInput` returned. For a vision-found target that is a guess, not a press | same | Medium, false success |
| 5 | Only the primary monitor is captured; a target on a second monitor gets the wrong coordinates | `vision.rs` capture | Medium, open |
| 6 | Race mode takes the first answer with no agreement check | `vision.rs:1035` | Low, open |
| 7 | `click_element` (unlike `system_click_element`) is not capped as an ML-only label | `orchestrator.rs:673` | Low, open |

(The advisor, Opus 5.5, confirmed 1 to 4 and added 5 to 7. It also corrected a mistake in my first read: the Ghost-mode click in `mouse.rs` no longer uses vision, so the real path is `run_click_element`.)

## What was built

In `run_click_element`, on the vision branch only (UI Automation hits are unchanged and never touch the network):

1. **Freeze and vet the foreground window first** with the screen tour's own `collect_target` and `capture_block_reason`. A sensitive window (the `live::safety` deny-list: banks, password managers, wallets, and so on) or one that cannot be identified is refused, aloud, **before anything is sent**. NEXUS says "…I did not look for it, sir."
2. **Recheck after the cloud call, before the click** with `foreground_still_matches`. If the window changed, nothing is clicked: "The window changed while I was looking, so I did not click, sir."
3. **Honest wording.** A vision-found click says "I clicked where X appears to be, sir." A UI Automation click still says "Clicked X, sir."

## Verification of the fix

`orchestrator::vision_click_gate_tests` (3 tests):

* order of operations in the source: freeze, then vet, then locate, then recheck, then click (a source scan, because the path needs a real screen and a cloud key);
* wording: a vision click is never reported as a confirmed press;
* the shared gate itself: a bank window, a password manager and an unidentified window are refused; Notepad is allowed.

Mutation checks (rule removed, a test must fail): vet removed, recheck removed, wording reverted. Results are in `docs/testing/runs/eight-weak-features-mutations-20261124.json`.

## Not done, and why

* Items 2, 5, 6, 7 above. Item 5 (a second monitor) needs a real second monitor to verify. Item 2 changes an owner-chosen setting: the Hub should say plainly that race mode sends the screen to both providers.
* The measurement harness (UI Automation ground truth against the vision model) **was not run**: it uploads a screenshot of the owner's screen to a cloud provider. It needs a clean desktop and the owner's say-so.

## Owner checklist (what moves Live-proven)

1. Gemini key set, a plain Notepad window in front. Say "click the File menu" on a window where UI Automation cannot see it (a game or canvas). Log: `vision click: fallback=… raced=…`; NEXUS says "I clicked where File appears to be".
2. Bring a password manager or a banking tab to the front and say the same. Expected: refused aloud, **no** `vision click` log line and no network request.
3. Say "click X", then switch window while it thinks. Expected: "The window changed while I was looking…", no click.
4. Run the measurement harness on a clean desktop (docs in `vision.rs` tests, `ignored` test).
