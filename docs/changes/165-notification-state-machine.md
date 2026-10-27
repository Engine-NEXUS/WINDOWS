# 165 — Notification state machine: failed / still failing / fixed (2026-10-25)

Built for Task P1 (Rust notification state machine, repo `D:\ULTRON`).

## 1. Goal and Problem

A connected service must never repeat itself and must report recovery. Previously, recurring GitHub CI emails for the same workflow in a repository arrived as new Gmail message IDs and new thread IDs, causing repeated spoken alerts or generic cooldown cards without state tracking.

This task introduces a pure notification state machine (`notify_brain.rs`) backed by SQLite persistence (`memcore::store::Store`). Per entity (e.g. `github:zync:build` or `github:zync:pr`), the brain remembers whether the entity is `ok` or `failing`:
- First failure is announced once (`PR failed in zync, sir.` / `Build failed in zync, sir.`).
- Subsequent failures for the same ongoing issue stay silent.
- Recovery is announced once (`Zync is green again, sir.`).
- Persistent failures continuing past a long lull (`STILL_FAILING_MS = 6 h`) are announced as still failing (`Build still failing in zync, 4 runs, sir.`).
- Both GitHub CI emails and the future GitHub Actions watcher (task P2) collapse into this exact same entity state machine.

## 2. Files Changed

- `src-tauri/src/notify_brain.rs` (new):
  - Pure core with injected clock `now_ms: u64` (`Obs`, `Kind`, `Fact`, `Entity`, `Line`, `Decision`, `Book`).
  - Derived entity ID: `{service}:{project_lowercase}:{pr|build}`.
  - Bounded memory book (capped at 300 entities with LRU `last_seen_ms` eviction; 30-day prune).
  - Deterministic line renderer `render(&Line, Option<&str>)` adhering strictly to <= 12 words with tail-address grammar (`tail`).
  - Process glue: `BOOK: Mutex<Option<Book>>`, `observe_fact<R>(app, fact, baseline)`, and `say<R>(app, line)` submitting Medium urgency events through `proactive_policy::submit_event` with event key `nb:{entity_id}:{failed|still|fixed}` and unique ID `nb_{entity_id}_{timestamp_ms}`.
- `src-tauri/src/lib.rs`:
  - Registered `pub(crate) mod notify_brain;` next to `pub(crate) mod notify;`.
- `src-tauri/src/memcore/store.rs`:
  - Schema: added table `entity_state(id TEXT PRIMARY KEY, failing INTEGER NOT NULL, since_ms INTEGER NOT NULL, fail_count INTEGER NOT NULL, last_ref TEXT NOT NULL, spoken_failure INTEGER NOT NULL, last_spoken_ms INTEGER NOT NULL, last_seen_ms INTEGER NOT NULL)`.
  - Added `DELETE FROM entity_state;` to `Store::wipe()`.
  - Added 30-day prune to `Store::housekeeping(now: i64)`.
  - Added `EntityRecord` struct and methods `Store::entities_load()` and `Store::entity_save()`.
- `src-tauri/src/memcore/mailwording.rs`:
  - Factored private helper `detect_github_failure_kind(subject: &str) -> Option<Kind>` shared by `github_plain` and `github_failure_fact`.
  - Added `pub fn github_failure_fact(m: &MailMeta) -> Option<Fact>` for GitHub-category failure mail with extracted repo, kind (Pr vs Build), and `run_ref = m.id`.
- `src-tauri/src/memcore/mailwatch.rs`:
  - In `fire_alerts`, before wording/submitting each single mail: checks `github_failure_fact(&m)`. If present, calls `notify_brain::observe_fact(app, &fact, false)`. On `Decision::Say(line)`, speaks via `notify_brain::say(app, line)`. On `Decision::Silent`, suppresses speech while preserving mail storage/marking.

## 3. State Table

The state machine implements this exact state table in `Book::observe(&mut self, f: &Fact, now_ms: u64, baseline: bool) -> Decision`:

