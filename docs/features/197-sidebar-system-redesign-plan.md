# 197 — One design system for every sidebar view: research, plan and prototype

Date: 2026-11-22. Status: **prototype built, waiting for the owner's review. Nothing in the app uses it yet.**
Prototype: `frontend/dev-sidebar-system.html` (run `npm --prefix frontend run dev`, open `http://localhost:5173/dev-sidebar-system.html`).

Owner request: redesign the sidebar views that show information (screen analysis, PR analysis, reverse engineer, research and the rest) so
they can be set, run on localhost, then refactored and improved until they are right.

---

## 1. What exists today (research)

The sidebar is ONE window (`UnifiedSidebar.tsx`) that hosts nine views. It resizes per view (520 dock, 740 modal, 960 workspace) and the views
adapt with container queries. Views are mounted lazily and kept mounted.

| View id | Component | What it shows |
|---|---|---|
| `assistant` | `SidebarApp.tsx` | Rendered markdown answers (tables, code with copy, callouts, images with a lightbox), the repository analysis dashboard, the CodeRabbit PR review panel, action confirmation cards, desktop choices, the topic title for research answers, read-aloud, font size |
| `spatial` | `SpatialDashboard.tsx` | Screen analysis: the screenshot with pins, the overview, the findings, the foreground review |
| `pr-list` | `PrListApp.tsx` (+ repos, starred, activity graphs) | The pull request list with Analyse and Merge |
| `architect` | `ArchitectApp.tsx` | The architecture map: import graph and blast radius |
| `stack` | `StackView.tsx` | Reverse engineer: the newest design (console layout) |
| `settings` | `SettingsSidebarApp.tsx` (+ `hub/`) | The Command Hub |
| `annotate`, `maps`, `place` | `AnnotatePalette`, `MapsPanel`, `PlaceCard` | Annotation palette, directions, a place card |

Design languages found in the code: `sidebar.css` (1,947 lines, the original frosted look), `unified.css` (shell and Command Hub tokens),
`stack.css` / `stack-v2.css` (the owner's "Console" redesign, 2026-11-16), `kit/` (AppBar, Card, Chip, DockButtons, Donut, BarChart,
MiniBar, LogoTile: the shared components).
Rules the code already enforces and the redesign keeps: panel windows are solid (a CI gate rejects blur or transparency),
no visible scrollbars, container queries not window media queries, theme tokens for light and dark (a contrast gate), no inline `style`
props in the stage window (CSP), colour only inside technology logos in the stack view.

The stack view's "Console" layout (deep charcoal canvas or soft grey in light, rounded cards with a hairline border, big numerals over
tiny caps labels, KPI tiles, bars with the value on the right, a filled-pill segmented control, an icon tab bar) is the newest and
the owner's own design from reference images. It is the base of this plan.

## 2. Problems the redesign solves

1. Four visual languages in one window: the answer view, the PR panel and the dashboards look like different apps.
2. Information order: most views open with text. The decision or the headline (verdict, what was found, what will be sent) is not first.
3. Severity and status are mostly colour; the owner's rule is black, white and grey, so shape and word have to carry it.
4. Actions (Merge, Approve, Confirm, Copy, Read aloud) scroll away with the content.
5. Keyboard and voice paths are not visible: the buttons do not show the key, the voice answer is not shown on the confirmation card.
6. The same fact is shown in different ways in different views (number tiles in one, badges in another, text in a third).

## 3. The design system

One kit, used by every view. Built from the existing `kit/` plus four new pieces.

**Tokens (dark / light)**: canvas `#0b0b0d` / `#eeeef0`, card `#151519` / `#ffffff`, second card `#1d1d22` / `#f3f3f5`, hairline
`rgba(255,255,255,.09)` / `rgba(0,0,0,.08)`, text, dim and faint text steps, one "ink" colour (white in dark, near-black in light) used for
everything that has to stand out: filled pills, the numbered badges, bar fills, the primary button. Radius 18 for cards, 12 for inner
blocks. Every surface is solid. Colour exists only inside logos.

**Type**: one humanist sans (Plus Jakarta Sans with Inter and Segoe UI fallbacks), tabular numbers. Scale: 34 / 28 big numerals, 20 section
heading, 17 title, 15 lead, 14 body, 12 to 13 secondary, 11 caps labels with 0.1 em tracking.

