# Notification Brain — Research, Changes and Testing (feature 151)

Created 2026-10-25. Companion documents: the approved plan [docs/features/151-notification-brain-event-key-cooldown-and-short-notifications-plan.md](../features/151-notification-brain-event-key-cooldown-and-short-notifications-plan.md) and the build records [docs/changes/157](../changes/157-notification-brain-phase-a-b.md) (Phase A+B), [158](../changes/158-notification-brain-phase-d-one-liners.md) (Phase D) and [159](../changes/159-notification-brain-phase-e-funnel.md) (Phase E).

Status: **phases A, B, D and E are built and unit-tested; a review pass on 2026-10-25 (change record 160) fixed six defects found in them. Nothing from this feature has a live-proof-ledger line yet** — §7 is the protocol for earning one. Phases C (cooldown tuning) and F (replay harness) remain open.

## 1. Problem statement

The owner's complaint, in two parts:

1. **Repeats.** The same logical event (one failing GitHub workflow, one ongoing conversation) was announced again and again. Worst case observed in the code's own data model: every CI-failure email for `nexus-engine/window` arrives as a **new Gmail message id AND a new thread**, so every thread-based dedup sees a "new" mail and speaks.
2. **Verbosity.** The spoken fallback read subjects up to 90 characters (`subject_label` clip), and non-GitHub categories had no local wording at all — they either stayed silent (falling to a verbose template) or burned cloud credits on the optional rewrite.

The requirement that shaped everything: for this owner (the NEXUS admin) the whole feature must run **100% in deterministic Rust — zero AI credits, zero dependency on the admin-brain model (Qwen @ 127.0.0.1:39219)**.

## 2. Code-verified root causes

Each was confirmed by reading the pipeline, not assumed (references are to the state before the fix):

| # | Root cause | Where |
|---|---|---|
| 1 | GitHub failure mail = new message id + new thread id per run; dedup keyed on either cannot recognise the repeat | `google/sentinel.rs` seen-ring, `memcore/mailwatch.rs` alert ids |
| 2 | Non-GitHub categories had no local one-liner; the template path spoke a ≤90-char subject | `memcore/mailtriage.rs::alert_text` |
| 3 | Medium tier only enforced a 20 s spoken gap and ≤6/hour — a burst of 5 identical mails inside one minute passed | `proactive_policy.rs` tier config |
| 4 | WhatsApp alerts used the id `wa_{jid}`; the engine's id dedup (`seen: HashSet`) drops duplicate ids **forever**, so a priority person's chat went silent after its first alert until app restart — the opposite failure of #3 | `memcore/wa.rs::fire`, `proactive_policy.rs::Engine::submit` |
| 5 | Two independent mail pipelines (the `sentinel` watched-thread poller and the `mailwatch` inbox watcher) could submit overlapping alerts | `google/sentinel.rs` vs `memcore/mailwatch.rs` |

Root cause #4 was *discovered during Phase E*, not in the original plan — wiring event keys forced the question of what the repeated `wa_{jid}` id was doing, and the answer was "silently swallowing every alert after the first".

## 3. Design (what the brain does)

### 3.1 Event keys — `src-tauri/src/notify.rs` (Phase A)

A pure function `event_key(source, thread_id, sender, subject)` derives one stable key per logical event. No clock, no network, no model:

| Mail shape | Key | Why |
|---|---|---|
| GitHub-shaped (`owner/repo` in subject) | `mail:gh:<owner/repo>:<stem>` | The **repository + workflow** is the repeated entity; thread ids lie (each run is a new thread). |
| Any other mail with a thread id | `mail:thread:<id>` | A real conversation thread is the event. |
| No thread id | `mail:<localpart>:<stem>` | Sender + normalised subject is the best available identity. |

The `stem` normalises the subject: lower-case, strips `Re:`/`Fw:`/`Fwd:` **wherever they sit** (a leading-prefix strip alone misses `[tag] Re: …` — this exact bug was caught by a unit test during Phase A), removes bracketed tags and `(2)`-style counters, drops `#N` run references, removes the mail tails `the github team` / `view it on github`, maps punctuation to spaces and caps at 60 characters.

### 3.2 Cooldown gate — `proactive_policy.rs` (Phase B)

