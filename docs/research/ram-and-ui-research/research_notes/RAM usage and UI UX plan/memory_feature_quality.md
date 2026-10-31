# NEXUS memory feature (memcore, profile, habits, routines, corrections, review, curator, session restore, briefing): quality and resource cost

Method: read-only code and doc review of D:\ULTRON (branch fix/loading-state-and-local-ci). No tests were run, NEXUS was not touched, and the contents of the real store were not read (only file size and mtime). No web sources were used or needed; every citation below is a local file. Labels: CONFIRMED (code, test or doc says so), ESTIMATED (my arithmetic from code caps), UNVERIFIED (nobody has measured it).

Source file shorthand: `memcore/` = D:\ULTRON\src-tauri\src\memcore\. `docs/mc/07` = D:\ULTRON\docs\memory-core\07-memory-v2-baseline.md.

## 1. Does recall work, and what is the evidence?

### Takeaway
Recall is measured only on a synthetic 40-fact (and 100-fact) store with a hand-written question set, by the same author who wrote the synonym families. Direct questions hit 21/21, dev paraphrases 19/19, held-out paraphrases 17/20 (hit@3). Nothing has been measured on the real store, a blind question set, a release build, or a live spoken turn, and the test assertions are far looser than the printed numbers.

### Cited Findings
- CONFIRMED. Test `memcore::recall_baseline` has 40 facts (20 targets, 20 distractors), 40 questions (21 direct, 19 paraphrase) and a 20-question held-out list. It is in `memcore/mod.rs` lines 905-1133. A second test builds a 100-fact store (60 filler) and checks whether the fact appears in the cloud context pack.
- CONFIRMED (doc). Results recorded in `docs/mc/07`: M0 baseline (2026-10-18, debug build) direct 20/21, paraphrase 0/19. After M1 synonyms and stemming: direct 21/21, paraphrase (dev) 19/19, held-out 17/20. Held-out misses: "who do I report to at work" (manager_name), "what name do you use for me" (nickname), "where did I grow up" (hometown).
- CONFIRMED (doc). 100-fact store, 60 questions: fact present in cloud context 57/60 with profile OFF and 57/60 with profile ON. The profile did not raise recall; the doc says so itself. `context_pack` p50 4.4 ms, p95 6.5 ms (debug build), largest pack 1,628 chars (~407 tokens), profile alone 1,397 of 1,400 chars.
- CONFIRMED (code). The test's pass thresholds are weak: it asserts only `direct >= 19` and `para >= 1` (mod.rs lines 1081-1082) and, for the pack test, `with >= without`, pack <= 2600 chars, p95 < 100 ms. A regression from 17/20 held-out to 0/20 would not fail the build. The printed numbers are informational, not gated.
- CONFIRMED (doc). The doc itself warns: "The dev-set 19/19 is not evidence of generality", families were written after the dev questions existed, held-out has the same author so it is an upper bound, and "a blind set written by someone else is the real test" (`docs/mc/07`, M1 section).
- CONFIRMED (code). Retrieval is lexical: SQLite FTS5 BM25 plus a second synonym/stem pass ranked at 0.7x (`SYNONYM_PENALTY`), rank = 0.6 relevance + 0.2 recency (30-day e-fold) + 0.1 use frequency (clamped 10) + 0.1 pinned, scaled by confidence (`memcore/store.rs` lines 53, 268-275). No embeddings.
- CONFIRMED (docs). Corrections: 10 scripted tests; replay of the PC's real `missed_intents.jsonl` (173 utterances, 74 pairs) gave 0 detections. The doc states this proves only "stays quiet", not that it catches real corrections, because that log holds failed-to-parse commands, not wrong-target commands (`docs/mc/07`, M2).
- CONFIRMED (docs). Routines: 17 tests on synthetic logs; "There is no real command history yet", never tried by voice (`docs/mc/07`, M3).
- CONFIRMED (docs). Review (M4): 15-16 tests with a mock model, 5 mutation checks. "Never run against a real model"; prompt quality unmeasured (`docs/mc/07`, M4).
- CONFIRMED (docs). Curator (M5): 9 tests, 5 mutations (one initially survived, test added). "Never run on real data"; with fewer than ~20 profile facts it changes nothing.
- CONFIRMED (grep). Test counts per file: store.rs 21, mod.rs 15, session.rs 24, briefing.rs 23, mailwording.rs 37, routines.rs 17, review.rs 16, timetable.rs 16, corrections.rs 11, curator.rs 9, profile.rs 6, synonyms.rs 6, memory.rs 28, conversation.rs 15 (`#[test]` counts).
- CONFIRMED (AGENTS.md 2026-10-27): the full Rust lib suite was reported 2,643 passing, so these tests were green at that time. I did not re-run them.

