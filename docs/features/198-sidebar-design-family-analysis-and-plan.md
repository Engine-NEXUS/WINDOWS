# 198 — A design family for the sidebar: every view gets the design its own task needs

Date: 2026-11-22. Status: **plan for the owner's decision. Nothing in this file is built.**
Replaces the direction of [197](197-sidebar-system-redesign-plan.md) (one look for every view), which the owner rejected:

> "I have not told you to copy it. As Google has its own design and GitHub has its own design, I want variety of design, each for its own task, separately, without any issue. Analyse, compare and plan it out perfectly."

The prototypes already built (`frontend/dev-sidebar-system.html`, `frontend/dev-sidebar-v2.html`) stay as material to borrow from; v1 is the uniform
monochrome system and v2 is the GitHub-dashboard-inspired one. This plan says what replaces them and how to do it without breaking anything.

---

## 1. What was asked, in plain terms

- Not one look for all nine sidebar views, and not a copy of the GitHub dashboard.
- A different design for each kind of task: a screen reading, a code review, a research answer, a security approval and a repository
  overview are different jobs, so they should look and behave differently.
- The views must not interfere with each other ("without any issue"): no style leaks, no theme breaks, no window problems.

## 2. How big products do it (analysis)

This section is general design-system practice (Google's Material, GitHub's Primer), not a study of either company's internals.

| | What is shared | What differs per product |
|---|---|---|
| **Google** | Type scale, spacing grid, elevation, accessibility rules, motion principles, the icon style | Gmail, Maps, Photos, Docs, Calendar each have their own layout model (list, map canvas, grid, page, timeline), their own signature component (compose window, route card, photo grid), their own colour emphasis and density |
| **GitHub** | Primer tokens, the avatar and label shapes, focus rings, keyboard conventions | Pull request review (diff, comments), Actions (a run graph and logs), Insights (charts) and Issues (a board) look and behave differently because the work differs |

The lesson both teach: **share a thin contract, not a look.** What is shared is the part that must never differ (accessibility, themes, keyboard, window
behaviour, spacing and type scales). What is not shared is the layout and the signature element that make a task easy.

## 3. What NEXUS has today (audit)

One window hosts nine views (`UnifiedSidebar.tsx`) and keeps every opened view mounted. Findings from reading the code:

| Finding | Consequence |
|---|---|
| 4 visual languages: the old frosted `sidebar.css` (1,947 lines), `unified.css` and the Command Hub `hub.css` (2,705 lines), the newer "Console" (`stack*.css`), and `pr-list.css` (1,511 lines, GitHub colours) | The owner sees different designs by accident, not by intention |
| Only the Tech Stack view (and, since the first batch, a handful of views) use the shared `kit/` | No shared contract exists, so a change in one place can leak into another |
| Views stay mounted but hidden, and most CSS is global (unscoped class names such as `.crp-title`, `.analysis-card`) | A rule written for one view can restyle another |
| Hard-coded colours and window-opacity rules are enforced by CI gates for every stylesheet | A new design must define its colours as tokens and keep every window surface solid |
| No visible dock or close buttons (arrow keys dock, Esc closes) | Already the owner's rule; the header of every view has none |

## 4. The task of each view (what the design has to serve)

| View | The user's job | Time pressure | Data shape | Input |
|---|---|---|---|---|
| Screen analysis | See what NEXUS saw and which part is which | Glance, then point | An image plus 3 to 8 located items | Hover, voice ("which is 2?") |
| PR analysis | Decide: merge, approve, or fix | Careful read | A verdict, numbers, findings with code | Read, filter, copy, act |
| PR list | Triage what needs attention | Scan | A feed of items with actions | Scan, pick, act |
| Repo analysis | Understand a project quickly | Skim | Facts, shares, structure | Read |
| Research | Get a sourced answer | Read | A short answer plus sources | Read, follow up by voice |
| Reverse engineer | Learn how a site or app is built | Study | Technologies with confidence, headers, weight | Read, compare |
| Answer | Read a reply (text, table, code) | Read | Markdown | Read, copy |
| Approval | Allow or refuse one action safely | Decide now | One action and its exact effect | "Yes", "cancel", a click |
| Choices ("which one?") | Pick one of two to five options | Immediate | 2 to 5 labels | Say a number |
| Architecture map | See structure and impact | Explore | A graph | Pan, select |
| Command Hub | Configure NEXUS | Calm | Many settings | Click, type |
| Maps, place card | Find a place and go | Glance | A place or a route | Read, open |

