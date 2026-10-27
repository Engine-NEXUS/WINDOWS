# 146 — Screen analysis images and foreground recreation boundary (2026-10-19)

## Built in this pass

- The existing narrated screen tour and spatial fallback attach the analysed screenshot to their sidebar payload. Each item card also receives a small JPEG crop made from the **same captured frame**; no later recapture can pair a different person or object with a card.
- The spatial sidebar displays that frame above the written overview and the per-item crops inside their cards. Images stay in the in-memory Tauri payload and are not persisted by this change.
- Vision prompts now describe people using visible evidence, with names only where the screen explicitly connects a label to a person. The previous prompt invited identification from facial likeness, which could turn a guess into a confident label.
- [Feature 146](../features/146-foreground-review-and-recreation.md) records the owner's two-command design: review first, then “recreate it” using the saved review. It also records the local-device REA boundary and the remaining work.

## Foreground capture hardening (follow-up)

- The Windows analysis entry now identifies the foreground app, title, optional URL and OS window/document rectangle before narration. Unknown, sensitive and unbounded targets stop before OCR or vision capture. A complete-URL hash detects a changed query without placing private query text in the vision prompt.
- OCR, spatial vision, and plain vision capture the verified foreground region. The narrated tour no longer widens a failed crop to the whole monitor. Focus is checked again before capture and before displaying spatial callouts; a changed target asks for a retry.
- `screen_context` has a pure policy test for unknown, missing rectangle, sensitive, changed tab/URL/query and moved window. Feature 146 has a seven-gate release checklist with evidence and current state.
- These checks reduce accidental capture of the wrong screen. They do not finish the requested two-command review/recreation feature or replace rebuilt live tests.

## Verification

- `cargo test --lib screen_context::tests --features custom-protocol,admin-brain`: 15 passed after foreground policy, region scoping and URL-fingerprint changes. This compiles the current Rust library on Windows.
- `cargo test --lib screen_tour::tests --features custom-protocol,admin-brain`: 33 passed during this hardening pass, before the later fast-probe split; the final library compiled in the `screen_context` run.
- `cargo test --lib vision::spatial_tests::thumbnails_are_cropped_from_the_analysed_frame --features custom-protocol,admin-brain`: 1 passed.
- `cargo test --lib screen_tour::tests --features custom-protocol,admin-brain`: 33 passed.
- `npx tsc --noEmit`: passed.
- `npx vitest run src/sidebar/spatialShared.test.ts src/net/screenTour.test.ts`: 14 passed.
- `git diff --check` on touched tracked files: passed; Git reported only an existing CRLF normalization warning for `orchestrator.rs`.
- REA 6.7.0 npm identity and integrity checked; installed outside the repository with scripts disabled; installed package identity and CLI capabilities/help verified. A real CDP smoke was blocked by the environment's automatic approval check.

## Manual-test build (2026-10-20)

- Added distinct deterministic `ReviewForeground` and `RecreateLastReview` intents. Both map to ArchitectCenter/LocalCommand and cannot enter compounds. Recreation is classified as destructive because it starts a coding agent.
- The review freezes and checks the foreground target, captures its region, shows the first screenshot in the spatial sidebar, and progressively adds at most six public pages. The public-page pass strips URL queries, rejects private addresses, disables redirects for HTML discovery, uses an isolated Chrome profile, and calls only REA's passive CDP commands. Each page has its capture status; unsupported and failed steps produce visible warnings. A native app produces foreground-only coverage.
- The review record has a stable ID and is sealed with the existing Memory Core process key. When that key is unavailable it remains session-only. Deletion invalidates the saved reference. The sidebar shows page images, stack claims with evidence and status, warnings, a destination-folder field, and a Start recreation button.
- “Recreate it” resolves that saved review. With a detected active coding project it starts the watched Codex adapter in that project; otherwise the user enters an existing destination folder in the sidebar. The agent runs under its existing approval preset. Starting the watch is not a claim that a recreation was completed.
- Person cards retain a same-frame crop and an unverified generic label. The validator needed to connect a visible name or sourced public fact to a person remains a release gate.