### Inferences
- The synonym layer is a hand-written concept dictionary. It will generalise only as far as the authors' vocabulary; the 3 held-out misses (manager, nickname, hometown) are exactly the "no shared word, no listed family" case. Expect real-world paraphrase recall below 17/20, plausibly well below, since real facts have messier keys than `dog_name`.
- 57/60 at 100 facts with the profile off or on suggests the cloud pack is bounded by the 8-fact cap (`MAX_PACK_FACTS`) and the 1,400-char profile, not by ranking, once the store has many facts.
- The recall tests never exercise episodes, mail, chat, or the legacy `memory.rs` path, so "the assistant remembers my chat" is backed by `conversation.rs` unit tests only.

### Gaps
- No blind (third-party-written) question set exists.
- No measurement on the real 844 KB store; reading it was out of scope and forbidden.
- No release-build timing; no live `[LAT] mem Nms` value recorded in docs.
- Whether the user's real facts are mostly user-told (pinned) or mined is unknown.

## 2. Does it learn the right things, forget correctly, and what is kept where?

### Takeaway
Learning is deliberately conservative (admit gate, two-sighting corrections, approval-gated review, 3-day-3-session routines). Forgetting has real mechanisms (age and count caps, decay, wipe with key rotation), but two documented discrepancies exist: the AGENTS.md claims about `habits.rs`, `memory_ipc.rs` and a top-level `profile.rs` do not match the working tree, and the egress log has an age cap but no row cap.

### Cited Findings
- CONFIRMED (code). Admit gate: empty, untrusted-as-fact, and secret-looking text refused; mail, slot, person and chat tiers skip the secret screen on purpose (`memcore/store.rs` lines 235-252). Values clipped at 500 chars (`MAX_VALUE_CHARS`).
- CONFIRMED (code, store.rs lines 22-70). Retention constants: egress 7 days (no row cap); episodes 30 days / 500; resume 7 days / 500; mail 7 days / 300; chat 7 days / 200; audit 30 days / 2,000; command_log 60 days / 3,000; corrections cap 300 (stale 30 days); routines max 30; pending suggestions max 30 (14-day TTL); decay 0.97 per idle day for unpinned derived facts, deleted below confidence 0.2 (about 53 idle days from 1.0). Pinned and user-told facts never decay.
- CONFIRMED (code). Profile cap 1,400 chars (`memcore/profile.rs` line 15); cloud pack budget 2,600 chars (doc 07, M1; mod.rs comment line 58); max 8 recall facts and 3 episodes per pack.
- CONFIRMED (doc, AGENTS.md hygiene entry). A one-time hygiene cleanup removed STT junk from memory, with a backup folder `_backup_hygiene_v1` created 2026-10-10 (also seen on disk).
- CONFIRMED (docs). Legacy path: `memory.rs` (core.json, episodes.jsonl: 500 episodes, 30 days, 800-char context, max 200 learned facts) and `conversation.rs` (24 raw turns, 6 live turns / 1,500 chars, brief 400 chars, 48 h replay age, 14-day brief age). On disk right now: episodes.jsonl 27,456 B, conversation.jsonl 8,288 B, core.json 69 B (file listing, sizes only).
- CONFIRMED (disk). `memory/memcore.enc` is 843,808 bytes, modified 2026-10-29 17:17 IST (today). Contents not read.
- CONFIRMED (code). Forget/wipe: wipe rotates the encryption key (crypto-erase) and clears queues; `hygiene`, `clear_tier`, `purge_episodes_where` exist. AGENTS.md also records that `forget` was widened to facts.json and `wipe_memory` to conversation/brief/diary/mail watches (docs/changes/107).
- CONFIRMED (doc). Curator never edits or deletes user-told facts; it retires routines unused 60 days and reorders the profile only when full (`docs/mc/07` M5). Near-duplicate merging and model summarisation are explicitly not built.
- CONFIRMED (working tree). `src-tauri/src/habits.rs`, `src-tauri/src/memory_ipc.rs` and a top-level `src-tauri/src/profile.rs` do NOT exist (`find` returns nothing; only `memcore/profile.rs` exists). `memory.rs` line 2 and 7 still mention `habits.rs` and `habits.json`. Commit 8f7aae6 ("ghost mode, screen analysis and memory pillars") is an ancestor of HEAD and touched habits.rs. Yet `docs/features/90-memory-profile-habits-and-memory-panel.md` and AGENTS.md describe `habits.rs` (6 tests), `memory_ipc.rs` and an "about-me profile (identity/can_do/needs/priorities)" file as built. Frontend `MemoryPage.tsx` line 555 still says NEXUS "learns your habits".
- UNVERIFIED. Why those files are absent (renamed into memcore, removed in a later commit, or lost in one of the working-tree collisions AGENTS.md records). I did not run git log -S to settle it. Treat the "habits" and "about-me profile (identity/priorities)" features as not present in this tree until checked.

