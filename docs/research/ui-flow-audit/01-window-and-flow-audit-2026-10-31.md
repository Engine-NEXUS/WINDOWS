# Window UI audit and flow review (2026-10-31)

Method: every window page was loaded in the dev browser at the real sidebar width (520 x 900) and checked by script for horizontal
overflow, clipped text, text above the top edge, empty header rows, buttons without a name and targets under 24 px, then looked at
by eye. Flows were checked against the runtime logs of 28 Oct and 31 Oct and the code. Nothing was changed for this document.
Dev-browser note: `sidebar.html?view=...` is ignored when the page believes it runs inside Tauri; use `?pin=<view>` (spatial, stack, annotate, settings, pr-list, architect, assistant).

## 1. Window by window

| Window / view | Result |
|---|---|
| Command Hub (`pin=settings`) | Clean. "Close" and the title only, no dock buttons, no overflow. Issue: three rows (Memory Core, Actions & Permissions, Activity Manager) use the same bar-chart icon. |
| GitHub dashboard (`pin=pr-list`) | Clean. The fixed bottom hint bar ("Esc to close - Ctrl+Space") sits over the last card; needs bottom padding equal to the bar. |
| Architect map (`pin=architect`) | Plain, no dock buttons. Issue: opened with no repo it shows "Waiting for repository..." with a spinner forever. It needs a short timeout, then focus the owner/repo box and say what to type. |
| Tech stack (`pin=stack`) | Clean. Title and Copy action only. |
| Screen analysis (spatial) | Fixed: the clipped heading and both dock buttons are gone; the card starts at the top. |
| **Annotate** (`pin=annotate`) | **Layout problem.** The palette is about 200 px wide but sits in a 520 x full-height window, so a mostly empty panel covers the right side of the screen you are trying to annotate. Needs its own small geometry (about 300 x 340, top right). |
| Assistant sidebar | Not testable without a live reply (the card is off-screen until it has content, by design). Earlier tests pass. |
| Settings window (`settings.html`), Companion HUD, Orb Studio | No overflow, clipping or unnamed buttons at their design sizes (Orb Studio is built for 800 px and is not responsive; fine at its real size). |
| Setup | Persona grid is fine. Minor: the "American Female" badge wraps on SAMANTHA, and the Continue button sits outside the card at narrow widths (check at the real window size). |
| Stage overlay (orb, captions, chips) | Layout slots are unit-tested; not rendered live. |

Verified clean everywhere audited: no horizontal overflow, no text above the top edge, no unnamed buttons, no tiny targets
(the one "tiny" hit, the Architect repo input, is a padded field, not a defect).

## 2. Flows

| Flow | Status |
|---|---|
| Wake then command | Working in the `a8c6f94b` build: transcripts arrive ("analyse this", "where is the Charminar"), no more "missing lifecycle" errors. The cloud speech check is the only gate against false wakes; the model alone scores 0.7-0.9 on ordinary speech. |
| Wake visibility | The console hides the `[WAKE]` and `verify:` lines (run.ps1 whitelist), so accepted and rejected wakes cannot be seen. Add them to the whitelist. |
| "Analyse this" | Working: reads the screen. The spoken narration ran about 30 s for a short email; the overview should be 10 s at most with the detail in the sidebar. |
| "Analyse this repo" | Fixed in source ("report" is the usual mishearing of "repo"): reads the open page, falls back to the screen. Not yet: a repo that is only **named** in an email (see plan A). |
| "Where is the Charminar" | Working: Places lookup found it despite "charmina". Problem: it also opened the sidebar card; you want the map only. |
| Maps window | Works but is an ordinary web window (title bar, resizable, centred 960 px), so it overlaps the sidebar and the Architect window. See plan C. |
| **Spoken "Build failed in WINDOWS" at start-up** | **A real failure, not a bug in the watcher.** The GitHub Pages build on `main` fails: `docs/archive/MAHAYUDDH_UI_SPEC.md` contains invalid UTF-8 and Jekyll stops. Fix: re-encode that file to UTF-8 or exclude `docs/archive` in the Pages config. |
| Diagnostics: Google `[!!]` | "This device isn't linked to the NEXUS cloud service yet." Local Google sign-in is fine, but anything routed through the Worker's Google link is unavailable until the device is approved. |
| STT noise | Whisper turns room noise into foreign text ("Eruð til þú...") even with English pinned; the non-ASCII filter drops it correctly. No action. |

## 3. Plan, in order

**A. Repo named in an email (your GitHub login).** Ask the vision model for repo names on screen (`owner/repo` or a bare name), resolve with the existing GitHub repo index (your repos plus every org), then: one match runs the normal repo analysis (overview sidebar plus Architect window); several matches ask which; none falls back to screen analysis. Same lookup inside "analyse this email". Tests: parse of model answer, resolution cases, no-match fallback.

**B. Maps: map only.** Remove the sidebar card from "where / when is X". One spoken line, the map window, nothing else.

**C. Maps inside our own window plus a panel layout manager.**
1. A Maps view in our frameless glass window (`?pin=maps`) using Google's Maps Embed API in an iframe: free and unlimited, needs an API key with "Maps Embed API" enabled; Tauri's origin must be allowed if the key has referrer restrictions. If no key or the embed fails, fall back to today's window.
2. A layout manager that keeps the open panels in order from the right edge: response sidebar (520), then the Architect map and the Maps view, sized to the free width and never overlapping. Left / Right arrows already move a window one dock step; add Shift + arrows to resize in steps, drag strip to move, and remember each panel's size and position per screen.
3. Tests: pure layout function for 1366 x 768, 1920 x 1080, 2560 x 1440 at 100 / 125 / 150 % with one, two and three panels; no overlap, all on screen.

**D. Quick fixes.** Annotate window geometry; Architect empty-state timeout; Hub distinct icons; GitHub dashboard bottom padding; narration cap of about 10 s; `[WAKE]`/`verify:` lines in the console; re-encode the Pages file so the build stops failing.

**E. Verification.** Both-theme screenshots of every view at 520 px; a live check list for you (wake, analyse this, analyse this repo, where is X, the email case, window ordering).