Focused checks on the current source: the distinct review/recreation parser test passed after accepting “analyze the current app”; all three public-page discovery/evidence tests and both REA boundary tests passed with `custom-protocol,admin-brain`. Frontend TypeScript, 369 Vitest tests, and the production frontend bundle passed earlier in this build pass.

The manual candidate at `src-tauri/target/release/nexus.exe` was built at 2026-10-20 12:57:25 (SHA-256 `0A5CF8F2BC93DB8AF01AD1DFC58516F1A765CCB699FA41039B0C010F8624B33B`). Its binary contains the foreground review and recreation paths. A later source-only parser fix for “analyze the current app” is **not in this candidate**; use “analyse this website” or “analyse this app” for the first trial. A fresh serial release rebuild reached the final executable replacement, then failed because another NEXUS process opened the file. A retry restarted library compilation after concurrent source edits; it was stopped and the verified candidate restored. `pwsh ./scripts/run.ps1` launched that candidate at 15:20 and reached the stage/orchestrator/audio startup logs. No voice or sidebar walkthrough has yet passed. The 2026-10-20 routing fixes recorded below — the screen-analysis sentence candidates and the foreground-review sentence/polite-lead-in handling — were **not in that release candidate**; they are compiled into the newer debug candidate described in the next section, and that release candidate has since been stopped.

### Superseded debug manual candidate (2026-10-20 16:16)

After the parser tests, `cargo build --bin nexus --features custom-protocol -j 1` completed successfully at 2026-10-20 16:16:15 local. The result is `D:\ULTRON\src-tauri\target\debug\nexus.exe`, SHA-256 `8F3199A4896AF4F23DA7B9B95453B1D999DA9DED800BB312E590729557290818`.

- This is a **debug manual candidate, not a release or installer build**. It is not the `target/release` binary, it is not what an installer ships, and no release or installer claim follows from it.
- It does contain the 2026-10-20 routing fixes — screen-analysis sentence candidates, foreground-review sentence candidates and polite lead-ins — that the 12:57 release candidate lacked.
- This executable was superseded by the 17:43 gate-fix build below. PID 26616 is no longer the running app.
- Startup reached “stage ready”. **No voice walkthrough has been performed**, so the voice, caption, orb and sidebar outcomes for this candidate remain unverified.
- Debug stdout/stderr: `%APPDATA%\com.nexus.assistant\nexus_debug_2026-10-20.log` and `nexus_debug_2026-10-20_err.log`. Vite logs, in the same folder: `vite_debug_2026-10-20.log` and `vite_debug_2026-10-20_err.log`.

## Live screen-analysis routing failure and the foreground-review follow-up (2026-10-20)

**Observed live failure.** The live transcript **“once tell the number. analyze my screen.”** (2026-10-20 15:01:29) never reached the local capture path. Every screen-analysis pattern is `^`-anchored, so the junk-prefixed whole matched nothing: the “analyse … screen” family, the “what is on my screen” family, the “what do I see” family and the research family all missed it. NLU was down, the turn fell through to the cloud Worker, and the reply was “I can’t see your screen” — the screen was never captured.

**Screen-analysis fix (source, included in the debug candidate).** The screen grammars now try every sentence candidate of the transcript — the whole text first, then each sentence on its own — against the same anchored patterns, so a harmless earlier STT sentence can precede the real command. The anchors, the required `screen` anchor and every misfire guard are unchanged.

**Foreground-review follow-up (source, included in the debug candidate).** The foreground-review grammar is the only other command that needs a bare target word (“analyse this website”, “review this app”), and it had the same defect class. It now reuses the same sentence splitter and additionally drops a polite lead-in (“please”, “can you”, “could you”, “would you”, “will you”) before the anchored check, so “Sure, this is the page. Analyse this website.” and “please, can you analyse this website” reach the review intent instead of being misread as a repository name by the generic analyse parser.