## 5. Options compared

| | A. One uniform system (v1, v2) | B. Fully separate designs, nothing shared | **C. A family: a thin shared contract + one identity per task** |
|---|---|---|---|
| Fit to each task | Low to medium: every task gets the same cards | Highest | **High** |
| Consistency where it matters (keys, themes, accessibility) | High | Low: each view reinvents them | **High (the contract)** |
| Risk of style leaks and breakage | Low | **High** (nine unrelated stylesheets) | Low (scoping rule enforced by a CI gate) |
| Effort | Lowest | Highest | Medium to high |
| Maintainability | Highest | Lowest | **High** (small contract, independent views) |
| Matches the owner's request | No | Partly (but "without any issue" fails) | **Yes** |

**Recommendation: C.** It is what Google and GitHub actually do, and it is the only option that gives each task its own design without the views interfering with each other.

## 6. The contract every view must obey

Kept deliberately small. Everything else is free per view.

1. **Window**: every surface solid (the existing opacity gate), no blur or transparency, no visible scrollbars.
2. **Header**: the view's own header may look different, but it has **no dock arrows and no close button**, it is the drag handle, and it names the view.
3. **Keys**: left and right arrows dock, up and down set depth, Esc closes. No view may swallow them (text fields excepted).
4. **Theme**: light and dark, from tokens only. Each view defines its own palette as tokens in `theme/tokens.css`, under its own prefix (for example `--scr-*`, `--rev-*`); no literal colours in a view's CSS (the existing ratchet), and the contrast gate must pass.
5. **Scoping**: every view has one root class (for example `.view-screen`) and **every selector in its stylesheet starts with that class**. A new CI gate (`check-sidebar-scopes.mjs`) fails the build on an unscoped selector. This is what removes the "without any issue" risk.
6. **Accessibility**: visible focus rings, a text label for every status (never colour alone), reduced-motion support, minimum text 12 px and contrast from the gate.
7. **Layout**: container queries (not window media queries) for the widths 420, 520, 740 and 960.
8. **Fonts**: system fonts only (no web fonts, the app works offline); each view may choose its own stack from the system set.
9. **Size**: one stylesheet per view, no more than 600 lines; shared primitives live in `kit/` and stay few (icon, pill, button).
10. **Tests**: each view ships a model file with unit tests, a render test, and an entry in the gallery page.

## 7. The family (one identity per task)

Each identity has a name, a layout idea, a signature element, a type and colour character, a motion note and the thing it must not borrow.

