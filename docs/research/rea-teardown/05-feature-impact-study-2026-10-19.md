# REA Feature-Impact Study — Strategies & Estimated % Boosts for Existing Features

**Date:** 2026-10-19. Status: RESEARCH ESTIMATE (numbers are engineering judgments, not measured — verification path in §5). Companion docs: [01 tool](01-rea-tool-verification-install-research-2026-10-19.md) · [02 law](02-browser-observation-law-compliance-research-2026-10-19.md) · [03 bay](03-teardown-bay-diag-design-research-2026-10-19.md) · [04 plan](04-build-plan-research-2026-10-19.md).

**Question the owner asked:** REA-as-its-own-website-analysis-command-center is the obvious win. But can REA **make existing features work better and more accurately** — and by roughly how much, per feature, so we learn something?

**Ground rule for every row:** THE LAW caps REA's reach — public sites, open-source code, owner's own projects, disposable Chrome profile, observe-only. Whatever a REA strategy would touch private/logged-in surfaces is marked N/A *by law*, not by skill.

---

## 1. The impact table

| Existing feature (anchor docs) | REA strategy | Est. boost | Basis & confidence |
|---|---|---|---|
| **A. Screen analysis / narrated screen tour** (change 85, 91, 93 · feature 98 · screen-tour research) | When the focused target is a browser window on a law-allowed public page: `rea inspect-web-page --include-accessibility-text` returns the page's real text (headings, captions, alt text, titles) **deterministically** instead of pixel OCR; `compare-web-captures` backs multi-state tours | **~30 % feature-level** (latency ~30 s → 2–4 s ≈ 85–90 % cut; OCR misreads of browser text eliminated on that lane) | Strong grounding: 98-spec's 3-engine chain with 12 s VLM timeout + screen-tour/04's "~30 s" finding; browser-screen is a large subset of tour traffic. Highest-confidence row in the study |
| **B. Text-evidence lane: celebrity/entity recognition** (feature 98 · screen-tour/02 §rule 6: "answerable only from text evidence") | REA accessibility text + page structure = a cleaner text-evidence corpus than OCR pixels; OCR stays the fallback for non-browser screens | **~25 % accuracy** on browser sessions | Same deterministic-source argument as A; confidence high while page is public, zero elsewhere |
| **C. Ghost Mode desktop control** (feature 129/133 · changes 132/133: ghost command center, "no longer asks a vision model on a UIA miss") | For web-app targets (pages inside Chrome; Electron display later — separate provider, v1 out of scope): CDP accessibility tree + route/endpoint candidates feed `ghost_learning`'s target dictionary; named NLU clicks resolve semantically without any vision engine | **~10–15 %** (browser-target subset; coordinates still come from NEXUS's own click pipeline — REA never supplies, per law) | Mechanism grounded (ghost 12/13 target dictionaries are the built design); subset share of tasks is an assumption to measure in T4 |
| **D. PR / repo analysis & own-project dev loop** (phase-11 PR verbs · feature 139 reconciliation) | `rea import-reference-source` (module graphs, dependency edges) for owner audits; `compare-web-captures` for pre/post-verification of the owner's own deployments; `inspect-go/rust-binary` for build provenance facts | **~5–10 % accuracy, larger latency win** in autonomous check cycles | Honest: models still read diffs; REA adds *verified facts* and deterministic regression-catch on own sites, not new analysis power |
| **E. Proactive watch** (feature 76 vision-mode watch · Gmail-engine watch state machine) | Reuse the existing watch/diff machinery on **public or own-site** targets: `observe_web_session` + screenshot/bundle compares = price/changelog/dispatch diffs without any VLM | **0 % on the email lane (by law: inbox stays email-only) — enables +1 watch surface** | Enabler, not an accuracy boost; the watch state machine is the existing shared asset |
| **F. YouTube / captions Q&A lane** (transcripts bay; screen-tour text rule) | Public YouTube pages: REA text evidence (titles, channel, on-page strings) beats OCR where OCR was used; the transcripts bay is already text-first, so overlap is small | **~5 %** | Edge-case value only — captions route already covers most asks |
| **G. NLU intent families** (feature 69 alias map · phase verb families) | Powers 3–5 new teardown intents ("teardown this page", "what's this site built with") | **0 % on existing intent accuracy — new verbs only** | Capability addition; core NLU unchanged |
| **H. Wake word / STT / TTS** (feature 71 · change 50 STT hallucination fix · TTS streaming research) | — | **0 % — honest zero.** No audio path exists between REA and speech engines | Include the zero so nobody force-fits synergy |
| **I. OAuth / apikeys / sidecar Worker calls** (server/sidecar/oauth.py) | — | **0 % by law** — private identity surfaces are out of REA's allowlist forever | |
| **J. Administrator share route (admin suite R6)** | REA-derived public-doc facts (titles, headings, source links) can enrich repo briefs with verified citations | **~5 %** (brief quality, citation verifiability) | Small but real; inherits privacy.hide invariants (admin suite doc 07 §7.2) |