- `Alert` gained `event_key: Option<String>`; the engine keeps a bounded ring (`KEY_RING = 200`) of recent keys.
- **Medium tier only**: `MEDIUM_KEY_COOLDOWN_MS = 300_000` (5 min). The first event speaks; repeats inside the window become sidebar **cards** with reason `event_key_cooldown` — never queued for speech.
- The window **rolls forward** while repeats keep arriving; suppression ends only after a real 5-minute lull. The next accepted line carries the count-up: `Sir, N more since you last heard — …` (only for `Sir,`-addressed lines under 180 chars).
- **High has a short fixed guard** (review pass, record 160): a High repeat of one event within `HIGH_KEY_COOLDOWN_MS` = 2 min becomes a card; the window is fixed, not rolling, so a High nag can never go silent for good. An **escalation** (a higher urgency than the last accepted alert for that key, e.g. Medium CI failure then a High security mail) always breaks through. **Critical ignores keys.** Low never speaks anyway.
- **A key is released when its alert never spoke**: if the accepted alert expires in the queue as a card (meeting, snooze, busy), the key stops suppressing repeats, so an event is never muted for something the owner did not hear.
- **Gate before wording**: `proactive_policy::key_in_cooldown` is a read-only peek; `mailwatch` skips the plain-wording step (and so any cloud call) for a repeat that will only become a card.
- New wrapper `submit_event(app, id, text, urgency, event_key)`; the old `submit` delegates with `None`, so every other caller kept compiling unchanged.

### 3.3 Short one-liners — `memcore/mailwording.rs` (Phase D)

`local_plain` now dispatches: GitHub → existing pattern wording (unchanged), everything else → new `category_plain`, which maps the **same keyword lists `classify` used** (the lists were made `pub(crate)` so wording can never name a condition the rules did not match) to one ≤15-word sentence:

- Database: deletion / pause / suspension / payment failure / read-only / usage limit / inactivity → e.g. `Sir, Supabase may pause your project.`; generic High → `… sent a database warning.`; Medium → `… sent a usage or billing notice.`
- Hackathon: waitlist / not-selected / selected / registration confirmed / deadline → e.g. `Sir, Devpost says you were selected.`
- Exam: hall ticket / seating / schedule / semester / backlog / viva / results / revaluation / forms → e.g. `Sir, Exam Cell released your hall ticket.`
- Internship: fixed line. Deadline: subject-based, with a **word budget that shrinks when the sender name is long** so the whole line stays ≤15 words.
- Anything unmatched returns `None` and the AI/template fallbacks keep the old behaviour.

### 3.4 Funnel — WhatsApp and watch (Phase E)

- `wa.rs::fire`: id `wa_{jid}_{timestamp}` (unique) + event key `wa:{jid}`. Medium priority-people repeats coalesce with count-up; High still speaks every fire.
- `watch_runner.rs::tidy_up`: the Medium tidy notes carry `watchpower:tidy`.
- Deliberately **not** keyed: `agent_watch.rs` and `watchpower` Say/SayAndAsk/follow-ups (High = break-through by design, one-shot ids) and webhook `/trigger` (it is a spoken-command equivalent, not a notification — throttling it would lose commands).

## 4. Prior art consulted (plan §3)

| Project | Licence | What was borrowed |
|---|---|---|
| ntfy | Apache-2.0 | Topic + priority model: one stable topic (our event key) with a priority that decides delivery treatment. |
| Apprise | BSD-2-Clause | Escalation chains and tag logic — the idea that urgency tier, not content, decides repeat treatment. |
| Apple Intelligence notification summaries | design reference | Deterministic short rewrites of one notification; also the cautionary tale — Apple paused the news-summary feature in Jan 2025 after hallucinated headlines, which validates keeping the wording table-driven instead of model-generated. |

## 5. Credit audit

Zero AI credits in the default configuration: event keys and one-liners are string functions; the cooldown is arithmetic in the existing engine. The optional cloud rewrite of mail the local rules cannot phrase **was default ON when this feature was first recorded** (`memcoreMailPlain` defaulted to true, contradicting this section); the review pass changed the default to **OFF** everywhere (Rust settings, `memcore::status`, `mailwording`, Hub toggle) and moved the cooldown ahead of wording so a suppressed repeat never calls a model. Owners who already saved `memcoreMailPlain: true` keep it until they switch it off in the Hub. Still behind local-STT-only and name-redaction guards. No admin-brain dependency.

## 6. Unit-test evidence (all run with `--test-threads=1`)

