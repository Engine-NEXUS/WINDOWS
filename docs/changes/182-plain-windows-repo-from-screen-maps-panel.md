# 182 - Plain windows, repo found from a name on screen, map-only "where is", Maps in our own panel

Date: 2026-10-31. Findings and plan: [audit 01](../research/ui-flow-audit/01-window-and-flow-audit-2026-10-31.md). Source and tests; not run live.

## Plain windows
No sidebar view draws a title bar or dock buttons any more (screen analysis, tech stack, annotate, Architect, GitHub dashboard, Command Hub). Left / Right arrows dock the window (one handler in `UnifiedSidebar`, only for the visible view); Shift + Left / Right resize a pinned panel. An invisible strip at the top drags the window. The clipped screen-analysis heading in the owner's photo came from that header row.

## Repo from a name on screen
"Analyse this repo / the repo / this report" (the speech engine hears "repo" as "report") reads the open page; if no repository page is open, `repo_hint` asks the vision model which GitHub repositories the screen names (an email, a notification), and bare names are looked up in the owner's connected GitHub and orgs (`github_repo_index`). One match runs the normal repo analysis (overview sidebar + Architect window); several ask which; none falls back to screen analysis. "Analyse this email" does the same for repos the email names and opens that repo's map.

## "Where is X" shows only the map
The details card in the sidebar is gone; one spoken line and the map window.

## Maps in our own panel + panel layout
`maps_panel::show_map` frames Google's Maps Embed API in a frameless `maps-panel` window (`?pin=maps`). `panel_layout` places every open panel (Architect map, Maps) in the free span beside the response sidebar in opening order, shrinking to minimum widths before overlapping, and remembers Shift + arrow sizes. It falls back to the old window when there is no key, the Embed API is not enabled for it (checked once per 10 minutes), `mapsEmbed` is off, or a route has no origin. The app CSP now allows `frame-src https://www.google.com`.

## Quick fixes
Annotate is a 320 x 380 window (it used to cover the right half of the screen); the Architect empty state stops spinning after 8 s and says what to type; three Hub rows have their own icons; spoken screen analysis is two sentences with the rest in the sidebar; the console shows `[WAKE]` and cloud-check results; `docs/archive/MAHAYUDDH_UI_SPEC.md` is valid UTF-8 (the GitHub Pages build failed on it, which is what NEXUS spoke as "Build failed in WINDOWS").

## Needs a live check
The embed needs "Maps Embed API" enabled on the key you saved as the Google Maps/Places key; without it the old window is used. Panel positions at 125 / 150 % scaling, three panels at once, and Shift + arrow resizing.