## 2. What we learn (the owner's actual ask)

1. **REA is a deterministic sensor for browser-fronted surfaces, not a general reasoning upgrade.** Its single biggest gift is *replacing* the least-reliable lane of existing research — pixel OCR and VLM on browser text — echoing the Gmail engine's Tier-1 lesson (URL-hash grounding at "100.0 % precision", vision-grounded research:29) at website scale.
2. **The law is the ROI limiter — and that's a feature.** Boosts concentrate exactly where law-compliance overlaps existing pain (public browser screens, own projects). Logged-in/private surfaces (inbox, timetable, WhatsApp) are *properly* untouched — same boundaries those features already learned for privacy.html.
3. **It rides existing rails:** provenance gate (`origin: Derived`, Read/Reversible only — doc-140 doctrine), watch state machine, ghost target dictionaries, Hub activity cards. No new doctrine was invented by any row of the table — which is why alignment research (§ vision/screen study) scored ~90 % structural.
4. **The zeros are findings.** Audio stack, NLU core, identity surfaces: force-fitting REA would be negative value; the kit is scoped, so it scopes cleanly.

## 3. Sequencing (where to build first, from this study)

1. **A/B first** (screen analysis lane): smallest code, biggest measured win — a teardown route that answers "analyse this page/screen" when law-check passes feeds existing tour/caption stacks directly.
2. **C second** (ghost browser grounding): depends on target-dictionary schema shape; schedule alongside ghost plan 129's remaining work to avoid file-ownership clashes (feature 133 coordination rules).
3. **D/E third** (dev-loop + watch): needs the full session service (T3) since these call multi-step REA flows.

## 4. Honest limits

- Percentages are judgment-based (feature-level deltas); the T4 corpus in [04 build plan §4](04-build-plan-research-2026-10-19.md) is what converts A/B into measured numbers.
- Electron `file://` targets (REA's separate provider) excluded from v1 per law scoping in doc 02 §6 — that's where C's ceiling would rise later.
- Row E depends on owner's watch-frequency consent + the observation-window cap (open question doc 03 §10.4).

## 5. Sources (checked 2026-10-19)

- `docs/changes/132-ghost-command-center-first-implementation.md` + `docs/changes/133-ghost-command-center-implementation-deep-dive-2026-10-16.md` (ghost no-vision route facts) and `docs/research/ghost-mode/12-no-vision-keyboard-and-cursor-learning-plan-2026-10-16.md`, `13-no-vision-control-evidence-sources-2026-10-16.md` (target dictionaries)
- `docs/features/98-screen-analysis-celebrity-recognition-and-zero-delay-spec.md` (3-engine chain timings), `docs/research/screen-tour/02-v2-root-cause-analysis-and-plan-2026-10-06.md` + `04-celebrity-recognition-and-zero-delay-architecture-2026-10-07.md` (~30 s root cause, text-evidence rules)
- `docs/research/vision-grounded-gmail-engine-and-proactive-watch-architecture-2026-09-29.md` (Tier-1 grounding lesson, watch machine)
- `docs/features/76-vision-mode-screen-email-scanning-and-proactive-watch-memory.md`, `docs/features/139-production-return-review-and-agent-task-reconciliation-plan.md` (watch surface + dev-loop context)
- `docs/research/live-mode-feasibility-2026-09-11.md:359-364` (why the vision-on-control lane must stay dead — framing for "sensor not actuator")
- REA docs (browser-observation, cli, website-script-export references) as in docs 01-03
- Admin privilege suite docs 07/09 (R6 share route, privacy.hide invariants, audit)
