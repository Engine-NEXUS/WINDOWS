# Change 81 — One shared list of ways STT writes "NEXUS" (Phase B4)

**Defect (audit #4):** the stage-2 wake verifier (`wakeword_oww.rs`) and `intent_parser::strip_wake_prefix` kept separate name lists. The verifier knew lexus/nixus/nexis…; the stripper only knew the exact word "nexus", so a command that began with an accepted soundalike ("lexus open chrome") never had the name removed and parsed to nothing. Neither handled the split forms the owner's real recordings produced ("next us", "next sis" — see the 2026-10-01 wake-data poisoning rescue).

## Change

- New `src-tauri/src/wake_names.rs` (always compiled): `EXACT`, `NEAR_WORDS` (+ nexxus, nexes, nixes), `NEAR_PAIRS` (next us, next sis, next sense, necks us), `GREETINGS`, `classify()`, `wake_prefixes()`.
- `verify_transcript_gated` now = `classify`: exact → fires at any neural score; near (single word **or pair**) → needs stage-1 prob ≥ 0.6 (unchanged two-key rule).
- `strip_wake_prefix` builds its prefixes from the same list ("<greeting> <name>" and bare "<name>", longest first; word-boundary rule unchanged).
- Still deliberately excluded (everyday words): texas, next, access, "next is", "next test", plural "nexuses".

## Policy change to be aware of

`next us` / `next sis` used to be hard-rejected by the verifier at any score (`test_gated_rejects_collisions`). They are now near forms: accepted when the neural model is also confident (≥ 0.6). Reason: real owner "nexus" recordings were transcribed as "next sis/next sense". Risk: a TV phrase "next us" together with a ≥ 0.6 neural score. Measure it, don't guess: record the soundalike clips (`voice_eval_set.py … --section negative` includes "next us") and run `wake_eval --stage2`; the scorecard's "Soundalike / everyday-word false alarms (with speech check)" row is the verdict. If it rises, drop the pairs from `NEAR_PAIRS` — one line.

## Tests

`wake_names` (2), `intent_parser::test_strip_wake_prefix_soundalike_forms` (9 stripped forms, 6 untouched, 4 end-to-end labels), verifier near-matrix extended (next us, Next, sis., nexxus, nixes), collisions test updated. Rust lib 941/941 serial; `e2e_voice_fixture` 2/2.
