# 195 — "Analyse my screen" wave: design plan (prototype only, not wired into NEXUS)

Date: 2026-11-22. Prototype: `frontend/dev-analyse-wave.html` (run `npm --prefix frontend run dev`, open
`http://localhost:5173/dev-analyse-wave.html`). Nothing in the app uses it yet.

## The moment
The user says "analyse my screen". NEXUS must show, within about 150 ms, that it heard and is looking, and it must hide the
2 to 6 seconds the vision model takes. One burst of rings leaves the orb once and covers the whole screen.

## Timeline (all times from the spoken command)
| Phase | Time | What the user sees |
|---|---|---|
| Capture | 0 ms | Screenshot taken first (stage excluded from capture, as the tour already does). The wave must never be in the picture. |
| Inhale | 0–140 ms | Orb squeezes about 10%, capsule shrinks 3%. |
| Wave | 140 ms – about 1.5 s | Three rings leave the orb centre, 110 ms apart, easeOutCubic, front thins from 20 px to 7 px, later rings fainter, edge fades to nothing. Radius reaches the farthest corner. |
| Analysing | after the wave | Nothing loud. Optional style C keeps a soft breathing edge glow until the answer is ready. |
| Tour | when the model answers | Existing narrated tour takes over (rings, arrows, callouts). |

## Styles under review
1. **Rings** (default): the plain burst above.
2. **Scan**: same front, with a faint dot grid revealed behind it that fades; reads as "scanning".
3. **Rings + edge**: rings, then the screen border breathes softly while the model thinks.

## Design rules
- Black, white and grey only, like the orb. The ring has a white core and a dark rim on both sides so it reads on light and dark screens.
- Additive overlay only. The stage is transparent over the real desktop, so the wave cannot distort or blur it.
- Click-through: the overlay takes no pointer input. Drawn on a canvas (the stage CSP drops inline `style` props; canvas is not affected).
- Reduced motion: no travelling wave, one calm wash that fades in and out over about 0.6 s.
- Orb dock top or bottom: the wave leaves from wherever the orb is.
- Cost: one fullscreen canvas for about 1.5 s, then it is cleared and idle. Needs a check on the real stage at 125% and 150% scaling and on a second monitor (primary only today).
- Cancel: Esc or "stop" ends the wave at once and clears the canvas.

## Open decisions for the owner
- Which style (1, 2 or 3), ring count and duration.
- Whether the "analysing" hold state (style 3) is wanted.
- Whether the wave should also play for "what's on my screen" and for the read/scan (OCR) path, or only the vision tour.

## Wiring plan (after approval)
New stage component beside `TourOverlay`, triggered by a `stage:analyse_wave` event from `orchestrator::run_screen_tour`
right after the capture returns; ends on the first tour callout or on cancel. Frontend tests for timing and cleanup; live check by voice.
