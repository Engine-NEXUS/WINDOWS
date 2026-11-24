# 198 — Each email is told once; mail you sent yourself is never an alert

Date: 2026-11-15. Source edits and offline tests only; needs a rebuild to take effect.

## What the owner saw

- Every morning the boot briefing opened with "5 things are open. First: A message released your hall ticket." The owner
  calls this email fake.
- The same emails came back in the briefing again and again.

## Causes (from the 2026-11-15 00:08 log and the code)

1. **The briefing re-read the same mail every day.** `briefing::gather` listed every important email the inbox watcher
   filed in the last 3 days, plus every watched thread, each time it ran. Nothing remembered that an email had already
   been mentioned, so one email could open three briefings in a row and a watched thread could open every briefing.
2. **The "hall ticket" item was mail from one of the owner's own addresses.** The sender is spoken as "A message" only
   when it matches the owner's own name or address (`mailwording::category_plain`, `explain::who_label`). Gmail's SENT
   label hides mail you send yourself within one account. It does not hide mail sent from one connected account to
   another, so the triage rules filed it as a High exam notice ("hall ticket" is a strong exam word).

The live alert path was already single-shot: a new email is announced once when it is first filed (`store_mail`
returns true only for a new record). That is unchanged.

## What changed

- `memcore/selfnames.rs`: `is_own_address` matches one of the owner's connected addresses exactly. It never matches on
  name tokens, so "lakshya@other.com" is not the owner.
- `memcore/mailtriage.rs`: `is_self_sent(from, account, names)` is true for mail from the receiving account itself or
  from another of the owner's addresses.
- `memcore/mailwatch.rs`:
  - Self-sent mail is skipped before triage, so it is never filed, spoken or toasted.
  - On the watcher's first tick each run, `purge_self_sent` removes self-sent records filed before this rule existed.
    This removes the existing "hall ticket" record.
  - A batch alert notes which emails it covers (`briefing::note_batch`).
- `memcore/briefing.rs`:
  - `MailItem` carries a `key`: `mail:<gmail id>` for filed mail, and `watch:<id>:<message count>:<deadline>` for a
    watched thread (a new reply or a moved deadline makes it new again).
  - A sealed-store record `briefing_told_mail` keeps the mail already told about (kept 30 days, at most 400 entries).
  - The boot briefing drops told mail before building. With nothing new, it says nothing about mail.
  - Mail in a briefing is marked told when the briefing is submitted.
  - `important_mail` (used by the briefing and "any important emails?") never lists self-sent mail.
- `proactive_policy.rs`: when a mail alert is actually spoken, its email is marked told (`on_alert_spoken`), so the next
  morning does not repeat it. An alert that was only shown as a card (nobody at the PC) is not marked, so the briefing
  still mentions it once.

Asking "any important emails?" or "show my briefing" still lists recent important mail, because that is an explicit
request.

## Tests

- `selfnames`: exact own-address match and a name-token lookalike that must not match.
- `mailtriage::mail_from_an_own_address_is_self_sent`.
- `mailwatch::mail_from_your_own_addresses_is_never_filed_and_old_copies_are_purged`.
- `briefing`: items carry their record key; self-sent rows are hidden; told mail is not repeated while new mail is;
  told entries expire and stay bounded; a watched thread is new again when it changes; a spoken alert maps to its
  email(s), and a batch is mapped once.

## Not verified

- No live run: needs a rebuild, then a morning boot with no new mail (expect no mail line), one with a new important
  email (expect it once), and a mail sent from one of your own accounts (expect silence).
- Unfinished requests in the briefing's open items still repeat day to day. This change covers email only.
