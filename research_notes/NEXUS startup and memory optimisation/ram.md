# NEXUS RAM audit (read-only, 2026-11-01)

Scope: read-only. Nothing was started, stopped, built, or edited. A release `nexus.exe` (PID 5492, started 15:57:56) was already running, launched by `nexus start`, and was sampled at 16:10 after about 13 minutes idle. Tags: **V** = verified (file:line, command output, or URL). **I** = inferred.

## 0. Live snapshot (V: `Get-Process` / `Win32_Process`, 2026-11-01 16:10)

| PID | Process | WS MB | Private MB | Peak WS MB |
|---|---|---|---|---|
| 5492 | nexus.exe (Rust core, 63 threads) | 127.8 | 79.5 | 145.6 |
| 5088 | WebView2 browser (manager) | 96.1 | 43.1 | 120.3 |
| 2840 | WebView2 GPU process | 83.7 | 101.3 | 182.1 |
| 26484 | WebView2 renderer (started at launch, so the `stage` window, I) | 64.9 | 43.8 | 86.4 |
| 14124 | WebView2 renderer (started 42 s later; probably the hidden `sidebar`, I) | 86.4 | 41.2 | 90.0 |
| 16896 / 2232 / 25628 | network / storage / crashpad utilities | 48.4 | 25.6 | — |
| **NEXUS app total** | | **507** | **334** | |
| 1784 | `node nexus.mjs start` (dev launcher, stays resident) | 159.5 | 134.3 | |
| 26664 | `node scripts/cdp_monitor.js` (dev log tap) | 49.1 | 59.9 | |
| **Total with the dev launcher** | | **~716** | **~528** | |

No Python sidecar was running: no `stt_server.py`, no `nlu_server.py`, no `brain_server.py`. Kokoro was not loaded either (the machine was online). So the idle footprint is **334 MB private for the app itself, plus 194 MB private for the two dev-launcher node processes**. Only the dev `nexus start` path creates those two node processes, and it also passes `--remote-debugging-port=9222` (V: `scripts/run.ps1:170`, `:263`; it also appears on the command line of PID 5088).

The AGENTS.md figure of "idle ~104 MB" is **no longer true** (V, this table). It predates the always-on stage overlay and the in-process models.

## 1. Inventory

