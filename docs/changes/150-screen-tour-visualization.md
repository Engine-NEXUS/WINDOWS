# 150 — Narrated screen-tour visualization (2026-10-20)

## Intent

Preserve NEXUS's existing one-item-at-a-time narrated tour while making its explanation easier to follow. The animation must be evidence-linked to the target box and live only for the spoken item. The sidebar screenshot and cards remain the durable result.

## How the existing tour draws

Rust `TourEngine` receives speech playback `Started(i)`/`Ended(i)` receipts. `Show(item)` emits one `screen:callout` with the current item's physical-pixel box, and `Clear` removes it before another item is shown. The React stage converts that box to CSS pixels, places a callout clear of the target and orb, draws a ring plus leader/arrow, and registers only the card as an interactive hitbox. At the end, Rust opens the sidebar with the analyzed screenshot and same-frame item crops. This flow continues to own speech, orb, cancellation and turn closure.

## Added in this pass

| Model category | Added motion | Visual purpose |
| --- | --- | --- |
| Person, product, food, object | Two restrained focus ripples | Draw attention to an image or object without hiding it |
| Text block, code | Short travelling underline | Guide the eye along the visible text region |
| Chart, diagram | Travelling border stroke | Trace the extent of a visual structure |
| UI element | Travelling leader beam | Connect a small control to its explanation card |
| Unknown, missing/large target | Existing ring, leader and card | Avoid meaningless or expensive animation |

The model's category is carried in the existing current-callout event. The stage uses an allowlist to choose a motion; unexpected categories fall back to the original style. Model boxes are clipped to the visible stage and malformed or tiny boxes lose their pointer. A late event after `tour_end` cannot revive an old callout. Viewport resize triggers placement again. Only the active item animates, and `prefers-reduced-motion` removes every new effect while keeping the static ring, leader and text. A high-contrast mode uses stronger static borders.

The tour prompt now asks for a direct answer, a useful reason for each pointed item, varied sentence openings, and visible-only relationships. The existing word limits, one spoken sentence per item, person-grounding rule and speech receipts remain intact.

The travelling border, line and ripple are **design adaptations** of [Magic UI's Border Beam](https://magicui.design/docs/components/border-beam), [Animated Beam](https://magicui.design/docs/components/animated-beam) and [Ripple](https://magicui.design/docs/components/ripple). No Magic UI package, source component or new animation runtime was added. The implementation uses NEXUS's stage SVG and CSS so it works with the stage CSP and does not load a model or animation framework at idle.

## Verification and remaining checks

- Frontend TypeScript, 25 focused geometry/tour-flow tests, and the production frontend build passed on 2026-10-20. The build reported its existing Vite/lottie warnings; no new build error appeared.
- All 33 `screen_tour::tests` passed after an unrelated `architect.rs` test-fixture compile blocker was repaired by adding its two existing `KeyFile` metadata fields to five fixtures.
- A debug desktop executable was rebuilt at 2026-10-20 19:05:37 local (`D:\ULTRON\src-tauri\target\debug\nexus.exe`, SHA-256 `0E7A38ADEA43EC89A1CD74958CBF430D6D1B9A2794E9384638BB79BBB9E4CE0F`). Its first launch reached stage ready. This is a manual candidate, not an installer. The first launch log is `%APPDATA%\com.nexus.assistant\nexus_debug_2026-10-20_tour_motion.log`; stderr includes the previously seen missing-registry-key message, which has not been linked to the tour.
- A synthetic 1366×768 Chrome rendering of the stage CSS showed the target ring, leader, ripple and card readable over a light sample page. It does not prove Tauri event timing or display scaling.
- A live voice tour is still required. Verify each category against a real page/app, target alignment at display scaling, card/orb collision, reduced motion, cancellation/barge-in, speech/callout timing, and sidebar continuity. Do not call the visual upgrade production-ready until that walkthrough passes.

## Rebuild and cross-check before manual testing (2026-10-20)

- Rechecked the tour source diff and targeted `git diff --check` with no whitespace errors. A whole-worktree check flags unrelated whitespace in `frontend/src/pr-list/`. The debug executable above is newer than the final `screen_tour.rs` and `tour.css` edits, and Vite serves the current stage on port 5173 (HTTP 200). Its SHA-256 still matches the recorded build.
- Re-ran TypeScript, 25 focused frontend geometry/orb-flow tests, all 33 Rust `screen_tour::tests`, the production frontend build, and the entire frontend suite (371 tests across 45 files): all passed. The turn-end gate passed.
- Repository-wide hygiene, IPC-contract and dead-code gates failed on files outside this tour: `agent_followup.rs` contains a fake private-key fixture that hygiene flags; IPC reports existing command/event inventory drift; dead-code reports `agent_followup`, `tts` and `vision` items. These failures were not cleared or hidden as tour successes.
- An additional `cargo build --release --features custom-protocol -j 1` was attempted after the frontend build. It was stopped during prolonged Windows optimization under concurrent Rust jobs; **the older release executable was not used**. Manual testing should use the live debug candidate above. This is a source build, not an installer rebuild.
- The long-running debug session logged repeated microphone-silence warnings, so the exact rebuilt binary was restarted for a clean manual run. The new process is PID 13644; all preflight layers passed and `stage ready after 1s` appeared. New logs: `%APPDATA%\com.nexus.assistant\nexus_debug_2026-10-20_tour_manual.log` and `nexus_debug_2026-10-20_tour_manual_err.log`. The previously seen registry-key stderr line remains. Microphone response to speech needs the live walkthrough.
