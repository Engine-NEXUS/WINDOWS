# Change 84 — Settings that agree with each other (Phase B8)

Three contradictions from the voice-accuracy audit (#9).

## 1. `wakeSensitivity` did nothing → now moves the threshold, live

The setting was stored and shown but read by no code. `WakeEngine` now keeps the calibrated threshold (`base_threshold`, `acoustic_profile.json` `kws_threshold`, 0.68 as shipped) and `set_sensitivity(label)` applies `sensitivity_threshold(base, label)`:

| label | threshold (base 0.68) |
|---|---|
| low (stricter) | 0.75 |
| medium (default) | 0.68 — identical to before |
| high (more willing) | 0.60 |

Unknown labels = medium; result clamped to [0.40, 0.90]. Applied at engine start (`run()`) and immediately on every Save (`save_settings` → `apply_wake_sensitivity`, no restart). Note: the current Command Hub sidebar has no sensitivity control (only the old `settings` window did); this makes the stored value effective for whoever sets it. Choose the value from the scorecard's recall vs false-alarms/hour rows, not by feel.

## 2. `localSttOnly`: UI said `true`, runtime read `false`

The struct default (and both frontend defaults) was `true`; every raw-JSON reader (`stt.rs` helper, `lazy_stt.rs` pre-warm, wake path) used `false` when the key was absent. A user who only pasted a Groq key saw "Local STT only" ON in Settings, and the first Save persisted it — silently ignoring their key (the trap change 78 only explained). One constant now: `commands::DEFAULT_LOCAL_STT_ONLY = false` + `local_stt_only_from_json`, used by the struct default, `read_local_stt_only`, `lazy_stt` and the wake path; frontend defaults are `false`. Existing files with an explicit value are unaffected. Without a key, local STT is still used regardless.

## 3. One bad field reset every setting

`get_settings` did `serde_json::from_str(..).unwrap_or_default()`. Any field with the wrong type, or an older file missing a newer required key, failed the whole parse → all defaults shown → the next Save overwrote the user's file. `parse_settings_lenient` merges field by field into defaults and reports the dropped keys (logged); only the offending field falls back. A file that is not JSON at all is copied once to `settings.json.corrupt` before defaults are used.

## Tests (+5)

`sensitivity_threshold` (wakeword), and in `commands.rs`: one-default-everywhere, lenient parse keeps valid fields, old file missing fields, valid/garbage cases. Rust lib 952/952 serial; frontend vitest 164/164 (+ the pre-existing unloadable `identityBanner.test.ts`).