### Inferences
- Because the capture hook for corrections and routines is `conversation::record_conversation_turn`, anything not recorded there (for example turns that fail before recording) is invisible to learning.
- The profile's ordering is "oldest first until full" unless the curator reorders. A new fact added to a full profile may silently never reach the cloud; the user would not know which facts made the cut except via the Memory page or egress log.
- Egress log growth: each cloud turn logs the full request plus the full memory text (up to ~2,600 chars plus transcript) for 7 days with no row cap. ESTIMATED: at 100 cloud turns/day and ~3 KB per row that is about 2 MB over 7 days, which would dominate the store. The real store being 844 KB is consistent with a mix of egress and records, but I did not inspect it.

### Gaps
- No evidence that a corrected command or review suggestion has ever occurred on real data.
- No test proves forget removes rows from the FTS table in every path (delete paths do remove `records_fts` rows explicitly in prune, clear_tier, purge; I did not audit every delete site).
- Secure-delete/SSD caveat is acknowledged in crypto.rs: shredding is hygiene, key rotation is the real guarantee.

## 3. Memory held in RAM and RAM estimate

### Takeaway
The store is one in-memory SQLite database (loaded from the decrypted snapshot) plus FTS index. Disk file 844 KB means roughly 1 MB of live DB plus short-lived 2-3 MB transient copies on every write. This is tiny against the 600-700 MB budget (single-digit MB), but a transient spike of a few copies of the DB occurs on each flush. Only the egress log and mail/chat/etc. caps bound it; nothing is truly unbounded except egress rows within 7 days.

### Cited Findings
- CONFIRMED (code). `Store::open` with a key opens `Connection::open_in_memory`, decrypts `memcore.enc` fully with `crypto::open`, and `deserialize_read_exact`s it. The DB is held in RAM for the process lifetime in a global `STORES` map of `Arc<Mutex<Store>>` (`memcore/store.rs` 282-331, `memcore/mod.rs` 63). `docs/changes/109` states "the Memory Core database is held in memory and persisted as ONE sealed blob".
- CONFIRMED (code). `flush_now` = `conn.serialize` (allocates a full DB copy) + `crypto::seal` (allocates a second, ciphertext copy) + `atomic_write` (temp file plus rename).
- CONFIRMED (disk). On-disk blob is 843,808 B. AES-GCM overhead is negligible, so the serialized DB is about 0.84 MB.
- ESTIMATED. Resident: DB pages in RAM ~0.85 MB, plus SQLite page cache and FTS structures (low single-digit MB), plus transient 2 x 0.85 MB during each flush. Total memcore footprint on the order of 2-5 MB steady; peak +2 MB per write. This excludes the legacy JSON files, which are read per call, not cached.
- ESTIMATED upper bound from caps (character counts): episodes 500 x ~300 chars, resume 500 x ~250, mail 300 x ~500, chat 200 x ~300, command_log 3,000 x ~60, audit 2,000 x ~100, corrections 300, plus facts. That sums to roughly 0.8-1.0 MB of text before indexes, FTS roughly doubles text columns. So the capped tiers can reach ~2 MB; the egress table is the only variable-size contributor (7 days, uncapped rows, up to ~3 KB each).
- CONFIRMED (docs). AGENTS.md 2026-10-17 measured idle tree 269 MB commit / 236 MB WS for all of NEXUS; Kokoro adds ~368 MB when loaded. Memory work is not a meaningful part of that.
- CONFIRMED (docs). Doc 109 itself says "revisit if memory grows by orders of magnitude" for whole-DB serialisation.

