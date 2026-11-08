# dotted orb

Vendored from https://github.com/Jakubantalik/thinking-orbs (MIT, (c) 2026 Jakub Antalik; licence in
`LICENSE-thinking-orbs.txt`). `engine/`, `presets.ts`, `types.ts`, `theme.ts` are upstream code (one
unused-parameter rename in `engine/core.ts`).

NEXUS code on top: `paintInk.ts` (tinted ink + voice-level swell + fade alpha), `resolveOrb.ts` (design + size to draw
options, denser than upstream's 64 px tuning for the larger capsule), `DottedOrb.tsx` (canvas, one rAF loop,
cross-fade between designs, reduced motion). Preview: `frontend/dev-orbs.html`.
