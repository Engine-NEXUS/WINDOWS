> **STATUS 2026-10-25: PHASES A, B, D, E BUILT (unit-tested, not live-proven); C and F redefined.** Records: `docs/changes/157`–`160`; test and review notes: `docs/testing/notification-brain-testing-and-research.md`. Deviations from this plan: hold-and-merge (original C) was not built; the original E (webhook/MCP) was dropped on purpose (webhook is a spoken-command path); in the build, C = cooldown tuning from live data and F = replay harness. Original F (group counts in memory/briefing/UI) is still open.

# 151 — The Notification Brain: event keys, cooldown, and short notifications (RESEARCH + PLAN)

Date: 2026-10-23. Owner ask: notifications from GitHub/Gmail/any MCP or API must arrive as **one short spoken line** ("the pr logs have failed from the nexus-engine/window"), repeated emails about the **same event within 1–5 minutes must be ignored**, and memory should group everything smartly. Machine used for analysis is admin, but **family/user builds have no admin brain — so the feature must be 100% deterministic with zero AI credits**.

## 0. The honest answer first

Today every fresh email is spoken separately. The cause is not the wake/STT pipeline (logs verify false wakes are suppressed correctly); it is that the notification speakers dedup by **message id**, not by **event**.

| # | Root cause (verified in code) | Where |
|---|---|---|
| 1 | Dedup key = `mail:{message_id}`; two emails about the same CI failure 2 min apart have different ids → both speak | `src-tauri/src/memcore/mailwatch.rs` (fresh-then-speak in `fire_alerts()`), `src-tauri/src/proactive_policy.rs` (`seen: HashSet<String>` on alert id) |
| 2 | Batch summarization only fires when **≥3 emails land in the same 90 s poll**; a 1–5 min gap straddles two polls → each speaks alone | `mailwatch.rs` (`batch_text()`, ≥3 rule) |
| 3 | Short one-liners exist **only for GitHub** (`local_plain()`); other categories fall back to a template that speaks the **full subject** (clipped 90 chars) | `src-tauri/src/memcore/mailwording.rs`, `mailtriage::alert_text()` |
| 4 | No coalescing key, no cooldown per topic, no "still failing" progress grammar anywhere | whole pipeline |
| 5 | Two independent Gmail pipelines (legacy 15 s sentinel + memcore 90 s watcher) can speak about the same thread | `src-tauri/src/google/sentinel.rs`, `memcore/mailwatch.rs` |
| 6 | MCP/webhook messages have no proactive channel: `webhook.rs` turns external JSON into a normal orchestrator turn — unqueued, undeduped | `src-tauri/src/webhook.rs` |

What already works and is kept: per-id dedup, sender mute + 10-min window, urgency triage (Medium+ speaks; Low is stored only), `proactive_policy` tiers (20 s gap, ≤6/hour, meeting awareness, snooze), WhatsApp learn-first + priority-people alerting, PII-redacted 7-day mail ring in memcore.

## 1. Design: one coalescing brain on the existing choke point

`proactive_policy::submit(app, id, text, urgency)` is already the single funnel for mailwatch, WhatsApp, leases, scheduler, briefing, agent watchers and the sentinel. The Notification Brain adds an optional **event key** and three little machines around it. No new speaker is created; nothing bypasses the policy.
1. **Event key** — `notify::event_key(source, subject, sender, extra)` → `(source, entity, kind)`:
   * sender/subject normalized (lowercase, strip `Re:`/`Fwd:`, `[builds]`-style tags, counters/`?`/dates), entity = GitHub `owner/repo` (reuse `mailwording::extract_github_repo()`) or CI run id when the subject carries one, else Gmail thread id when the fetched metadata exposes it, else a 12-word subject-stem hash.
   * Store: tiny ring (last 200 keys with last-spoken time and a `count-since-heard`), lives next to the policy's `seen` map.
2. **Cooldown + count-up** — same key **within 5 minutes** of the last spoken line for it → `proactive:card` only (sidebar), counter++. When a later non-suppressed event for the same key finally speaks, it uses the progress form.
3. **Progress grammar** — after a hold window, one line per group: first event = template line; repeats = *"Still failing — two more from nexus-engine/window since you last heard."* Count, then template tail. Never the full subject.
4. **Data flow unchanged**: watchers still fetch, triage, store to memcore (`mail:{id}` keeps its per-email dedup — the brain dedups *speech*, the store dedups *records*).

## 2. Implementation phases (each one ship-able)