### Inferences
- The growth risk is not RAM but write amplification (section 4): cost per write scales linearly with DB size, and egress is the thing that grows it.
- The 600-700 MB ceiling is not threatened by memcore. It is threatened by TTS/NLU/STT, which belong to other notes.

### Gaps
- No measurement of actual process RSS attributable to memcore. A Task Manager delta with `memcore` on vs off (`"memcore": false` setting exists) would settle it; not done.
- Real row counts per tier on the user's store are unknown.

## 4. CPU, I/O and idle wakeups

### Takeaway
Memory causes many small background wakeups and several full re-seals per cloud turn. The idle wake rate is dominated by about eight timers at 10-60 s cadence, each of which re-reads and re-parses settings.json from disk. The per-30 s resume reseal was already fixed. Reseals are the main avoidable I/O cost.

### Cited Findings
- CONFIRMED (code). Timers in memcore (constants / sleeps):
  - resume sampler: every 30 s while user not idle (`resume.rs` SAMPLE_SECS=30), skipped under Focus Mode. Probes the foreground window in `spawn_blocking`, 4 s timeout.
  - session snapshot: std thread every 60 s (`session.rs` SNAPSHOT_EVERY_SECS=60), skipped if idle >= 30 min. Enumerates all top-level windows; Explorer folders via PowerShell (~0.5 s, cached 5 min per code comment). Writes only if content changed or `REFRESH_SECS` elapsed (`save_last`).
  - mail watcher tick 30 s; poll 90 s active / 8 min away (`mailwatch.rs`).
  - WhatsApp poller: 30 s tick; 45 s active, 5 min idle, 5 min when bridge down (`wa.rs`); default OFF per AGENTS.md.
  - leases tick 30 s (`leases.rs`), scheduler tick 30 s (`scheduler.rs`), leave-by tick 60 s (`leavewatch.rs`).
  - GitHub watcher: 30 s startup, 2-min polls (AGENTS.md), 15 s HTTP timeout; deploy watcher 5-min polls (AGENTS.md; `deploy_watch.rs` uses 300 s as base delay).
  - curator: first check after 10 min then every 15 min, runs at most every 20 h only when user away >= 5 min (`curator.rs`, doc 07). Review: first check 5 min, then every 10 min, cheap `due` check (`review.rs`).
  - briefing presence poll every 10 s but only for up to 20 min after boot, once (`briefing.rs` lines 52-53).
  - keep-awake poll: 3 s forever (`keep_awake.rs` lines 215-222) reading `ghost::drill_running()` and `orchestrator::has_active_request()`; this is outside memcore but also a constant wake.
