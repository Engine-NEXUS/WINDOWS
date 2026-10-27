# 159 — Notification brain Phase E: WhatsApp and watch funnelled into the event-key brain (2026-10-25)

## Scope

Implements phase E of [docs/features/151-notification-brain-event-key-cooldown-and-short-notifications-plan.md](../features/151-notification-brain-event-key-cooldown-and-short-notifications-plan.md): the remaining notification sources submit through `proactive_policy` with feature-151 event keys, so one logical event speaks once per 5-minute Medium window instead of repeating (or, as before, going permanently silent).

## Root cause found while wiring

`wa.rs` submitted every alert with the id `wa_{jid}`. The engine's id dedup (`seen: HashSet<String>`) drops duplicates **forever**, so after the first alert from a chat, every later message from that priority person was silently swallowed until app restart — even High-urgency ones. The event-key work exposed this because the correct shape is the opposite: unique id per fire (so the engine ever sees it) plus one stable event key (so Medium repeats coalesce with a count-up, and High still breaks through).

## Implemented

- `memcore/wa.rs` `fire()`: id is now `wa_{jid}_{timestamp}` (unique per fire) and the submit carries the event key `wa:{jid}` via `submit_event`. Medium priority-people repeats now coalesce into one spoken line per 5-minute window with a `Sir, N more since you last heard — …` count-up after the lull; High priority people still speak on every fire (deliberate break-through).
- `watch_runner.rs` `tidy_up()`: the Medium tidy-notes submit carries the stable key `watchpower:tidy`, so two tidy sessions within 5 minutes do not speak twice.
- Deliberately unchanged:
  - `agent_watch.rs` and the `watchpower_*` Say/SayAndAsk/follow-up submits — all High urgency, which bypasses the cooldown by design; their ids are already one-shot per watch/action.
  - `webhook.rs` `/trigger` — it routes a transcript through the normal orchestrator pipeline; it is the equivalent of the user speaking, not a notification, so throttling it would lose commands.
  - `mcp_client.rs` — contains no `proactive_policy` submits; there is nothing to funnel (MCP-bridge chat reads reach the brain only via `wa.rs`, which is now keyed).

## Phase boundary

Phase C (cooldown tuning from live observation) and F (replay harness) remain per plan.

## Verification

- `cargo test --lib -- --test-threads=1 wa:: proactive_policy:: watch_runner::` — 73 passed, 0 failed. The existing engine test `same_event_key_within_five_minutes_speaks_once_and_counts_the_rest` already covers the exact WhatsApp shape (unique ids, one key, cooldown, count-up).
- A live second-phone WhatsApp test was not run in this session (the `memcoreWhatsapp` flag is off by default until the read-receipt test passes).

## Credit audit

Zero AI credits: ids and keys are string building; the gate is the existing Phase-B arithmetic.
