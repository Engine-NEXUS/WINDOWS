# Change 78 — Ghost Mode, Screen Analysis & Memory Fix Pass (+ Free/BYOK Key Hygiene)

## Problem & Motivation

After the Dexter comparison the user narrowed the work to three pillars and two constraints:

- **Pillars:** (1) *Memory* — remember chats, know who the user is / what they can do / what they need / their priorities, learn over time; (2) *Ghost Mode* — "broken"; (3) *Screen analysis* — "broken".
- **Constraints (binding):** NEXUS is built at zero cost; cloud features run on the user's **own free Groq + Gemini keys (BYOK)**; local usage must stay inside **600–700 MB RAM**.

Six read-only audits (onboarding/keys, MCP bridge, TTS, memory, ghost, screen) fed a plan (`/root/.claude/plans/nexus-vs-dexter-curious-sundae.md`, approved before any code). Items that cost money or RAM were **evaluated and deferred** (see "Deliberately not built"). Everything below was found by reading code; the app cannot run on the Linux box used for this work, so each fix is verified by unit tests + compilation, and the Windows-only paths are listed in the live checklist at the bottom.

## Phase 0 — Key hygiene (BYOK)

| Defect | Fix | File |
|---|---|---|
| Keys typed in Settings were written only to `settings.json`, but the **keychain is read first** — a stale keychain copy silently overrode the corrected key (and "Test key" tested the old one) | `save_settings` trims the three keys and writes the keychain too (an emptied field clears it) | `commands.rs` |
| `migrate_api_keys_to_keychain` had no caller; it also skipped services already in the keychain | Called at boot; now **settings.json wins when it differs** (empty/stripped file leaves the keychain alone) | `auth_vault.rs`, `lib.rs` |
| `read_groq_api_key` read `settings.json` only (keychain-only keys ignored by STT, vision gates) | Delegates to keychain-first `read_api_key`; both trim | `commands.rs` |
| `save_settings` rebuilt `nexus-config.json` from scratch, dropping `identity` / `profileId` / `lastKnownStatus` | Updates only `serverUrl`/`userId`/`deviceId`, preserves the rest | `commands.rs` |
| `identity_state.rs` defined `POLL_INTERVAL_SECS`/`POLLS_PER_SESSION` twice (E0428 — the crate did not compile on any platform) | Removed the duplicate | `identity_state.rs` |

**Deviation from the plan:** the plan said to strip keys from `settings.json`. Several modules read them directly (`lazy_stt.rs`, `telegram.rs`, `wakeword_oww.rs`) and `get_settings` does not inject keychain values, so stripping would have broken them. Keys stay in `settings.json` (trimmed) **and** the keychain. The Settings copy now says "OS keychain + your local settings file" instead of overstating.

## Phase 1 — Ghost Mode

| # | Root cause | Fix |
|---|---|---|
| G1 | `center::action_disposition` dropped **every** ghost-session turn from an `Unenrolled` speaker except `Strong` evidence — and speaker verification is off by default, so it is always Unenrolled. `deterministic-live` / `-dictation` / `-screen-*` sources scored 0 ("no evidence") and were dropped as ambient speech | `(Strong, Unenrolled)` → Allow in ghost; those four hand-written regex sources score as strong. Narrative (`Unknown`) and ML-guessed (Medium) evidence still need a verified owner — **deliberately narrower than the plan**, which said Strong\|Medium (an open mic + ML guesses is the wrong trade) | `center.rs` |
| G1b | Stop-words ("halt", "abort", "stand down") parse as `Unknown`, so the ambient gate ate them | Stop-words bypass the gate (still dropped for `Rejected` audio) | `orchestrator.rs` |
| G2 | `request_stop()` sets `GHOST_CANCEL`, only `ghost_enter` cleared it → after one voice "stop", every later action said "Stopped, sir." | `clear_stale_stop()` at the start of each fresh turn when no drill is running (clearing in `drill_end` would have broken the post-drill `!stop_requested()` drain checks) | `ghost.rs`, `orchestrator.rs` |
| G3 | `ghost::spawn_relisten_watchdog` had **zero callers** (lost in the 10-01 `lib.rs` recovery) | Started in `lib.rs` setup | `lib.rs` |
| G4 | A dropped ambient turn emitted nothing; the ghost mic is single-shot and the 8 s no-input timeout is skipped in ghost → orb "listening", mic dead | `close_dropped_ghost_turn` emits `Done` (silent relisten), skipped mid-drill/outside ghost | `orchestrator.rs` |
| G5 | Stage cold start grace (8 s) tuned for the empty stage; a rebuilt stage never learned a session was live | Grace → 20 s; every heartbeat re-announces a live ghost session (listener is idempotent). **Ctrl+Alt+X kill-switch semantics left unchanged** (safety control; not changed without a live test) | `stage.rs` |
| G6 | UIA search rooted at the *focused leaf* (`UIMatcher::from` searches descendants only) → "click X", address-bar URL, and queued steps after Ctrl+T found nothing | Root at the foreground **window** (`foreground_window_element`, type-checked against `uiautomation 0.25` / `windows 0.36` for the Windows target); `verify_grounding` checks the foreground *process name* (`foreground_is_browser`) instead of a URL a fresh new tab doesn't have | `screen.rs`, `browser_url.rs`, `ghost.rs` |
| G7 | `resolve_and_open_app` returns `Ok(success:false)` for an unknown app; runners matched bare `Ok(_)` → spoke "Ok sir." after opening nothing, never reached the Win-search fallback | Match `Ok(r) if r.success`, else fall back (also in the WhatsApp drill) | `orchestrator.rs`, `ghost_drill.rs` |
| G8 | Vision key gate accepted a Groq-only key although `provider_order("auto")` is Gemini-only → "daily vision limits are spent" with no call made. `parse_coords` split decimals ("512.5, 300.2" → (512, 5)) | `has_usable_vision_key(order, groq, gemini)` at both gates (spoken message names the free Gemini key); decimal-aware `parse_coords` | `vision.rs`, `mouse.rs`, `orchestrator.rs` |
| G9 | `ghost_wry::g_wry` did `ptr::read` then dropped the original → double owner of the `Arc`s, one decrement too many per call (latent use-after-free) | `ManuallyDrop` transfers ownership | `ghost.rs` |
| G10 | Mid-drill speech for `new tab` / `type` / `click` / `focus` / dictation executed inline, bypassing the FIFO queue | Inline match skipped while `session_active() && drill_running()`, so it reaches the queue block | `orchestrator.rs` |