- CONFIRMED (code). `memcore::flag()` reads and JSON-parses the whole settings.json from disk on every call (mod.rs 73-79); `enabled()` calls it; `with_store`/`store_handle` call `enabled` each time (mod.rs 82-99). So each tick and each store access pays a file read plus parse.
- CONFIRMED (code). Every mutation that calls `flush()` re-serialises, re-encrypts and atomically rewrites the whole file (store.rs 359-378). Cloud-turn path: `context_pack_with` calls `mark_used` twice (each flushes), `log_egress` flushes, episode logging flushes; the orchestrator also calls `log_egress` (orchestrator.rs line 3192). That is about 3-5 full reseals of ~0.84 MB per cloud turn, ESTIMATED from the call sequence.
- CONFIRMED (code comment, store.rs 1134-1136). The resume sampler previously resealed ~2,900 times/day; `touch` no longer flushes. `batch()` exists for deferring flushes but `mark_used` and `log_egress` do not use it.
- CONFIRMED (docs). Doc 07 baseline: pack build p50 6.3 ms / p95 9.9 ms (M0, debug), later p50 4.4 ms / p95 6.5 ms. So retrieval is cheap; the write path is the cost.
- UNVERIFIED. Wall time of one flush on this laptop (0.84 MB serialise + AES-GCM + write + rename). ESTIMATED low single-digit to tens of ms on SSD; not measured.
- CONFIRMED (code). Network: periodic polls to Gmail history, Google Calendar, WhatsApp local bridge (127.0.0.1, no internet), GitHub API (ETag 304s are free per AGENTS.md), Vercel/Render/Netlify/Supabase (token-gated, default on but inert without tokens), review model call (OFF by default). The cloud rewrite of mail lines is OFF by default.

### Inferences
- Idle wakeups from memcore alone: roughly 0.03 + 0.017 + 0.033 + 0.033 + 0.033 + 0.017 + resume 0.033 Hz, about 0.2 wakes per second, plus the 3 s keep-awake poll (0.33 Hz). Individually trivial, but they add up to a roughly constant 0.3-0.5 wakes/s that prevents deep idle residency, more relevant on battery than any CPU load figure here. ESTIMATED sum; I did not measure with powercfg or a profiler.
- Cheap win: a single shared in-process settings cache (invalidated on save) would remove ~8 file reads/parses per minute of idle plus one per store access.
- Cheap win: wrap the cloud-turn writes (`mark_used` x2, `log_egress`, episode) in `Store::batch` to cut 3-5 reseals to 1, and/or cap or shrink egress rows (store hash plus first N chars).
- Cheap win: merge the 30 s mail/leases/scheduler/WhatsApp ticks into one 30-60 s tick, or make them event-driven (they mostly check flags and clocks).

### Gaps
- No powercfg /energy, ETW, or battery-drain measurement of memcore pollers exists in the repo.
- Real flush frequency per day is unknown (would need counting `flush_now` calls).
- Whether `focus_mode::paused()` is honoured by all pollers; only resume and session visibly check it in what I read.

## 5. Privacy and safety

### Takeaway
The privacy design is thoughtful (sealed at rest, redaction, egress log the user can see, off-by-default risky features), with honest scope limits. Weak points: profile facts go to the cloud on every turn by default, egress log stores redacted but full text of everything sent, key persistence lives in Windows Credential Manager, and several pieces were never tested against real services.