| Phase | Deliverable | Files | Notes |
|---|---|---|---|
| **A** | `notify::event_key()` + key registry ring (pure functions, injected clock, full unit tests) | new `src-tauri/src/notify.rs` | normalization rules in §1.1; collision-safe (hash suffix) |
| **B** | Cooldown + count-up in `proactive_policy::submit`: optional `event_key` param; 5-min window → card-only; progress line on speak | `proactive_policy.rs`, callers: `mailwatch.rs`, `wa.rs`, `watch_runner.rs`, `agent_watch.rs`, `sentinel.rs`, `leases.rs` | id dedup unchanged; Medium tier only (High/Critical bypass cooldown — deadline/security alerts are not re-throttled) |
| **C** | Hold-and-merge: pending Mediums queue 60–90 s; same-key events merge into one count line; different keys each keep their line (≤2 lines, then the rest batch into "N more — say important emails") | `proactive_policy.rs` | bounded memory; never blocks Critical; expiry falls back to single item speak |
| **D** | Short templates for **all** categories: Supabase/database/warn/deadline/OTA/e2e network notices + generic; cap ≤15 words, subject clip 40 chars | `mailwording.rs`, `mailtriage.rs` | deterministic regex − zero credits; cloud rewrite stays opt-in for admin only |
| **E** | One funnel for MCP/API: `webhook.rs` and MCP bridge events submit with keys + urgency so they get the same cooling | `webhook.rs`, `mcp_client.rs` | optional later: ntfy-style self-hosted phone push for Critical |
| **F** | Memory + UI: group-per-key in memcore mail ring (event-key meta), "any important emails" digest and boot briefing report **group counts** ("nexus-engine/window failed twice overnight"), Hub notifications page groups by key | `memcore/store.rs`, `memcore/briefing.rs`, `orchestrator.rs::run_mail_digest`, frontend `MemoryPage.tsx` / actions page | retention unchanged (7-day ring, cap 300) |

### §2.1 Credit audit (the owner's hard requirement)

| Component | Credits | Why |
|---|---|---|
| Event key, cooldown, count-up, hold-merge | **0** | timers + hash maps |
| All short templates (Phase D) | **0** | on-device regex |
| Gmail polling (history API, metadata only, bodies never fetched) | **0** | OAuth — not AI |
| Spoken lines → Edge TTS | **0** AI credits | unofficial endpoint |
| Cloud AI rewrite of one-liners | **off by default for every build** | admin may later opt in; family builds never call it. (Groq STT for wake verification is pre-existing behaviour, unchanged.) |

Result: the whole feature is **zero AI credits** and completely independent of the admin brain (`127.0.0.1:39219` is not referenced).

## 3. Prior art we borrow (verified 2026-10-23)

| Source | Licence | What we take |
|---|---|---|
| NTFY topics + 5-level priority (github.com/binwiederhier/ntfy) | Apache-2.0/GPLv2 | topic = coalescing key analogy; their GitHub-Actions examples send `repo/commit/status` — CI alert shape matches ours |
| Apprise escalation chains + tag logic (github.com/caronc/apprise) | BSD-2 | priority-group dispatch with fallbacks maps onto Low→Critical tiers; tag AND/OR logic matches "notify only tagged-critical" mute groups |
| Apple Intelligence notification summaries (iOS/macOS) | — (design ref) | per-app summary toggles for the Hub; prioritization of notifications; **the Jan-2025 pause**: they disabled news/entertainment summaries after BBC misfires — validates deterministic templates for structured sources |
| Home Assistant notification grouping / throttle patterns | Apache-2.0 | grouping by coalescing key before display |

No project anywhere combines voice readout + topical cooldown + progress grammar — that part is NEXUS-specific (nothing to copy, patterns only).

## 4. Acceptance targets (each becomes a test)

1. Two GitHub failure emails for the same `nexus-engine/window` run 1 minute apart → **exactly one** spoken line + one sidebar card; second email shows as "still failing — one more".
2. Every proactive spoken line from the brain ≤ 15 words with the full subject never spoken.
3. Medium-tier breakthrough alerts still respect 20 s gap / ≤6 per hour; High/Critical still break through within ~8 s.
4. Sentinels and mailwatch never speak the same thread twice in one session.
5. `webhook.jsonl` external event → queued, dedup'd, ≤1 line, same grammar.
6. No category ever falls back to the 90-char template; fallback exists in tests as the final guard only.
7. Full local test suite: `cargo test --lib --features custom-protocol -- --test-threads=1`; goldens for event-key normalization; replay fixture with the owner's real failure emails.

## 5. Risks and honesty

* **Key collisions**: different incidents in the same repo → same key. Mitigation: CI run-id extracted from subject when present; without it, the cooldown window (5 min, short) bounds the damage; Hub card always preserves per-email records.
* **Thread id not always in metadata**: fallback chain in §1.1 is tested for the common GitHub/Gmail shapes only; exotic senders fall back to subject-stem hashing, which is conservative (may not merge occasionally).
* **Hold-and-merge adds ≤90 s latency to Medium notifications** — acceptable per owner ("smart beats instant"; Critical is never delayed).
* **Zero new memory tiers** — mail ring usage unchanged; key meta is a small in-memory ring (not persisted).

## 6. Sources

* Code on `D:\ULTRON` (2026-10-23): `mailwatch.rs`, `mailtriage.rs`, `mailwording.rs`, `proactive_policy.rs`, `sentinel.rs`, `google/mail.rs`, `webhook.rs`, `memcore/store.rs`, `memcore/briefing.rs`, `wa.rs`, `leases.rs`, `agent_watch.rs`, `watch_runner.rs`.
* Repo docs: `docs/features/100-memory-core-plan.md`, `docs/memory-core/01-overview-and-architecture.md`, `docs/features/113-…proactivity-center.md`, `docs/features/78-sentinel-orb…`, `docs/changes/83-proactive-speech-policy.md`.
* Prior art: ntfy repo + docs; Apprise repo/README; Apple Support "Summarize notifications" pages; Macworld/The Verge coverage of iOS 18.3 pausing news summaries; Apple newsroom Apple Intelligence announcement.
* General research queries: web search 2026-10-23.
