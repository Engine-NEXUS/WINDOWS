# Notifications that inform instead of repeating (2026-11-12)

## 1. The report

NEXUS spoke an email as a carbon copy of it. Real line from the boot briefing (`nexus.old.log`, 2026-11-10 18:57):

> First: a GitHub notice from GitHub: “[GitHub] A fine-grained personal access token has been added…”.

The quote marks, the bracketed prefix and the trailing ellipsis were all read out. The owner wants a brief, neat explanation that informs (what it is, whether it needs me, by when), for email, GitHub, WhatsApp and the rest.

## 2. How a mail became speech before

`memcore/mailwatch.rs` (poll, triage, store, alert) -> `mailwording::plain_alert_text` (Local rules -> optional AI -> template) -> `proactive_policy::submit_event`.

- Only the sender, subject and Gmail's ~100-character preview were read. `google_io` requested `format=metadata`, and a test asserted that bodies were never requested.
- Local rules (`github_plain`, `category_plain`) gave a short line for known GitHub patterns and for database, hackathon, exam and internship keywords. Everything else, including the token notice, 2FA, SSH key and OAuth mails, returned `None` and fell to the template.
- The template (`mailtriage::alert_text_with`), the batch summary (`batch_text_with`) and the briefing (`briefing::mail_line`) spoke the subject verbatim: `{phrase} from {sender}: {subject}`, `“{subject}”`. The Deadline rule spoke the first ten words of the subject.
- An AI rewrite existed behind `memcoreMailPlain` (off by default).
- WhatsApp alerts already carry no message text (name only); calendar leave-by and agenda lines embed the event title; CI and deploy lines carry only the repo name and a kind.

## 3. Decisions (owner, 2026-11-12)

- Read the **email body** for important mail (`gmail.readonly` was already consented, so no new sign-in).
- **Local rules first, the AI only as a helper** for what the rules do not recognise.

## 4. What was built

- `memcore/explain.rs` (pure, offline): `explain_mail` builds one or two short sentences: a security notice (token added or expiring, two-factor changed, SSH key added, new sign-in, password changed, app given access, security alert: what happened and what to do), else the existing category line, else a topic (billing, an order, your application, your studies, an event, a booking, your account, a newsletter), else an honest generic line ("An email arrived that I could not sum up. It is in the sidebar."). A detail sentence names what the mail asks and by when only when the mail says so ("It needs you to submit something by friday at 5 pm."). `find_when` needs a due-style word in front, so "the meeting is on Friday" is not a deadline, and prefers a date that also gives a time.
- **The echo gate** `echoes_source`: a line that repeats five or more consecutive words of the subject (or, for a subject shorter than five words, the whole subject) is rejected, whoever wrote it. Four words was tried and rejected: "added to your account" is ordinary speech. The fixed security sentences are exempt (our own wording).
- `clean_for_speech` removes bracketed prefixes, quotes, ellipses, URLs and addresses. `short_title` shortens event titles at the first separator and to seven words (leave-by reminders, the calendar read-out).
- **Body reading** (`google_io::gmail_body`, `mailtriage::...`, `mailwatch::attach_bodies`): only for new mail already triaged Medium or higher, at most 5 per poll, never in the quiet first catch-up. The extract (at most 1,200 characters; plain part, else HTML stripped; quoted replies and unsubscribe footers dropped) lives in `MailMeta::body`: never stored (`to_stored_with` never reads it), never printed (`MailMeta`'s `Debug` hides it), never spoken.
- **AI helper** (unchanged switch `memcoreMailPlain`, default off; also needs not local-only STT, name hiding off, a provider key): only when the explainer returned the generic line; at most 40 cloud explanations a day (`ai_budget_take`); the redacted prompt (now with a redacted body extract) is logged under Sent to cloud; the answer must pass `accept_line` (3 to 30 words, no URL or address, no quotes, ellipsis or brackets, numbers grounded in the subject, preview or body, no instruction phrases, and not a copy of the subject).
- The template, batch and briefing now speak explanations. `StoredMail` gained `explain` (at most 160 characters, built when the mail is filed with the owner's own name hidden); the briefing speaks it and, for older records, says who and what kind, never the subject.

Examples: the token mail -> "GitHub says a new access token was added to your account. If that was not you, revoke it." An assignment mail with a body "please submit your report, it is due Friday at 5 pm" -> "Prof Rao mentions a deadline about your studies. It needs you to submit something by friday at 5 pm."

## 5. Verification

- 3,201 Rust library tests pass; 18+ new tests in `explain.rs` (explanations, security notices, dates, the echo gate, body extraction for plain, HTML-only and capped mail, prompt-injection text in a body not followed or spoken), new tests in `mailwording.rs` (AI gate, grounded numbers, redacted prompt with body, daily budget, five real-shaped mails never echo their subject) and `mailtriage.rs` (the body is not in the stored JSON nor in `Debug` output; the explanation uses what the body said).
- Mock-server test: bodies are requested only for the two new important mails of the poll, once each, never in the baseline, never for dropped mail.
- Mutation checks: an always-false echo gate fails 2 tests; ignoring the body cap fails 1; removing the redaction from the prompt fails 3.
- Pinned wording tests were changed on purpose (`alert_wording`, the self-sender test, the Deadline rule, the briefing tests that quoted a subject); each now asserts the new wording and that the subject is not spoken.

## 6. Not verified

- **Real mail on the owner's laptop**: no mail has gone through the new path yet (two Google accounts are connected and mail watching is on). A guided test is needed.
- The AI helper against a real model (it is off by default and has never been exercised here).
- Non-English mail (rules are English; the generic line is the fallback).
- WhatsApp "read that message" still speaks a gist of the last three messages when `memcoreWhatsappSpeak` is on; GitHub CI lines are unchanged (already brief); both were left as they are.
- The plan's separate `memcoreSummariseAi` switch was not added: the existing mail switch is reused to keep the change small.

## 7. Next

A real-mail sitting: send test mails (invoice with a pay-by date, an assignment with a due time, an exam notice, a mail nothing recognises, a real GitHub token add/delete), read each spoken line, the Sent-to-cloud log with the switch off and on, and check that no mail body appears in `nexus.log` or the stored record. Pass rule agreed before: 10 real mails, at least 9 lines judged "informs, does not repeat", none with a quoted subject, URL or invented fact, no body text in logs.