**What did not change.** The review and recreate grammars stay strictly anchored and stay separate: one candidate must be the whole command. Repository and pull-request analysis (“analyse my repo”, “analyse pr 24 in servx”, “review this PR”), an application-permission review (“review the app permissions”, “review app permissions”) and a voice recreation (“recreate my voice”) are still refused by this grammar, and a negated order (“do not analyze this website”, “please do not analyse this website”) is refused even when a polite lead-in precedes it.

**Verification state.** Focused parser tests were added for the positive path (a leading sentence, polite lead-ins, review and recreate kept distinct) and for the negative path (repo/PR analysis, permission review, voice recreation, negated order). The owner ran them after this edit:

- `cargo test --lib foreground_review_ --features custom-protocol,admin-brain -j 1`: **passed, 3 tests**.
- The freshly compiled `nexus_lib-088600957ca50bf2.exe` was then run directly and **passed** `foreground_recreation_survives_a_leading_sentence_but_stays_distinct`, `test_screen_analysis_survives_stt_noise_sentences` and `test_screen_annotation_survives_stt_noise_sentences`, one each.
- `git diff --check` on `intent_parser.rs`: **passed**, with only the existing CRLF normalization warning for that file.

These are source-level checks only. The current debug candidate contains the routing fixes (see the gate-fix candidate below), but it has not run a successful screen-analysis voice walkthrough, so **the voice and sidebar outcome remains unverified**.

## Foreground URL gate follow-up (2026-10-20 16:24)

The next live turn, “analyze my screen,” reached the local `screen_analysis` intent and GhostCenter. The running app identified Brave as foreground but replied “I couldn't verify the foreground page.” This is a different failure from the earlier parser miss: `screen_context::capture_block_reason` required a browser URL even when the app, title and bounded region were known. The process running at that time was the older `target/release/nexus.exe` built at 12:57, not the 16:16 debug candidate.

The current source makes the URL optional for screen analysis and foreground review. It still requires an identified app, nonempty title and verified region, and it still blocks recognized banking, password-manager and wallet windows, as the owner requested. A URL, when available, enriches the answer and permits public-page discovery. Without one, the review reports foreground-only coverage. The Windows URL probe now reads an address-bar control from the exact foreground browser window; it does not treat webpage text fields or a title-derived guess as a verified URL. A recheck rejects a changed title, region, URL, or a URL that appears or disappears between probes.

Verification: `cargo test --lib --features custom-protocol,admin-brain -j 1 -- screen_context::tests` passed 21 tests. The same freshly compiled test binary passed 5 `browser_url::tests`. These checks do not prove that Brave exposes its address bar through UI Automation on this laptop or that a live voice turn reaches the sidebar.

### Gate-fix debug manual candidate (2026-10-20 17:43)

`cargo build --bin nexus --features custom-protocol -j 1` completed in 7m 48s. `D:\ULTRON\src-tauri\target\debug\nexus.exe` was written at 17:43:32 local, SHA-256 `185419A7277DCBDCD91E903682380649E3ACE4F55359E581789EC29E736A1535`. The 12:57 release process was stopped, its `scripts/run.ps1` launcher exited, and the new debug executable started as PID 3068. Vite was listening at `127.0.0.1:5173` (Node PID 3760). Startup reached `[BOOT] stage ready after 1s`; no successful voice/sidebar walkthrough was observed. This debug candidate was superseded by the phrase-routing build below.

New stdout/stderr logs are `%APPDATA%\com.nexus.assistant\nexus_debug_2026-10-20_gate.log` and `nexus_debug_2026-10-20_gate_err.log`. The earlier 16:16 logs belong to the superseded build. The startup stderr contains `ERROR: The system was unable to find the specified registry key or value.`; the process remained alive and reached stage ready. The message has not yet been tied to screen analysis.

## Equivalent screen-analysis phrases (2026-10-20 18:32)

