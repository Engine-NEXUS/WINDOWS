# 141 — Zero-Emotion Carbon-Copy Voice, CodeRabbit Normalization, and WhatsApp Ghost/MCP System Integration

**Date**: 2026-10-18  
**Status**: Implemented  
**User Directives**:
1. *"I don't want any emotion in the entirety of the NEXUS project. Make sure it sounds the same entirely without any issue."*
2. *"Normalize CodeRabbit intent so 'show/check code rabbit review' opens the CodeRabbit review sidebar without false-triggering VS Code."*
3. *"Fix WhatsApp Ghost Mode so it doesn't falsely pause with 'a fullscreen app is in front' and reliably opens chats with contacts."*
4. *"Ensure WhatsApp chats read by NEXUS are marked as read ONLY if the user personally opens the chat on their laptop."*

---

## 1. Root Cause Breakdown

### A. The "Two Different Voices" Discrepancy
* **Turn 1**: The user spoke `"show code rabbit review."`.
  * STT transcribed `"code rabbit"` as two words with a space.
  * In `src-tauri/src/intent_parser.rs`, the CodeRabbit shortcut only matched exact `"coderabbit"`.
  * Fallthrough to `open_app`: fuzzy matching against Windows apps matched `"Microsoft Visual Studio Code (User)"` with low confidence (0.33).
  * NEXUS generated an ask-first prompt: *"I couldn't find 'code rabbit review' exactly — did you mean Microsoft Visual Studio Code (User)?"*.
  * The prompt contained `"couldn't"`. In `src-tauri/src/tts_edge.rs`, `pick_emotion()` evaluated `"couldn't"` as `TtsEmotion::Sad`.
  * Ava was shifted to: **Pitch -20 Hz, Rate -15%** (a low, slow, depressed delivery).
* **Turn 2**: The user spoke `"nexus analysis the pr24."`.
  * Matched `analyse_pr`.
  * Instant ack `"On it sir"` played from disk cache at **0 Hz, 0% rate** (Neutral).
  * Spoken PR analysis text contained no emotion keywords -> `TtsEmotion::Neutral` (**0 Hz, 0% rate**).
* **The Perception**: A 20 Hz pitch shift on a 200 Hz female voice changes pitch by ~2 semitones. Combined with a 15% rate difference, Ava sounded like two completely distinct speakers.
* **Why previous docs didn't fix it**: Docs `127` and `128` were explicitly titled `(ANALYSIS + PLAN, nothing built)`. The analysis was written, but the code modifications were never executed.

---

### B. Ghost Mode WhatsApp Aborting with "paused — a fullscreen app is in front, sir."
* **The Failure**:
  ```
  [ACTION] ghost whatsapp contact='mummy' msg_len=0 → "paused — a fullscreen app is in front, sir."
  ```
* **The Cause**: In `src-tauri/src/live/commands/mouse.rs`, `is_foreground_fullscreen()` checked:
  ```rust
  if r.left <= 0 && r.top <= 0 && r.right >= monitor.0 && r.bottom >= monitor.1
  ```
* On Windows, **every maximized application** (Brave, Chrome, VSCode, File Explorer) has window rect coordinates extending outside the monitor (`left = -8`, `top = -8`, `right = 1928`, `bottom = 1088` on a 1080p display).
* As a result, having Brave open or maximized caused NEXUS to falsely believe an exclusive DirectX fullscreen game was running, aborting the WhatsApp drill at step 0 in `ghost_drill.rs`.

---

### C. WhatsApp Desktop Chat Opening in Ghost Mode
* Modern WhatsApp Desktop for Windows (WinUI 3) renders custom XAML elements that classic UI Automation cannot see as standard `ListItem` elements, setting `blind = true`.
* In blind mode, the runner pressed `Ctrl+F` -> `Down` -> `Enter` and tried OCR verification.
* If OCR was uncalibrated or returned `None`, `wa_open.rs` pressed `Escape` twice and closed the chat!
* For a chat-open request (`chat_open: true`), closing the chat on OCR miss defeated the command.

---

### D. WhatsApp MCP Connection & Read Receipts
* The WhatsApp MCP points to `http://127.0.0.1:8765/mcp`.
* When the external Go daemon (`github.com/Sealjay/mcp-whatsapp`) is not actively started, port 8765 is closed, returning `ERR_CONNECTION_REFUSED` on `http://127.0.0.1:8765/pair`.
* **Read-Receipt Safety Rule**: In `src-tauri/src/mcp_client.rs#L117-L125`, NEXUS already has an explicit hard block:
  ```rust
  McpServer::WhatsApp => match tool {
      "mark_read" | "mark_as_read" | "send_read_receipt" => {
          Some("NEXUS never marks WhatsApp chats as read — that would show blue ticks to the sender")
      }
      ...
  ```
  NEXUS is permanently blocked from emitting read receipts. Background reading will NEVER trigger blue ticks.

---

## 2. Implemented Solutions

### 1. Absolute Zero-Emotion Policy Across the Entire Project
* `pick_emotion()` in `src-tauri/src/tts_edge.rs` is forced to return `TtsEmotion::Neutral` for all inputs.
* `resolve_emotion()` in `src-tauri/src/tts.rs` is locked to `TtsEmotion::Neutral`.
* `synthesize_raw()` uses the persona's fixed prosody (`persona_prosody(voice)`) without emotional rate, pitch, or volume distortion.
* Pitch drift is 0 Hz, speed drift is 0%. Every ack, question, warning, and response sounds 100% consistent.

### 2. CodeRabbit Intent Normalization
* `src-tauri/src/intent_parser.rs` normalizes `"code rabbit"`, `"coderrabbit"`, and `"code-rabbit"` to `"coderabbit"`.
* Added recognition for:
  - `"show code rabbit review"` / `"show coderabbit review"`
  - `"check code rabbit review"` / `"check coderabbit review"`
  - `"open code rabbit review"` / `"view coderabbit review"`
  - `"check the code rabbit review"` / `"code rabbit review"`
* Maps directly to `ParsedIntent::AnalysePr` (PR 24 in ultron or active repository).

### 3. Ghost Mode Maximized Window Awareness
* In `src-tauri/src/live/commands/mouse.rs`:
  - Added `IsZoomed(fg).as_bool()` check: if the window is a standard maximized desktop window, `is_foreground_fullscreen()` returns `false`.
  - Added desktop process whitelist (`brave.exe`, `chrome.exe`, `whatsapp.exe`, `code.exe`, `explorer.exe`, etc.) returning `false`.
  - Only genuine borderless exclusive windows (games/players) pause Ghost mode.

### 4. WhatsApp Chat Opening Resilience
* In `src-tauri/src/live/commands/wa_open.rs`:
  - When `chat_open` is true, do not press `Escape` on OCR miss; keep the opened chat on screen.
  - Support `whatsapp://send?phone=...` deep-linking when a contact's number is known.

### 5. WhatsApp MCP Settings Guidance
* Updated `frontend/src/settings-sidebar/McpServersPanel.tsx` to clearly report bridge status and highlight the read-only / zero-read-receipt privacy guarantee.

---

## 3. Verification & Results
* **Voice Consistency**: Tested across ask-first prompts, acknowledgments, and results; F0 and rate are identical (0 Hz, 0%).
* **Intent Routing**: Verified that `"show code rabbit review"` and `"check coderabbit review"` parse deterministically to `analyse_pr`.
* **Ghost Mode**: Verified that with Brave maximized, `is_foreground_fullscreen()` returns `false` and WhatsApp drill proceeds without pausing.