| Component | Where | When it loads | Idle RAM | Peak RAM | Unload |
|---|---|---|---|---|---|
| Wake engine: tract `melspectrogram.onnx` (1.04 MB), `embedding_model.onnx` (1.27 MB), `nexus.onnx` (0.82 MB) | `wakeword_oww.rs:540`, `:905`, `:3600` | Always | inside nexus.exe (I: about 10-25 MB of optimised plans and buffers) | same | never (needed) |
| Speaker check (Voice Lock): a **second `AudioFeatures`**, i.e. a duplicate mel and embedding model set | `wakeword_oww.rs:4475`, `:4523` (also `:4444` for enrollment) | Always while `speakerVerification` is true (it is true in the live settings, V) | I: about 5-15 MB | same | never |
| Tier-3 command classifiers `oww/commands/*.onnx` | `wakeword_oww.rs:771-934` | Always, if the files exist | 0 here (no `commands/` dir among the resource files listed, V) | — | — |
| Silero VAD v4 (ort, 1.72 MB) | `wakeword_oww.rs:2743`, `vad.rs:31` | Always | I: about 5-10 MB plus an ORT thread pool | same | never |
| Smart Turn v3.2 (ort, 8.28 MB) | `wakeword_oww.rs:2798`, `turn_detect.rs:170` | Only when `smartTurn: true`. That key is absent from the live settings, and `read_smart_turn` defaults to false (V: `commands.rs:2911`) | 0 now; I: about 20-40 MB when on | | never once loaded (restart required) |
| Local NLU, BERT-Mini, **in-process via tract** (`nexus_nlu.onnx` 16.8 MB plus `.onnx.data` 16.8 MB) | `nlu_local.rs:44`, `:157`, `:202` (prewarm at wake: `wakeword_oww.rs:3175`) | On the first wake (prewarm) | +42 MB after the first wake (AGENTS.md measurement; not re-measured) | same | **never** (static `MODEL`; only `reload()`) |
| Kokoro TTS (ort, `model_quantized.onnx` 88 MB, already int8) | `tts_kokoro.rs:436-506`, policy `tts_network.rs:192-222` | Only when offline or the cloud TTS fails | 0 now | 259-369 MB (AGENTS.md measurement) | after 10 min online-stable or 10 min idle (V: `tts_network.rs:32`, `:212`); reload ≈1.7 s cold |
| TTS ack cache: 33 phrases kept as f32 PCM in a HashMap | `tts.rs:183`, `:398-432` | At startup, pre-synthesised | I: about 4-6 MB (33 phrases × ~1.5 s × 24 kHz × 4 B) | | never |
| Disk TTS cache (`tts_cache`, 284 files, 16.7 MB) | `tts_cache_disk.rs:15`, `:144` | Per-line read on demand | ≈0 resident | | n/a |
| memcore sealed DB, decrypted in memory (`memcore.enc` 0.84 MB) | `memcore/mod.rs:63` | Always | I: about 2-5 MB | | never |
| Small static caches (auth tokens, regex, brand logos, oEmbed, app registry) | grep `static .*HashMap` | Always | I: under 5 MB in total | | |
| Moonshine STT sidecar `stt_server.py` | `lazy_stt.rs:25-26` | On wake, only if there is no Groq key or `localSttOnly` is on (V: `wakeword_oww.rs:4373-4377`) | 0 now | 150-340 MB (AGENTS.md measurement) | **idle kill after 60 s** (V: `STT_KEEP_ALIVE=false`, `lazy_stt.rs:26`; the comment at `:315` claiming "kept alive" is stale). Cold start costs seconds |
| NLU sidecar `nlu_server.py` | `lazy_nlu.rs` | Not called anymore: no `lazy_nlu::` caller found by grep (V). Replaced by `nlu_local` | 0 | | — |
| Qwen brain `brain_server.py` (468 MB GGUF) | `lazy_brain.rs:136-144` | Admin-only, on demand | 0 | I: ≥500 MB | **no idle timeout** (V: `lazy_brain.rs:136`) |
| Node agent-watch sidecar | `agent_watch.rs:31`, `lib.rs:806` | Only while a coding-agent watch is active | 0 | I: about 50-70 MB | 10 min idle (V) |
| WebView2 `stage` (fullscreen transparent overlay with the WebGL orb) | `dyn_windows.rs:113`; low-memory mode at `mic_permissions.rs:41-51` | Always | renderer 44 MB plus a share of the GPU process (101 MB) and the browser process (43 MB) | GPU peak 182 MB | never (needed) |
| WebView2 `sidebar` | `dyn_windows.rs:65` | On demand | renderer about 41 MB (I, PID 14124) | | **hidden instead of destroyed**, because `DEV_KEEP_WINDOWS_ALIVE = true` (V: `dyn_windows.rs:281-293`) |
| `settings`, `setup`, `architect-map`, `maps-panel`, `place-card`, `calibrate-toolbar`, `hand`, `orb-studio` | `dyn_windows.rs:42-166` | On demand | 0 when closed | about 40-90 MB per renderer | destroyed on close (V: `commands.rs:88`, `:1502`, `:1545`; `architect.rs:729`; `maps_panel.rs:108`, `:125`; `place_card.rs:708`; `hand/mod.rs:1212`; `calibration.rs:253`) |
| External Google Maps window (`open_external_window`) | `dyn_windows.rs:308` | On demand | — | a cross-origin renderer, I: about 80-150 MB | not checked |
| Edge TTS persistent WebSocket | AGENTS.md (2026-10-17 entry) | Always | negligible | | |