| View | Identity | Layout and signature element | Character |
|---|---|---|---|
| Screen analysis | **Spotlight** | The screenshot is the page: large, with numbered pins; hovering a finding dims the rest and rings its region; hovering a pin lights its row. Findings sit under it as a compact list | Visual and quiet; neutral palette so the picture carries the colour; short, snappy motion |
| PR analysis | **Review desk** | A verdict banner, a diff-squares strip, a five-axis review radar, findings as a filterable list with code and copy, checks as a live list; Merge / Approve / Comment pinned | Technical and dense; a code font for paths and snippets; status colours with shape and word |
| PR list | **Inbox** | A triage feed: day groups, a card per PR with tags, an author row and two actions; segmented filter on top | Fast to scan; the GitHub dashboard feel the owner likes stays here, in its own view |
| Repo analysis | **Atlas** | An overview poster: name and avatar, count tiles, a segmented language bar, a commit heatmap, a structure bar chart and a quality checklist | Calm, map-like; a green-ish data ramp allowed; no actions |
| Research | **Briefing** | Editorial: a topic headline, a short answer with numbered citations, an agreement ring, key points, a source list with a favicon tile and domain, follow-up chips | Reading-first; a serif or humanist body and large line height; very little chrome |
| Reverse engineer | **Blueprint** | Technical: a confidence ring, a tile grid of technologies with their own logos, bars with values, security headers as a checklist | Engineering-drawing feel: thin lines, a mono font for versions; colour only in the logos (the owner's rule for this view stays) |
| Answer | **Page** | A clean page: a TL;DR, then prose, tables, callouts and code in a strict type scale; Read aloud and Copy pinned | Typography only; no cards except for code and callouts |
| Approval | **Gate** | One decision on a spare screen: what will happen as a large headline, the exact message or action in a bubble, a countdown ring for the 5-second voice window, two big buttons | High contrast and calm; large type; the only view with a countdown |
| Choices | **Pick** | Large numbered rows that are exactly what can be said; a drain bar for the 30-second window | Voice-first; the number is the biggest thing on screen |
| Architecture map | **Map** | A full-height graph canvas with a legend and a blast-radius panel | Diagram aesthetic; its own zoom and pan |
| Command Hub | **Control room** | Settings in grouped sections with switches and short help; the settings keep their current structure | Neutral and compact |
| Maps, place card | **Card** | A place photo or map first, then facts and one "go" action | Image-led |

Why these differ on purpose: the Screen view is judged by whether you can find the part of the picture, so the picture is the layout. The Gate is judged by
whether you can decide safely in two seconds, so it has almost nothing on it. The Briefing is judged by whether you can read and trust it, so it is
typographic. Making them share cards would make all three worse.

## 8. How it is built without breaking anything

```
frontend/src/sidebar/
  contract/           the thin shared part: header shell (no buttons), icon, pill, button, the scoping and token rules (docs + tests)
  views/
    spotlight/        View.tsx, spotlight.css (all selectors under .view-spotlight), model.ts, *.test.ts
    review-desk/      ...
    inbox/            ...
    atlas/            ...
    briefing/         ...
    blueprint/        ...
    page/             ...
    gate/             ...
    pick/             ...
theme/tokens.css      one token block per view (--scr-*, --rev-*, ...); no colours anywhere else
scripts/check-sidebar-scopes.mjs   new gate: every selector in views/*/*.css starts with the view's root class
```

Conflict controls: scoped selectors (enforced), tokens per view, no global element selectors, one root class per mounted view, the old CSS for a view
deleted in the same change that ports it (so two designs never fight over one element), and the existing gates (colours, opacity, contrast, IPC, turn
ends, hygiene) stay green on every change.

## 9. Phases

| Phase | Work | Done when |
|---|---|---|
| P0 | Owner reviews this plan and the identities; decisions in section 11 | answered |
| P1 | The contract: header shell, the scoping gate, the token pattern, a gallery page that lists every view | gate passes and rejects an unscoped selector (tested) |
| P2 | Prototype each identity on localhost, one page per identity, with mock data, so the owner picks per view | owner approves each identity |
| P3 | Port the views in this order: Gate, Pick, Spotlight, Review desk, Page, Briefing, Atlas, Inbox, Blueprint (already close), then Map and the rest | each view: tests, gates, and a screenshot set at 420, 520, 740 |
| P4 | Delete the old CSS per view as it is replaced; live check in the real window, light and dark, 125 % and 150 % scaling | the old stylesheets are gone or only hold the views not yet ported |

Each phase ships as its own branch and pull request. The first batch already on `feat/sidebar-system` (shared kit, approval, choices, PR analysis, repo analysis,
screen analysis under one look) is reviewed against this plan: it is kept where it matches an identity (the Gate, Pick, parts of Review desk and Atlas) and
reworked where it does not.

## 10. Risks and answers

| Risk | Answer |
|---|---|
| A new design leaks into another view | Scoped selectors enforced by a gate; one root class per view |
| A palette breaks in light or dark | Tokens only, in `tokens.css`, with the contrast gate |
| Nine designs are too much to maintain | Each is small (one stylesheet under 600 lines, one model with tests) and independent |
| A view loses a function while being redesigned | Each port keeps the old component's logic and props; the old tests are kept or rewritten, never deleted |
| It looks different from what was approved | P2 shows each identity on localhost first; nothing is ported without approval |
| The window rules regress (solid, no scrollbars, keys) | The contract gates and per-view render tests cover them |

## 11. Decisions for the owner

1. Is option C (the family with a thin contract) the direction?
2. Are these the right identities, and are any of the names or ideas wrong for the task? Anything to add, merge or drop?
3. Colour: the family allows each view its own palette. Should status colours (green, amber, red) be allowed in the Review desk and Atlas, with shape and word as well,
   or should every view stay black, white and grey except logos?
4. Order of work: the order in P3, or another.
5. The GitHub-style Inbox: kept as the PR list's own design, as in v2?
6. Reading font for the Briefing (a system serif such as Georgia, or the same humanist sans as the rest)?