### Cited Findings
- CONFIRMED (code and docs). At rest: AES-256-GCM via `ring` of the whole DB; data key in Windows Credential Manager entry `memcore_data_key_v1`, service `com.nexus.assistant` (`memcore/crypto.rs`). No key available means plaintext SQLite file fallback (the UI says so). Wrong key or tamper is refused without overwriting. Wipe rotates the key. Voice profiles use an independent key so a memory wipe cannot strand them (AGENTS.md).
- CONFIRMED (docs/109). Honest scope: stops disk theft and casual reads by other users, NOT same-user malware (it can call the same API for the key).
- CONFIRMED (docs). A two-process keychain probe passed ("entry appeared in cmdkey /list; a second process read it back"), closing the earlier keychain-persistence gap (docs/changes/109). I also see that memcore.enc exists and was last written today with a session history going back to October, which is consistent with persistence working, but I cannot prove it decrypts without reading it.
- CONFIRMED (docs/code). What leaves the device on each cloud turn: the transcript plus a memory pack (profile block <=1,400 chars, up to 8 facts, up to 3 episodes, each episode clipped to 120 chars) after `pii_filter::sanitize`; plus conversation prompt (recent turns / brief). Sent to Groq, Cerebras or Gemini via the router, or the Cloudflare Worker (`orchestrator.rs` ~3170-3200; docs/features/90). Profile is ON by default and excludes health, money, ID, credential and belief topics, and mined (not told) facts (doc 07, M1).
- CONFIRMED (code). The egress log records exactly what was sent (`log_egress(route, request, memory)`), stored in the sealed DB for 7 days, visible on the Memory page; review digests are logged under route `review`.
- CONFIRMED (code and docs). Name redaction (`memcoreRedactNames`, `memcore/names.rs`, 7 tests) swaps known people for Person A/B and back; OFF by default; the per-turn log stores the redacted transcript, not the original; strangers' names typed into a sentence are not covered (doc 109).
- CONFIRMED (docs). Review (cloud model reads PII-filtered digest of up to 14 cloud turns) is OFF by default, approval-gated, each suggestion must be grounded in the user's own words, and passes code checks for secrets, instructions, links, PII and sensitive keys. Residual risk named by the doc: lexical grounding could pass "Asha" from "my sister is not Asha", so approval is mandatory.
- CONFIRMED (docs). WhatsApp: read-only allowlist; `mark_read` and presence tools refused; OFF by default; speaking message text OFF by default. Whether the bridge's reads send receipts is UNVERIFIED (docs/changes/114, testing/whatsapp-read-receipt-test.md).
- CONFIRMED (AGENTS.md). Keys remain duplicated in the keychain and plaintext `settings.json` (open item). Not memcore data, but same threat model.
- CONFIRMED (docs). "Not verified" lists in changes 107-114: first-launch migration on real data; Memory page never viewed in a real window (at the time of writing); name redaction against a real Worker; real Google/Gmail/Calendar data; spoken reminder round trip; real reboot briefing; WhatsApp bridge; review prompt on a real model. AGENTS.md 2026-10-27 still lists many live proofs as owner-pending, and says nothing was run live in the most recent source pass.

### Inferences
- The egress log is a privacy asset (transparency) and a privacy liability (a second copy of redacted conversation text in the store for 7 days). Same-user malware that unlocks the store sees it all.
- Default-on profile injection sends the user's pinned facts to third-party LLMs on every turn. It is documented, filterable and visible, but a user who has not read the docs may not know.

### Gaps
- No red-team test of the PII filter against the real store contents.
- No evidence the "forget X" voice command removes the fact from the sealed store, the legacy core.json, AND episodes mentioning X; I only saw the docs claim coverage for specific files.
- Retention of old cloud-bound text in `conversation.jsonl` beyond what the 48 h replay age implies was not checked.

## 6. Session restore and boot briefing quality

### Takeaway
Both are deterministic and heavily unit-tested (session.rs 24 tests, briefing.rs 23, restore.rs 12, resume.rs 10) with no model in the loop, but they have been validated mostly against synthetic data and a terminal window, not live.

### Cited Findings
- CONFIRMED (docs/110). Resume points are sampled foreground windows (7-day ring, not sent to the cloud, sensitive windows and lock screen never recorded, URLs reduced to host+path). Briefing is a pure builder that is silent when it has nothing true to say and uses no invented progress numbers.
- CONFIRMED (docs/110). "Not verified": delivery not heard end to end (reboot, 45 s delay, presence gate); title quality across Electron apps and browsers without readable URL only checked on a terminal window; priorities come only from watched emails and unfinished requests.
- CONFIRMED (AGENTS.md 2026-10-10). Session restore: snapshots every 60 s, session-end hook, greeting names the whole last session; "Not run live: orb behaviour, reboot offer round trip, shutdown hook, Brave/Edge restore flag, WhatsApp Ctrl+F/UIA, cleanup on real data".
- CONFIRMED (AGENTS.md 2026-10-20). The briefing and boot hello were reworked (hello every logon, details once a day, stage-ready gating); "Open: unified PendingQuestion ... Nothing audible/visual was run live."

### Inferences
- Because `touch` skips flush, the persisted `last_seen` can lag by up to the next real mutation; harmless for briefing accuracy except "how long ago" statements right after a crash.

### Gaps
- No live evidence any of these features have run in the owner's daily use except what appears incidentally from today's file mtimes (memcore.enc written today).

## 7. Concrete improvements (no code changed)

### Takeaway
Ranked by value for the cost: measure first, then cut write amplification and idle wakeups, then fix recall evidence and the doc/code mismatch.