| Current State | Observation | Condition | Transition & Decision |
|---|---|---|---|
| Unknown or Expired | Failed | Unknown entity or `failing` entity with `now - last_seen_ms >= FAIL_FORGET_MS` (12 h) | Treated as ok (forgotten failure becomes news again). Transitions to Row 2. |
| ok | Failed | `baseline == true` | Records `failing=true`, `since=now`, `fail_count=1`, `last_ref=run_ref`, `spoken_failure=false`. Returns `Decision::Silent`. |
| ok | Failed | `baseline == false` | Records `failing=true`, `since=now`, `fail_count=1`, `last_ref=run_ref`, `spoken_failure=true`, `last_spoken_ms=now`. Returns `Decision::Say(Failed{count: 1})`. |
| failing | Failed | Same `run_ref == last_ref` | Same run re-observed. Refreshes `last_seen_ms=now`. Returns `Decision::Silent`. |
| failing | Failed | New `run_ref != last_ref`, `spoken_failure == true`, `now - last_spoken_ms >= STILL_FAILING_MS` (6 h) | `fail_count += 1`, `last_ref=run_ref`, `last_spoken_ms=now`, `spoken_failure=true`. Returns `Decision::Say(StillFailing{count: fail_count})`. |
| failing | Failed | New `run_ref != last_ref`, `spoken_failure == false`, `!baseline` | `fail_count += 1`, `last_ref=run_ref`, `last_spoken_ms=now`, `spoken_failure=true`. Returns `Decision::Say(Failed{count: fail_count})` (first time owner hears). |
| failing | Failed | New `run_ref != last_ref`, within 6 h window or baseline | `fail_count += 1`, `last_ref=run_ref`. Returns `Decision::Silent`. |
| failing | Fixed | `spoken_failure == true` and `!baseline` | Records `failing=false`, `fail_count=0`, `spoken_failure=false`. Returns `Decision::Say(Fixed)`. |
| failing | Fixed | `spoken_failure == false` or `baseline == true` | Records `failing=false`, `fail_count=0`, `spoken_failure=false`. Returns `Decision::Silent`. |
| ok | Fixed | Any | Records ok, refreshes `last_seen_ms=now`. Returns `Decision::Silent` (routine successes are never announced). |
| Any | Any | Every call | `last_seen_ms = now_ms`. |

## 4. What Is Persisted

The `entity_state` SQLite table in `%APPDATA%\com.nexus.assistant\nexus.db` stores only minimal structural metadata:
- `id TEXT PRIMARY KEY`: derived entity key (e.g. `github:zync:build`).
- `failing INTEGER`: boolean (1 for failing, 0 for ok).
- `since_ms INTEGER`: epoch millisecond timestamp when the current failing state began.
- `fail_count INTEGER`: consecutive failure count for the current failure streak.
- `last_ref TEXT`: opaque run reference (e.g. mail ID or workflow run ID).
- `spoken_failure INTEGER`: boolean indicating if a failure was actively spoken to the owner.
- `last_spoken_ms INTEGER`: epoch millisecond timestamp of the last spoken notification for this entity.
- `last_seen_ms INTEGER`: epoch millisecond timestamp of the most recent event seen.

**Zero private content stored:** No commit message, branch name, author, email body, snippet, sender name, or repository content is persisted in `entity_state`. Entries not seen for 30 days are pruned in routine store housekeeping.

## 5. Verification and Tests Added

- `notify_brain::tests`:
  - `test_entity_id_derivation`: verifies `entity_id` format for `Kind::Pr` and `Kind::Build`.
  - `test_state_table_row1_unknown_treated_as_ok`: unknown entity + Failed -> `Say(Failed{count: 1})`.
  - `test_state_table_row1_forgotten_failing_treated_as_ok`: failure older than 12 hours forgotten -> treated as ok, spoken again.
  - `test_state_table_row2_ok_failed_baseline_vs_live`: verifies baseline failure is silent vs live failure spoken.
  - `test_state_table_row3_failing_failed_same_ref_is_silent`: same run reference re-observed stays silent.
  - `test_state_table_row4_failing_failed_new_ref`: new run ref inside 6 hours stays silent; past 6 hours speaks `StillFailing`.
  - `test_state_table_row4_failing_failed_unspoken_first_notice`: baseline failure followed by live failure speaks first notice `Failed{count: 2}`.
  - `test_state_table_row5_failing_fixed`: spoken failure then Fixed speaks `Fixed`; unspoken failure then Fixed stays silent; baseline Fixed stays silent.
  - `test_state_table_row6_ok_fixed_is_silent`: ok + Fixed stays silent.
  - `test_state_table_row7_last_seen_updated_every_call`: verifies `last_seen_ms` refresh across calls.
  - `test_book_cap_eviction_and_prune`: verifies 300 cap LRU eviction and 30-day pruning.
  - `test_render_all_lines_under_12_words`: verifies word count <= 12 and exact phrasing with/without address across all `Line` variants.
- `memcore::store::tests`:
  - `entity_state_round_trip_and_prune`: verifies save, load, 30-day housekeeping prune, and `wipe()`.
- `memcore::mailwording::tests`:
  - `test_github_failure_fact`: unit tests PR run, push run, no repo, non-failure mail, and non-GitHub mail.
- `memcore::mailwatch::tests`:
  - `notify_brain_mail_failure_sequence`: pure `Book::observe` sequence for mail-failed -> Say(Failed), mail-failed(new id) -> Silent, later Fixed -> Say(Fixed).

## 6. What Was NOT Run Live

- Cargo and git commit were not run (per strict task rules; reviewer will compile and test).
- Live Google polling with physical OAuth token querying live inbox for failing GitHub emails.
- Spoken audio playback through Kokoro TTS engine on hardware speakers.
- Passage of 6 hours / 12 hours wall-clock time on live system (tested via injected clock in pure unit tests).