All windows share one user-data folder (`...\com.nexus.assistant\EBWebView`, V from the PID 5088 command line). As a result there is one browser process and one GPU process, and same-origin windows can share a renderer (V: Microsoft's process-model documentation, §3). Each extra window is therefore about +40-90 MB (renderer only), **not** the "~250 MB per window" figure stated in AGENTS.md and in the `dyn_windows.rs:274` comment (I, from the snapshot).

## 2. Free reductions vs trade-offs

### FREE (no loss of speed or features)
1. **Stop leaving the dev launcher resident:** about **194 MB private** (V, measured). `node nexus.mjs start` (134 MB) and `cdp_monitor.js` (60 MB) both stay alive. Use a launcher that exits after spawning, and make the CDP tap opt-in. This also removes `--remote-debugging-port` (`run.ps1:170`). Installed release builds are not affected.
2. **Flip `DEV_KEEP_WINDOWS_ALIVE` to false** (`dyn_windows.rs:281`): about **40-90 MB** whenever the sidebar has been opened once (I; PID 14124 renderer measured at 41 MB private / 86 MB WS). The code itself says this must be done before release. The cost is a WebView cold create on the next sidebar open, which is the documented design (AGENTS.md lazy windows).
3. **Remove the duplicate speaker-check model set** (`wakeword_oww.rs:4475`, `:4523`). One option: share the tract model plans (the `TypedRunnableModel` behind an `Arc`) and keep only separate streaming state. The other option: feed the speaker check the embedding frames the live engine already computed. I: about 5-15 MB saved, no latency cost (it may even be faster).
4. **Store the ack cache as i16** (`tts.rs:190`): I: about 2-3 MB. Trivial gain.
5. **Free the startup-only buffers:** for example, the boot connection-diagnostics and app-registry scan results, if they are retained (not checked; see §6).
6. **Duplicate NLU weights:** `nexus_nlu.onnx` and `nexus_nlu.onnx.data` are both exactly 16.78 MB (V, file listing). If the `.onnx` file already embeds the weights, the `.data` file is a stale duplicate (I). Check whether tract reads the `.data` file at all. If it does not, this saves only disk and installer size, not RAM.
7. **EcoQoS:** already present (`power.rs`, per AGENTS.md). It saves energy, not RAM.

### TRADE-OFFS
- **Stage window `MemoryUsageTargetLevel=LOW`** is already set on the always-visible stage (V: `mic_permissions.rs:41-51`). Microsoft documents LOW for **inactive** apps. Under LOW, memory "could potentially ... be swapped out to disk", and if a script then runs, "performance might be impacted" (V: URL in §3). The stage drives the orb's WebGL animation on every turn, so this may be costing wake-to-orb latency right now. Recommendation: **set NORMAL while a turn is active and LOW only after about 30 s idle.** That is better for speed and roughly neutral for RAM. (Not measured; flagged.)
- **Unload NLU after idle** (it is never freed today): +42 MB freed, but the reload costs latency. The existing wake-time prewarm (`wakeword_oww.rs:3175`) loads it while the user is still speaking, so with a 10-30 min idle unload the next command would probably not notice (I; needs measuring).
- **Qwen brain idle timeout** (admin-only): ≥500 MB freed. The next cold load is about 12 s, but the brain path is already non-blocking with a fallback.
- **ORT session options for the always-on ort sessions (VAD; Smart Turn when enabled):** limit each session to 1-2 intra-op threads (`with_intra_threads`), because by default every session builds its own thread pool. This saves thread stacks and arena space, I: a few MB each, with negligible speed cost for models this small. **Disabling the arena or the memory pattern on Kokoro saves about 100 MB but is about 34 % slower** (AGENTS.md measurement): do not do it.
- **Working-set trimming** (`EmptyWorkingSet` on idle sidecars or WebView processes): only moves pages to the standby list, so commit is unchanged. Re-touching them costs soft faults, or hard faults under pressure (V: §3). Task Manager's "Memory" column improves; real RAM pressure barely changes. Not recommended except one trim after long idle.
- **Moonshine `tiny_streaming` instead of `small`:** saves RAM but worsens accuracy (12 % vs 7.84 % WER, per AGENTS.md). The live setting is already `small_streaming` (V). Leave it.
- **WebGL particle count, GPU process** (101 MB private / 182 MB peak, V): halving the particles (6240 → about 3000), or using a smaller canvas/backbuffer for the fullscreen stage, would probably reduce GPU-process memory (I). It is a visual trade-off.
- **Chromium flags** (`--js-flags=--scavenger_max_new_space_capacity_mb=8`, `--disk-cache-size`): small savings. Microsoft says production apps should not rely on these flags. All windows must pass **identical** arguments, or creating the second environment fails (V: §3).

## 3. External research

- WebView2 `MemoryUsageTargetLevel`: LOW is intended for inactive apps; memory may be swapped to disk; scripts that run afterwards may be slow; the app must restore NORMAL itself. https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_19 (V, fetched; high confidence)
- WebView2 process model: one browser process per user-data folder, renderers shared across WebViews depending on site isolation, separate folders create separate process groups, and identically configured environments must match. https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model (V, search summary; high confidence)
- WebView2 browser flags: not supported long-term in production; `AdditionalBrowserArguments` replaces rather than appends; the scavenger js-flag exists; `disk-cache-size` exists. https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags (V, search summary; medium confidence). I found nothing official on `--renderer-process-limit` or `--max-old-space-size` for WebView2 (unverified).
- Tauri vs Electron memory: similar, since both are Chromium-based. https://github.com/tauri-apps/tauri/issues/5889 (V, search summary; low relevance)
- ONNX Runtime memory levers: the arena trades peak memory for speed; the memory pattern helps fixed shapes but hurts dynamic shapes; prepacking is on by default (disabling it trades speed for memory); `session.use_device_allocator_for_initializers`; `use_ort_model_bytes_directly` applies to the ORT format only. Sources: https://www.sidefx.com/docs/hdk/onnxruntime__session__options__config__keys_8h_source.html , https://huggingface.co/csukuangfj/ios-onnxruntime/blob/main/1.22.2/onnxruntime.xcframework/Headers/onnxruntime_session_options_config_keys.h , https://kiwix.lema.org/stackoverflow.com_en_all_2023-11/questions/74173750/onnx-runtime-memory-arena-reuse-and-pattern (V via search; medium confidence. The official ORT tuning page was not fetched.)
- Working-set trimming: trimmed pages go to the standby list (transition pages), re-access is a soft fault, frequent trimming causes faults, and it does not really save memory. https://learn.microsoft.com/windows/win32/memory/working-set , https://www.controlup.com/script-library-posts/trim-process-working-sets/ , https://hacklines.com/en/editions/317087/click (V via search; medium confidence; the last source is a single-author test)
- Not researched (time): Moonshine ONNX vs alternatives memory, and Python lazy-import savings. STT RAM only matters on the no-Groq / local-only path, which is already killed after 60 s idle.

## 4. Three configurations

Baseline (V, 16:10): app 334 MB private / 507 MB WS; plus 194 MB private for the dev launcher.

**(1) Zero-risk:** expected about **-235 to -300 MB private** in the owner's dev setup; about **-45 to -100 MB** for an installed build.

| Change | File | Saving | Evidence |
|---|---|---|---|
| Launcher exits after spawn; CDP tap and debug port opt-in | `nexus.mjs`, `scripts/run.ps1:170`, `:263` | 194 MB | measured |
| `DEV_KEEP_WINDOWS_ALIVE=false` | `dyn_windows.rs:281` | 40-90 MB after the first sidebar use | measured renderer, I attribution |
| Share the speaker-check models | `wakeword_oww.rs:4444-4526` | 5-15 MB | I |
| Ack cache as i16 | `tts.rs:190` | 2-3 MB | I |
| Stage LOW only when idle, NORMAL during turns (speed fix, RAM neutral) | `mic_permissions.rs:41-51`, plus a turn hook | ±0 | doc |

**(2) Balanced:** adds about **-45 to -60 MB** while idle, with a small cold-load cost that is hidden by the wake-time prewarm.
- NLU idle unload after 15 min (`nlu_local.rs`: add an `unload()` that drops `MODEL` and resets `INIT_ATTEMPTED`, plus a timer). Saves 42 MB (AGENTS.md measurement). Cost: the first command after a long idle may wait for the prewarm (I: hundreds of ms, needs measuring).
- `with_intra_threads(1)` for the VAD and Smart Turn sessions (`vad.rs:31`, `turn_detect.rs:170`). I: 3-10 MB.
- Admin-only: Qwen brain 30-min idle kill (`lazy_brain.rs`). ≥500 MB when it was in use.

**(3) Aggressive:** adds about **-50 to -150 MB**, with explicit trade-offs.
- Halve the orb particles and render the stage canvas below native DPI. Saves GPU-process memory, I: 20-60 MB. Visual cost.
- Chromium flags `--js-flags=--scavenger_max_new_space_capacity_mb=8 --disk-cache-size=…`. I: 5-20 MB; more GC; unsupported in production.
- Kokoro: shorten the online unload grace from 10 to 2 min. Only matters around network blips. Cost: a 1.7 s cold reload if the network drops again.
- One working-set trim of the WebView utility processes after 10 min idle. The visible WS drops; commit is unchanged.
- Rejected even here: disabling the Kokoro ORT arena (-34 % speed, AGENTS.md) and Moonshine tiny (accuracy loss).

## 5. Measurement protocol (describe only; run manually by the owner)

1. Launch NEXUS the way it will ship (the installed exe, or `nexus.exe` started directly, without `nexus start`), then wait 2 minutes.
2. A script (PowerShell) finds `nexus.exe`, walks the descendants with `Win32_Process` by `ParentProcessId` (including the WebView2 tree, whose browser process has `--webview-exe-name=nexus.exe`, and the Python/node children), and records `WorkingSet64`, `PrivateMemorySize64` and `PeakWorkingSet64` per process every 5 s to CSV.
3. Scripted phases, each 2 min: idle → 10 voice turns (wake → command → reply) → open and close the sidebar 3 times → offline TTS line (unplug the network) → 12 min idle (to observe the unload timers).
4. Record latency from the existing `[LAT]` log lines (wake→first audio), `[CUE]`, and the Kokoro load time. **Pass rule:** median `[LAT]` stays within +5 % of the baseline, and no feature check fails.
5. Compare the CSVs before and after: idle private, peak private, and the private total after the sidebar closes (it should return to baseline once windows are destroyed).

## 6. Could not verify

- Which window renderer 14124 belongs to: the CDP `/json/list` returned empty, so the sidebar attribution is inferred.
- Per-model resident size inside nexus.exe (wake, VAD, speaker, NLU, ack cache): this needs a heap profiler or a load/unload A/B test. The numbers in §1 for these are inferred.
- Whether the NLU `.onnx.data` file is a real duplicate.
- Whether the stage's LOW memory target is currently hurting wake latency (not measured).
- Whether there are retained startup buffers (diagnostics, app registry scan).
- RAM of the Moonshine, Qwen, and agent-watch sidecars on this build (none were running). The AGENTS.md figures for them are reused unchecked.
- Official ONNX Runtime tuning docs (seen only through mirrors) and whether Tauri 2 passes per-window `additional_browser_args`.