### Cited Findings
(These are recommendations, grounded in the findings above; no external sources.)

### Inferences
1. Build a blind recall set: have another person write 40-60 questions against 40 real-shaped keys (messy ones like `hackathon_team`, `wa_phone_mummy`), and make the recall test assert the held-out and blind scores (for example fail if held-out < 14/20). Today a collapse would not break CI.
2. Measure on the real machine, read-only: count `flush_now` calls per hour, time one flush, and sample process RSS with memcore on/off. This turns every ESTIMATED figure in sections 3-4 into CONFIRMED.
3. Batch the cloud-turn writes. `mark_used` (twice), `log_egress`, and the episode write each reseal the 0.84 MB blob; wrapping them in `Store::batch` gives one reseal per turn. Alternatively do an append-only journal and snapshot rarely.
4. Bound the egress log by rows as well as age, and store hashes plus a truncated copy for older rows (for example full text for 24 h, 200-char preview for the rest). Likely the largest single contributor to blob size.
5. Cache settings.json reads in memory (invalidate on `save_settings`) so `flag()`/`enabled()` stop reading and parsing the file on every tick and store access.
6. Collapse the 30 s pollers (mail tick, WhatsApp tick, leases, scheduler, resume) into one scheduler tick, and make them skip when the user is idle or on battery saver. Consider raising the 3 s keep-awake poll to an event/notify design.
7. Resolve the documentation/code mismatch: either restore or delete references to `habits.rs`, `memory_ipc.rs`, top-level `profile.rs` and the "about-me profile / habits" features in AGENTS.md, docs/features/90 and `memory.rs` comments; the Memory page text promises habit learning.
8. Make the profile fill order visible and stable: show on the Memory page which facts are inside the 1,400-char profile and warn when a new pinned fact did not fit, since a fact beyond the cap is silently excluded.
9. Run the review feature once against a real free model in a controlled session, log the raw suggestions, and tune the prompt before anyone turns it on; same for the curator on a populated copy of the store.
10. For recall beyond synonyms, add per-fact aliases written at save time (a cheap model call or a rule table) before considering embeddings, as doc 07 itself proposed.

### Gaps
- The cost of these changes was not prototyped. Item 3 and 5 look low risk from reading the code; item 6 touches several modules with separate owners.

## Summary scorecard

| Area | Verdict | Confidence |
|---|---|---|
| Recall of told facts, synthetic | Good: 21/21 direct, 17/20 held-out | CONFIRMED (author-biased) |
| Recall on real data / blind questions | Unknown | UNVERIFIED |
| Learning (corrections, routines, review, curator) | Safe by design, never exercised on real data | CONFIRMED tests; UNVERIFIED live |
| Forgetting / retention | Caps, decay, wipe with key rotation exist; egress has no row cap | CONFIRMED |
| Privacy | Strong design, on-by-default profile to cloud, key in Credential Manager | CONFIRMED |
| RAM | ~2-5 MB steady plus ~2 MB transient per write, store 0.84 MB on disk | ESTIMATED |
| CPU / idle | Many 30-60 s timers, repeated settings.json reads, ~3-5 reseals per cloud turn | CONFIRMED code; cost UNVERIFIED |
| Doc vs tree | habits.rs, memory_ipc.rs, top-level profile.rs missing from tree | CONFIRMED mismatch; cause UNVERIFIED |

Files cited: D:\ULTRON\src-tauri\src\memcore\{mod.rs, store.rs, crypto.rs, profile.rs, resume.rs, session.rs, briefing.rs, mailwatch.rs, wa.rs, leases.rs, scheduler.rs, leavewatch.rs, curator.rs, review.rs}, D:\ULTRON\src-tauri\src\{memory.rs, conversation.rs, keep_awake.rs, orchestrator.rs}, D:\ULTRON\docs\memory-core\{06,07}*.md, D:\ULTRON\docs\changes\{107-114}*.md, D:\ULTRON\docs\features\90-memory-profile-habits-and-memory-panel.md, D:\ULTRON\AGENTS.md, and file sizes/mtimes only in %APPDATA%\com.nexus.assistant\memory\.