Not done (listed, not silently dropped): G11 secondary items (orb hitbox over the taskbar while ghost is active, stage geometry sync to the monitor, waves rect re-emit, ring `note_expected`/`note_idle` leak, half-started session when `register_esc` fails).

## Phase 2 — Screen analysis

| # | Root cause | Fix |
|---|---|---|
| S1 | `try_ocr_answer` set the pending sidebar text + flipped the view but never emitted `sidebar:show`; the sidebar is kept warm and only fetches pending content on mount → "full text is in the sidebar" pointed at a blank/stale card on every use after the first | Emit `sidebar:show {query,text}` (same payload the other warm-window paths use) |
| S2 | `stage_set_hitboxes` now requires `(source, rects)` (commit `ff42fa7`); callers sent `{rects}`, the error was swallowed → pins never clickable | `source:"spatial"` / `"annotation"`; failures are logged |
| S3 | The stage CSP nonce makes React `style={{}}` inert (documented orb bug) — boxes/pins/SVG collapsed to 0×0 | Positioned via DOM refs (`placeAt`), SVG layer styled by CSS class |
| S4 | "what's on my screen" always went to OCR even with a valid Gemini key | OCR first only for text requests (`screen_query_wants_ocr`: read/scan/list/text…) or when the VLM is unusable |
| S5 | Any 429 locked vision (shared with ghost click grounding) until the next Pacific midnight | `classify_429` (quotaId PerDay vs PerMinute): daily → lock, rate → 60 s cooldown; whole model ladder tried before reporting quota |
| S6 | Non-2xx swallowed, up to ~60 s of dead air, a rejected key was indistinguishable from "no result"; the plain-text retry re-failed identically | Failures logged with status + body; typed `SpatialError` → one spoken reason (rejected key / rate limit / timeout); 25 s total cap; plain-text retry only for parse/availability misses; the reason survives into the OCR/UIA fallback line |
| S7 | NEXUS photographed its own "Analyzing…" caption/orb and old pins | "Analyzing…" spoken **after** the screenshot (`on_captured`); stale pins cleared + 150 ms before capture |
| S8 | Reply truncated at 2048 tokens (thinking tokens count) parsed as nothing | 4096 budget, `finishReason` logged, truncated JSON repaired (`repair_truncated_json`). `thinkingConfig` deliberately **not** sent — unverified per model; listed in the live checklist |
| S9 | Pins never dismissed | 20 s expiry in the stage layer |
| S10 | Parser missed "whats", the curly apostrophe, "display", "can you analyse my screen", "look at my screen" | Tolerant `parse_screen_analysis_command` (+ guards: "screen time/saver/…" are not analysis) |

Not done: stage geometry sync / multi-monitor capture, ML-sourced `screen_analysis` quota guard, honoring cancel inside `run_screen_analysis`, "tour overlay" (no code exists — a feature, not a bug, needs a decision). `AnnotationCanvas.tsx` still uses inline `style` for ink colour / pointer events (same CSP issue; out of scope, flagged).

## Phase 3 — Memory (about-me, history, self-learning)