| Phase | Command | Result |
|---|---|---|
| A+B | `cargo test --lib -- --test-threads=1 notify:: proactive_policy::` | **25 passed, 0 failed** (6 event-key/stem tests incl. mid-subject `Re:` handling and resend-collision; 19 engine tests incl. the full cooldown timeline `speaks → card → card → speaks with "Sir, 2 more since you last heard — …"`, High/Critical/Low bypass, distinct keys independent, count-up guards, permanent id-dedup) |
| D | `cargo test --lib -- --test-threads=1 mailwording mailtriage` | **42 passed, 0 failed** (deleted/billing/limit lines, keyword-free mail stays `None`, hackathon status lines, exam lines, internship, deadline with word-budget cap, and a ≤15-word cap asserted across one line per situation with a long sender name) |
| Review pass | `cargo test --lib -- --test-threads=1 notify:: proactive_policy:: mailwording mailtriage mailwatch wa:: watch_runner:: memcore_mail_plain` | **136 passed, 0 failed** (adds release-on-expiry timeline, High guard, escalation, pure-peek, agent-notice merge, batch-key, default-off test). Four mutations (Medium window 0, High guard off, release removed, batch key first-only) each fail a test. |
| E | `cargo test --lib -- --test-threads=1 wa:: proactive_policy:: watch_runner::` | **73 passed, 0 failed** (`same_event_key_within_five_minutes_speaks_once_and_counts_the_rest` covers the exact WhatsApp shape: unique ids, one key) |

`cargo check --lib` passed at every step. The full CI sweep `cargo test --lib --features custom-protocol,admin-brain -- --test-threads=1` was **not** re-run in these sessions (long build); the targeted runs share the same test binary compiled with default features.

## 7. Live verification protocol (what earns a ledger line)

Nothing below has been run live; until it is, do not describe the notification brain as field-proven.

**BOT — replay acceptance test (plan §4).** The single test that proves the headline promise:

1. Trigger two GitHub failure emails for the same workflow (`nexus-engine/window`) about one minute apart.
2. Expected: **exactly one** spoken line; the second becomes a sidebar card with reason `event_key_cooldown`.
3. Wait past a 5-minute lull, send one more. Expected: spoken, prefixed `Sir, N more since you last heard — …`.
4. Evidence: console `[MAILWATCH] plain=…` lines + the sidebar card reasons; save the console log under `docs/testing/runs/`.

**OWNER — real-inbox soak.** One day of normal use; note any repeat that still speaks, and any suppression that hid something wanted. This is also the raw data for Phase C.

**OWNER — WhatsApp (only after the read-receipt test passes, flag is off by default).** Have a priority person send 3 messages in 5 minutes: expected one spoken line, not three, not zero (the old behaviour for the second-and-later alerts was **zero, silently**, until restart).

**OWNER — High bypass check.** A second Dependabot/security alert minutes after the first must still speak immediately (proves the cooldown did not leak into High).

## 8. Phase C tuning procedure (when live data exists)

`MEDIUM_KEY_COOLDOWN_MS` lives in `proactive_policy.rs`. Tune from the soak data: if real repeat-bursts (newsletters, CI retry storms) still get through too early, raise it; if wanted updates feel suppressed, lower it. Do **not** extend the gate to High/Critical — break-through is a design property, not a bug. After any change, re-run the Phase A+B test set; the timeline test encodes the 5-minute value, so it must be updated in the same commit.

## 9. Open work

- **Phase C** — tuning, blocked on the §7 soak data (not on code). Two things to judge from that data: the Medium window **rolls**, so a retry storm repeating more often than every 5 minutes stays silent for its whole duration (cards remain); and the High guard length (2 min).
- **Sentinel overlap** — `google/sentinel.rs` (owner-requested watched threads, High) still speaks through `speak_proactive_alert` with no event key; it can in principle announce a thread that `mailwatch` also announced. Left as is on purpose (explicit watch requests should not be throttled); revisit if the soak shows doubles.
- **`return_review.rs`** is a tracked file that no `mod` declares, so it is not compiled; it calls `proactive_policy::merge_agent_alerts_into_return`, which does not exist (record 157 claimed it but it was never written; it was removed again after review because nothing compiled calls it). Write it together with the module wiring.
- Original plan phase F (group counts in the memory ring, boot briefing and Hub) is not built.
- **Update 2026-10-25 (records 165-166):** the failed / still failing / fixed state machine (`notify_brain.rs`) and the GitHub Actions watcher (`memcore/github_watch.rs`) are built and unit-tested; they are NOT live-proven. Live protocol: connect GitHub, push a commit that breaks a workflow, expect exactly one line ("Build failed in <repo>, sir."), push another failing commit (silence), push a fix ("<repo> is green again, sir."), restart NEXUS while a build is failing (no repeat).
- **Phase F** — replay harness: a test-only driver that feeds recorded mail/WhatsApp fixtures through `Engine::submit` and asserts the spoken-line transcript, so the acceptance test becomes automatic instead of manual.
