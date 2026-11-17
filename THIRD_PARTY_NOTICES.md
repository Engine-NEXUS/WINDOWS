# Third-Party Notices (reuse-first work — partial)

This file lists third-party material **copied or adapted into this repository** by the reuse-first plan
(`docs/research/jarvis-landscape/`). It is **not yet a complete inventory**: the full dependency licence
inventory (Rust crates, npm packages) is still to be generated — see doc 02 §"Next actions". Entries below
were added together with the code they cover.

| Material | Where in this repo | Upstream | Licence | Notes |
|---|---|---|---|---|
| Silero VAD v4 model `silero_vad_v4.onnx` (SHA-256 `a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28`) | `src-tauri/resources/vad/` | copied from `cjpais/Handy` `src-tauri/resources/models/`, model by `snakers4/silero-vad` | MIT | Silero's own copyright line **not yet verified from source** — confirm before distribution |
| VAD design (Silero v4 driven at 512-sample frames with carried LSTM state; hysteresis idea) | `src-tauri/src/vad.rs` | `cjpais/Handy` `src-tauri/src/audio_toolkit/vad/silero.rs` (+ `vad-rs`) | MIT — Copyright (c) 2025 CJ Pais | Re-implemented on `ort` (no `vad-rs` dependency); structure follows Handy |
| Smart Turn v3.2 model `smart-turn-v3.2-cpu.onnx` (SHA-256 `2bb026316b14a660486a75b1733cd3fbab8c2fd0314dc9af7be49f8cca967e4f`) | `src-tauri/resources/smart_turn/` | `pipecat-ai/smart-turn` (HuggingFace `pipecat-ai/smart-turn-v3`) | BSD-2-Clause (per repo page; copyright holder to confirm — Pipecat's own is "Daily") | |
| Whisper log-mel front-end for Smart Turn | `src-tauri/src/turn_detect.rs` | behaviour matched to HuggingFace `WhisperFeatureExtractor` and the reference code in `pipecat-ai/pipecat` `src/pipecat/audio/turn/smart_turn/_whisper_features.py` | BSD-2-Clause — Copyright (c) 2024–2026, Daily (Pipecat) | Independent Rust implementation, parity-tested (max diff 3e-5) |
| Turn-policy design (eagerness low/medium/high) | `src-tauri/src/turn_detect.rs` | concept from OpenAI Realtime `semantic_vad` docs | n/a (documentation of a closed API; no code) | Case 2 in doc 04 |
| Barge-in strategy design | `src-tauri/src/wakeword_oww.rs` (`barge_stop_decision`) | design influence: `pipecat-ai/pipecat` `src/pipecat/turns/*` | BSD-2-Clause | No code copied |
| Kokoro-82M model `model_quantized.onnx` (int8, 92,361,116 B, SHA-256 `fbae9257e1e05ffc727e951ef9b9c98418e6d79f1c9b6b13bd59f5c9028a1478`) and voice packs (522,240 B each) | downloaded at runtime into `%APPDATA%/com.nexus.assistant/voices/`; dev copy in git-ignored `src-tauri/resources/kokoro/` | `hexgrad/Kokoro-82M`, ONNX conversion `onnx-community/Kokoro-82M-v1.0-ONNX` at revision `1939ad2a8e416c0acfeecc08a694d14ef25f2231` | Apache-2.0 (model card) | Not committed. Voices ship in the same repo; no separate licence notes found |
| Kokoro vocabulary table (114 symbols), 510-phoneme chunking idea | `src-tauri/src/tts_kokoro.rs` | `pguso/kokoro` (`tokenizer.rs`, `pipeline.rs`) | Apache-2.0 | Table generated from upstream source; synthesis glue re-written on our `ort` |
| `misaki-rs` 0.6 (English G2P, `default-features = false`) | Cargo dependency | `MicheleYin/misaki-rs` | MIT | **Dictionary data** is "based on the original Misaki" and was partly generated with espeak-ng output (its `expand_dicts.py`) — provenance/legal status unresolved for distribution (doc 09 §6). espeak itself is NOT linked |
| (removed) Piper TTS / `piper-rs` / bundled `espeak-ng-data` | — | — | GPL-3 | Removed 2026-10-05 (change 81) |
| Memory v2 design (always-on bounded user profile with a hard size cap, add/patch/remove memory edits, approval-gated write review, background review of conversations treated as data, skills-like procedures idea) | `src-tauri/src/memcore/{profile,review,curator,routines,corrections}.rs` | design influence: Hermes Agent by Nous Research (`NousResearch/hermes-agent`, official docs on persistent memory and the memory/skills review); idle consolidation idea from Letta (`letta-ai/letta`, "sleep-time" agents) | MIT (Hermes Agent, per its repository); Apache-2.0 (Letta) | **No source code copied** - re-implemented in Rust on our encrypted store; the review prompt and all text are written independently. Hermes copyright line **not yet verified from source** - confirm before distribution, and keep this row if any prompt wording is ever copied |
| Dotted thought-orb engine (9 animated orb designs: geometry modes, presets, theme hook) | `frontend/src/avatar/dotted/` (engine copied; `paintInk.ts`, `resolveOrb.ts`, `DottedOrb.tsx` are NEXUS code) | `Jakubantalik/thinking-orbs` (`src/engine/*`, `src/presets.ts`, `src/types.ts`, `src/theme.ts`), checked 2026-11-07 | MIT - Copyright (c) 2026 Jakub Antalik (full text in `frontend/src/avatar/dotted/LICENSE-thinking-orbs.txt`) | Copied unmodified except one unused-parameter rename in `engine/core.ts`. Not an npm dependency. Keep this row and the licence file with the code |
| Reverse-engineer fingerprints: JavaScript library script file names, script URLs and script banners (with version capture) | `src-tauri/src/site_fingerprints.json` (signals whose evidence ends with "[Retire.js]"; generated by `scripts/import_fingerprints.py`) | `RetireJS/retire.js` `repository/jsrepository.json` at commit `35dca7b367884e571528203f1bf86928707671f5` | Apache-2.0 (full text in `src-tauri/src/fingerprint_licenses/LICENSE-retire-js.txt`) | Patterns converted to Rust regex syntax (version placeholder replaced, other groups made non-capturing; patterns with backreferences or lookaround dropped). Vulnerability data is NOT imported |
| Reverse-engineer fingerprints: page globals that identify a JavaScript library, read by the hidden render pass | `src-tauri/src/site_fingerprints.json` (signals whose evidence ends with "[Library Detector]"; generated by `scripts/import_fingerprints.py`) | `johnmichel/Library-Detector-for-Chrome` `library/libraries.js` at commit `4838701f6cc04bb5a94d7a19bba1e6318c7707c0` | MIT - Copyright (c) 2010-2016 Andrew Bredow, John Michel, and other contributors (full text in `src-tauri/src/fingerprint_licenses/LICENSE-library-detector-for-chrome.txt`) | Only plain `win.<path>` presence and version paths are converted; the detection functions themselves are not copied or run |
| Tech Stack sidebar: local logos for detected technologies, languages and apps | `frontend/public/tech-icons/*.svg`, `frontend/src/sidebar/techIcons.json` (generated by `scripts/vendor-tech-icons.py`) | `simple-icons` 13.21.0, `@iconify-json/logos` 1.2.17, `@iconify-json/devicon` 1.2.81 (MIT), `@iconify-json/vscode-icons` 1.2.99 (MIT), `@iconify-json/cib` 1.2.3 (CC0); marks fetched from each tool's own site by `scripts/scrape-tech-icons.py` (`frontend/public/tech-icons/scraped/`, sources in `techIconsScraped.json`); NEXUS's own `activity-logos` | CC0-1.0 (simple-icons licence copied to `frontend/public/tech-icons/LICENSE-simple-icons.md`) | Marks only, recoloured to the brand colour; product marks belong to their owners and presence implies no affiliation |
| (not used) Wappalyzer technology definitions and their forks (`enthec/webappanalyzer`, `HTTPArchive/wappalyzer`) | — | — | GPL-3.0 | Deliberately not imported: NEXUS ships no GPL code or data |
| Agent Island: permission-reply contract (nothing is allowed without a human click; per-agent stdout shapes; AskUserQuestion answers), approval timing (800 ms ack, 108 s decision, silence falls back to the agent's terminal), island window placement and cursor test | `src-tauri/src/agent_approval_reply.rs` and `frontend/src/agent-island/diff.ts` (ported, tests included; the line-diff engine is from `windows/src/core/diff.ts`), `agent_island.rs` and `agent_monitor.rs` (design and timings) | `Louis-CFM/coucou` `windows/hook/src/reply.rs`, `windows/src/core/diff.ts`, `windows/src-tauri/src/island.rs`, `pipe.rs` | MIT - Copyright (c) 2026 Louis Raillé | Code ported under the MIT notice below. **Not copied:** the names "Coucou"/"Mochi", the character, icons, sounds, `docs/media/` and `design/` (reserved by its `LICENSE-ASSETS.md`). Transport is NEXUS's own token-authenticated webhook, not coucou's named pipe |
| Test fixture `sapi_open_whatsapp_16k.wav` | `src-tauri/tests/fixtures/` | generated locally with Windows SAPI | n/a | Synthetic speech, test-only |

## MIT licence text (Handy, Silero, coucou)

Coucou: Copyright (c) 2026 Louis Raillé. Same MIT terms as below; the copyright line above applies to the parts ported from `Louis-CFM/coucou`.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions: The above copyright notice and
this permission notice shall be included in all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED
"AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## BSD 2-Clause licence text (Pipecat / Smart Turn)

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of
conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice,
this list of conditions and the following disclaimer in the documentation and/or other materials provided with the
distribution. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## logoapi.sh

21 product logos in `frontend/public/activity-logos` were downloaded from https://logoapi.sh (embedding in an app is
allowed by its terms; the whole catalog is not copied; takedown contact takedown@logoapi.sh). Per-file url, hash and
date: `frontend/public/activity-logos/SOURCES.json`. The marks belong to their owners.
| Pronunciation table for sound-based matching (126,000 English words with phonemes, compacted to one character per phoneme) | `src-tauri/src/sound_match_cmudict.txt`, built by `scripts/gen_pron_lexicon.py`, used by `src-tauri/src/sound_match.rs` | The CMU Pronouncing Dictionary, Copyright (C) 1993-2015 Carnegie Mellon University (cmusphinx/cmudict; data file taken from the `cmudict` Python package data directory, whose wrapper code is GPL and is NOT used) | BSD-style (CMU licence: keep the copyright notice, the conditions and the disclaimer in source and binary distributions) | Derived data only (first pronunciation, stress removed). The CMU copyright notice, conditions and disclaimer are reproduced in `src-tauri/src/sound_match_cmudict.LICENSE`. |

## Trademarks

Logos are trademarks of their owners and are shown only to identify the alert source.