The owner confirmed the visual-tour contract: “analyse my screen,” “what is on my screen,” and “explain what I am seeing” should request the same captured-window Gemini analysis, spoken item-by-item narration and callouts, with the screenshot and detail cards in the sidebar. The existing parser and tour already handled those exact forms. This pass closed common phrasing gaps for “explain what is on my screen,” “tell me what's on my screen,” “describe what is on this screen,” and stacked polite/wake-word leads. The parser continues to refuse a negated instruction and unrelated repository/PR analysis. “Read/scan my screen” stays OCR-first for text transcription; “analyse this website/app” is the separate technology review.

Focused verification: `test_screen_analysis_phrases` passed (including the added positive and negative forms), `test_screen_analysis_survives_stt_noise_sentences` passed, and both `tour_routing` tests passed. `cargo build --bin nexus --features custom-protocol -j 1` completed in 6m 53s. The new debug executable was written at 18:32:32 local, SHA-256 `5958D7A45EB0A7F700186430D8099E3898C81CD79916DC5F1678B05AB32A0273`, and is running as PID 16632 with Vite PID 3760. Pre-flight passed and startup reached stage ready. Logs: `%APPDATA%\com.nexus.assistant\nexus_debug_2026-10-20_phrases.log` and `nexus_debug_2026-10-20_phrases_err.log`. The same registry-key stderr message appeared; it has not been linked to screen analysis. No successful voice/sidebar tour has yet been observed.

## How to run the manual walkthrough

The current phrase-routing debug candidate is running (PID 16632) with Vite on `127.0.0.1:5173`. `pwsh ./scripts/run.ps1` launches the older release binary, so relaunch `D:\ULTRON\src-tauri\target\debug\nexus.exe` directly for this test. First put a nonsensitive Brave page in the foreground and say **“analyse my screen,” “what is on my screen,”** or **“explain what I am seeing.”** The narrated tour should point at and speak about each detected item, then open the sidebar with the captured image and detail cards, even if Brave exposes no URL. Repeat on a nonsensitive native app. Then put a public HTTPS website in the foreground and say **“analyse this website.”** The first sidebar image should match that page; when a URL can be verified, the page gallery, technology evidence, coverage warnings and reference ID should appear as inspection advances. Without a verified URL, the review should say it covered only the foreground window. Banking, password-manager and wallet windows must still be refused. Then test **“recreate it”** with no active coding project: enter an existing destination folder and press **Start recreation**. In a separate trial with an active coding project, say “recreate it” and confirm the agent works in that project. Also try a signed-in/private tab, cancellation, switching windows during capture, and “recreate it” before any review. Do not put secrets in the trial page.

For this debug candidate, keep `%APPDATA%\com.nexus.assistant\nexus_debug_2026-10-20_phrases.log`, `nexus_debug_2026-10-20_phrases_err.log`, `vite_debug_2026-10-20.log` and `vite_debug_2026-10-20_err.log` from the run, especially `[FORE_REVIEW]`, orb, caption, and agent-watch lines. A release-candidate run writes `nexus_unified.log`, `nexus_unified_err.log`, `cdp_unified.log` and `cdp_unified_err.log` instead. Note the exact spoken phrase, visible result, review ID, target URL/title, whether the first screenshot matched, each page's `captured`/`not captured` status, and any warning. If the agent ran, include its task ID and final files/build/test receipt. Logs may contain sensitive window titles; inspect them before sharing.

## Remaining release work

The Rust adapter, sidebar, and recreation dispatch are ready for manual testing, subject to the checks recorded below. The 2026-10-20 routing fixes are compiled into the running debug manual candidate but are still **source-only until the next release/installer build**, and the voice and sidebar walkthrough remains unverified because no voice turn has been performed. A native folder picker, proof of image/name grounding and sourced public-person facts, encrypted restart/wrong-key tests, prompt-injection tests, complete cancellation and turn-end tests, browser/REA live exercise, Windows voice walkthrough, and macOS implementation/walkthrough remain. REA is installed only on the development laptop, not packaged in the installer. **Feature 146 is not production ready.**
