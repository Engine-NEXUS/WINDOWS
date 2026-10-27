# 160 — Notification brain review pass: gate-before-wording, release-on-expiry, High guard, cloud off (2026-10-25)

Cross-check of records 157–159 against the code (plan: `docs/features/151-…plan.md`; notes: `docs/testing/notification-brain-testing-and-research.md`).

## Found and fixed

1. **Missing function.** Record 157 said `proactive_policy::merge_agent_alerts_into_return()` was implemented; it was not (only the call in `return_review.rs` existed). The lib still compiles because `return_review.rs` is not declared in `lib.rs`. It was briefly added here, then removed again: it had no compiled caller and the dead-code ratchet rejects it. When `return_review.rs` is wired into `lib.rs`, that function (drop queued `agent_watch_*` alerts, keep their ids in `seen`) must be written together with it.
2. **Cloud rewrite default.** `memcoreMailPlain` defaulted to ON, contradicting the "default-off / zero credits" claim. Now OFF in `NexusSettings`, `memcore::status`, `mailwording::plain_alert_text` and the Hub toggle. Existing saved `true` values are kept.
3. **Cooldown ran after wording**, so a suppressed repeat could still call a cloud model. Added read-only `Engine::would_suppress` / `proactive_policy::key_in_cooldown`; `mailwatch::fire_alerts` uses the free template text for a held repeat and skips `plain_alert_text`.
4. **Batch key.** A 3-or-more-mail batch was keyed by its first mail only, so a mixed Medium batch could be muted by an unrelated earlier event. `batch_key` now keys a batch only when every mail is the same event.
5. **Key marked heard before it spoke.** If the accepted alert expired in the queue as a card (or hit the hourly limit), repeats stayed suppressed and the "N more" count was wrong. `KeyNote` now tracks the owner alert id; a card-expiry releases the key.
6. **High repeats were unprotected** (hackathon, exam and database warnings are High). Added a fixed 2-minute High guard (`HIGH_KEY_COOLDOWN_MS`); escalation to a higher urgency and Critical still break through; the key ring is now least-recently-used.
7. Removed an unused `HACK_STATUS` import; corrected stale comments and docs (plan status, phase labels, credit audit).

## Not changed (listed as open in the testing doc)

Sentinel overlap, the rolling Medium window's long-storm silence, original phase F (group counts), live proof.

## Verification

`cargo check --lib` clean (no warnings). `cargo test --lib -- --test-threads=1 notify:: proactive_policy:: mailwording mailtriage mailwatch wa:: watch_runner:: memcore_mail_plain` — 136 passed. Four mutations (Medium window = 0, High guard off, release removed, batch key first-only) each killed. Frontend `tsc --noEmit` clean; `vitest run src/settings-sidebar` 105 passed. The full CI sweep and the live replay (testing doc section 7) were not run.