**Components** (the prototype's helpers are the React components to write):

| Component | Used for |
|---|---|
| `AppBar` (logo tile, title, subtitle pill, dock buttons) | Every view |
| `Card` (caps title, right slot) | Everything |
| `KpiTile` (caps label, big numeral, small sub line) | Numbers: files, added, removed, findings, stars, confidence |
| `Bar` (label, value on the right, track) | Checks, languages, structure, headers, confidence |
| `Pill` (default, solid, outline, dashed) | Status and metadata; solid / outline / dashed carry severity together with a word and a symbol |
| `Chip` (mono) | File paths, branches, identifiers |
| `SegPill` | Filters: All / High / Medium / Low |
| `NumberedList` (ink circle, title, line) | Screen findings, key points, choices |
| `Callout` (icon, caps word, text) | Note / Important / Tip: never colour alone |
| `CodeBlock` (with Copy) | Answers and PR suggestions |
| `SourceList` (favicon tile, title, domain, snippet, numbered citation) | Research |
| `ActionBar` (sticky bottom, primary and secondary, shows the key) | PR (Comment, Approve, Merge), Answer (Read aloud, Copy), Confirmation (Cancel, Confirm) |
| `VoiceTimer` (thin draining bar, "Listening for yes or cancel") | Confirmation and the choice list |

**Severity without colour**: High = solid pill with a filled triangle, Medium = outline pill with a diamond, Low = dashed pill with a
circle, each with the word. Status of a check = the bar fill and the word ("passed", "running").

**Behaviour rules**: the headline first (verdict, answer, what will be sent), then the numbers, then the detail; actions pinned at the
bottom; every action shows its key; every pin or citation number matches a numbered item; reduced motion removes the rise and bar animations;
focus rings are always visible; every control reachable by keyboard; the existing arrow-key docking is kept.

## 4. Per-view plan (what the prototype shows)

1. **Screen analysis**: the screenshot first with numbered pins that match the numbered findings; one line saying what it is looking at;
   chips for the app, the read time and the model; a foreground review card (readable text, sensitive content) with bars.
2. **PR analysis**: the verdict pill and the branch chip, the title, the author line; four number tiles; a severity filter; findings grouped
   with the file chip, one sentence and a copyable suggested code block; checks as bars; Comment / Approve / Merge pinned at the bottom.
3. **Repo analysis**: four number tiles (stars, forks, files, branch), an About card, a language donut with a legend, structure bars.
4. **Research**: the topic, a short answer with numbered citations, an agreement pill, the key points, a source list whose numbers match the
   citations, and follow-up chips for the next question.
5. **Reverse engineer**: kept as the owner designed it (it is the model); the prototype reproduces it so the others can be judged against it.
6. **Answer**: a type scale, a table that scans, callouts with an icon and a word, a code block that copies, and Read aloud / Copy pinned.
7. **Confirmation**: what will happen as the headline, the exact message as it will be sent, the five-second voice window as a draining
   bar, the keys on the buttons.
8. **Which one?** (app and control choices): large numbered rows that are exactly what can be said.

Not in this first prototype (planned next): the PR list, the architecture map, the Command Hub, maps and place card, the annotation palette.

## 5. How it will be built into the app (after the owner's review)

| Phase | Work | Risk |
|---|---|---|
| P0 | Review the prototype; list changes | none |
| P1 | Move the tokens into one `sidebar-system.css` (dark and light), pass the contrast and opacity gates; extend `kit/` with `KpiTile`, `SegPill`, `Callout`, `ActionBar`, `SourceList`, `NumberedList`, `VoiceTimer`; tests for each | low: new files only |
| P2 | Port the views one at a time, each behind its own tests, in this order: confirmation, choices, screen analysis, PR analysis, answer, research, repo analysis | medium: real data shapes differ from the mock |
| P3 | Retire the old `sidebar.css` rules view by view; the CSS-size and hard-coded-colour gates must stay green | medium |
| P4 | Live check in the real window at 420, 520, 740, 960 and at 125 % and 150 % scaling, light and dark, with real data | the part that needs the owner |

Each phase ships on its own branch and pull request with the repository gates and the unit tests, a before/after screenshot set, and a note of
what was not run live.

## 6. Decisions for the owner

1. Is the "Console" layout (the stack view's look) the right base for every view?
2. Severity and status without colour (solid / outline / dashed with a symbol and a word): keep, or allow one restrained accent colour for status?
3. Actions pinned at the bottom of the window: keep?
4. Density: comfortable only, or keep the compact option?
5. Which views come first in the port (suggested order above).
6. Anything missing from a view that today's version shows and the prototype dropped.

## 7. How to review

Open the prototype, choose a view on the left, switch theme, window width (420, 520, 740) and density. Arrow up and down change the view, T switches the
theme. The "About this view" box says what changed and why. Tell me what to change, per view; each round of changes is quick because the page is plain HTML and CSS.
