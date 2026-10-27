# 158 — Notification brain Phase D: short one-liners for non-GitHub mail (2026-10-25)

## Scope

Implements phase D of [docs/features/151-notification-brain-event-key-cooldown-and-short-notifications-plan.md](../features/151-notification-brain-event-key-cooldown-and-short-notifications-plan.md): deterministic, keyword-driven one-liners for the non-GitHub mail categories (Database, Hackathon, Exam, Internship, Deadline), so spoken notifications stay short (≤15 words) without spending AI credits. The cloud rewrite now only fires for mail even these patterns cannot phrase — by construction rarer than before.

## Implemented

- `src-tauri/src/memcore/mailtriage.rs`: the classification keyword lists (`DB_HIGH`, `DB_MEDIUM`, `HACK_STATUS`, `HACK_DEADLINE`, `HACK_RECEIVED`, `EXAM_STRONG`, `EXAM_WEAK`) are now `pub(crate)`. The wording layer reuses the exact lists that decided the category, so a one-liner can never name a condition the rules did not match.
- `src-tauri/src/memcore/mailwording.rs`:
  - `local_plain` now dispatches: GitHub → the existing `github_plain` (unchanged), everything else → new `category_plain`.
  - `category_plain` maps the matched keywords to one short sentence per situation — deletion/pause/suspension/payment/read-only/limits for databases, selected/not-selected/waitlist/registration/deadline for hackathons, hall-ticket/schedule/semester/backlog/results/revaluation/forms for exams, a fixed line for internships, and a subject-based deadline line whose subject budget shrinks when the sender name is long, keeping the whole line ≤15 words.
  - Anything unmatched returns `None`; the AI-rewrite and template fallbacks keep the old behaviour untouched.

## Phase boundary

GitHub wording, batch summaries and the AI fallback are unchanged. Phase C (cooldown tuning), E (WhatsApp/MCP funnel) and F (replay harness) remain per plan.

## Verification

- `cargo test --lib -- --test-threads=1 mailwording mailtriage` — 42 passed, 0 failed. New tests: database deleted/billing/limit lines, keyword-free database mail stays `None`, hackathon selected/rejected/registered/deadline lines, exam hall-ticket and weak-update lines, internship and deadline lines, and a ≤15-word cap across one line per situation (long sender name included).
- The previous `local_plain_non_github_returns_none` expectation was updated deliberately: Database mail that the rules classified as High now gets a local line instead of falling through.
- Live speech was not exercised in this session.

## Credit audit

Zero AI credits added: the one-liners are pure string formatting over already-classified mail. The cloud rewrite path is unchanged and stays behind its default-off guards.
