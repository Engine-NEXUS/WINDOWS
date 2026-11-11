# Change 192: notifications explain instead of repeating

Date: 2026-11-12. Branch `feat/notifications-explain-not-repeat`. Research: `docs/research/session-2026-11-10/05-notifications-explain-instead-of-repeat.md`.

- New `src-tauri/src/memcore/explain.rs`: one short explanation per mail (security notices, category, topic, what it asks and by when from the body), an echo gate against copying the subject, speech cleaning, event-title shortening, body text extraction.
- `mailwording.rs`: `explained_text`; `plain_alert_text` is rules -> explainer -> optional AI (opt-in, 40 a day, redacted, logged, code-checked) -> honest generic line; the Deadline rule no longer speaks the subject; `accept_line` rejects quotes, ellipsis, brackets and subject copies.
- `mailtriage.rs`: the template and batch lines explain; `MailMeta::body` (memory only, hidden from `Debug`); `StoredMail::explain`; `to_stored_with`.
- `google_io.rs` + `mailwatch.rs`: `gmail_body` and `attach_bodies` read the body of new important mail only (at most 5 per poll, never in the baseline); the poll test now asserts exactly that.
- `briefing.rs`: speaks the stored explanation, never a quoted subject.
- `leavewatch.rs`, `agenda.rs`: event titles shortened for speech.

## Verify

3,201 Rust library tests, dead-code ratchet (139 known, none new), IPC and hygiene gates; mock-server poll test; three mutation checks (echo gate, body cap, prompt redaction) each fail named tests.

## Not verified

Real mail through the new path on the owner's laptop; the AI helper against a real model; non-English mail.