**Bugs fixed**
- `memory::get_memory_context` `String::truncate(800)` panicked on multibyte text → char-safe (`truncate_chars`); same class in `router::truncate` (log lines).
- `split_is`/prefix slicing used byte offsets from a *Unicode*-lowercased copy → ASCII-lowercase (panic on e.g. `İ`).
- Writes were non-atomic and a corrupt `core.json` was replaced by `{}` → temp-file + rename, corrupt original kept as `*.corrupt`.
- `conversation::merge_briefs` kept the **oldest** five key points (brief froze once full) → newest wins; brief carries `updated_at`; prompts drop turns older than 48 h (unresolved 7 d) and briefs older than 14 d.
- `"my X is Y"` stored any complaint ("my mic is not working") → personal-attribute allow-list (explicit "remember …" still accepts anything).
- `get_learned_corrections` returns objects; the frontend destructured tuples (swallowed `catch {}`) → learned STT corrections never applied; `normalizeLearnedCorrections`.
- The Worker discarded `dialog_context.memory` → `general_prompt.ts` (data-fenced, sanitized, capped); 9Router system prompt explains the `Context:` block.
- `clear_transcript` was a no-op while the label promised "Remove all stored transcripts" → deletes conversation + brief + episodes.
- Mail-watch baseline never advanced (re-alert every 15 s forever) → `advance_mail_watch`; `diff_thread_state` now reports a deadline change anywhere in the new messages and the *newest* reply.
- Local commands left no trace → privacy-whitelisted turn + habit counter (`local_turn_summary`; never dictation, searches, message bodies or contacts).

**New capability** — see `docs/features/90-memory-profile-habits-and-memory-panel.md`: `profile.rs` (always-injected about-me block, PII entries stay local), narrow voice grammar, `habits.rs` (local self-learning → suggestions the user confirms), `memory_ipc.rs` + a **Memory tab** in Settings.

## Phase 4 — Guided BYOK onboarding

- Setup Preferences step: optional free Groq / Gemini key card; a Groq key explicitly switches speech-to-text to Groq's cloud (stated next to the field) — no silent privacy flip (`setupKeys.ts`, tested).
- Settings → Groq card: free-key link, and a one-click fix for the trap where a saved key is ignored because **Local STT only** (default on) is still set (`keyHints.ts`).
- No speech engine at all (no Python sidecar, no key) used to be silence → one spoken guide per run (`stt_unavailable_guidance`).
- Pending-approval Worker denial now also points at the free key path; diagnostics no longer claim "Groq cloud primary" for keyless users.
- Default local STT model → `small_streaming` (~300 MB) to fit the 600–700 MB budget (`DEFAULT_MOONSHINE_MODEL`); a saved choice is untouched.

## Deliberately not built (advantages / disadvantages — decision record)

| Option | Why not |
|---|---|
| Worker free tier / bundled shared key | You pay, abuse surface, reverses doc-88 "no self-service AI access"; conflicts with "built for free" |
| Bundle Python + Moonshine | Installer size + RAM; spoken guide instead |
| Premium TTS (ElevenLabs/OpenAI) | Paid; Edge Ava + Piper already free. Design retained in the plan (opt-in tier ahead of Edge, premium cache key, ElevenLabs `with-timestamps`) |
| Generic MCP bridge | New `McpServer` variant + side registry, confirm-by-default for unknown tools, unverified handshake; not needed for the pillars |

## Verification

- Rust `cargo test --lib -- --test-threads=1`: **922 / 922** (baseline 874; +48 new) — run on Linux with Windows-only call paths temporarily enabled/stubbed so they were *type-checked*; scaffolding removed before commit. Windows-only UIA helper additionally type-checked for `x86_64-pc-windows-gnu` against `uiautomation 0.25` + `windows 0.36`.
- Frontend: `tsc` clean (non-test), vitest **164 / 164** (new: learnedCorrections, memoryModel, keyHints, setupKeys). `src/setup/identityBanner.test.ts` fails to *load* (`describe is not defined`) — pre-existing, untouched. One unrelated one-off flake was seen once and did not reproduce in 3 re-runs.
- Worker: vitest **102 / 102** (+5: `general_prompt`).
- A latent compile error existed on `HEAD` (`identity_state.rs` duplicate consts); fixed.

## Live checklist (Windows, user-run — cannot be done from this environment)

1. **Ghost:** "ghost mode" → "open Brave" → "new tab" → "search almonds"; then "stop" followed by another command (must run); log must not show `ambient drop (weak evidence, auto session)`; kill the sidecar and confirm the orb recovers (watchdog log line `ghost-watchdog`).
2. **UIA rooting:** with focus in the Brave omnibox, "click the second result" resolves via UIA (no vision call).
3. **Screen:** "analyse my screen" with a valid Gemini key, a bad key, no key, and after a forced 429; pins visible and correctly placed at 125 %/150 % scaling; the sidebar shows OCR text on the *second* use; confirm BitBlt no longer captures the "Analyzing…" caption. Check `finishReason` / `thoughtsTokenCount` in the log for `gemini-3.5-flash-lite` and whether the fallback model IDs still resolve.
4. **Memory:** "my priority is …" → restart → "what are my priorities" (also with 9Router keys removed so the Worker path answers); Settings → Memory shows/edits/deletes entries; "Clear" really empties the stored conversation.
5. **Keys:** paste a corrected Gemini key over a previously-saved one → "Test Key" reflects the new key.
6. **RAM:** with `small_streaming`, steady-state total stays ≤ 700 MB.
