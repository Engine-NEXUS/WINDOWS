# 179 - UI plan, Phase 5 (first part): accessible names, live region, and how the work was delegated

Date: 2026-10-29. Follows [178](178-ui-phase-4-stage-layout-and-overlays.md).

## What changed
- Icon-only buttons now have accessible names (sidebar scroll-to-top and lightbox, Architect error close, annotate tools and colour swatches).
- Toggle-style buttons expose `aria-pressed` (activity period, API-key category, appearance variants and swatches, Memory tabs and filters).
- New `stage/CaptionLiveRegion.tsx` (+ `liveRegionModel.ts`, test): a visually hidden polite live region that mirrors the caption, or says "Working" while the spinner is up. Review fix: the caption grows letter by letter, so the region publishes only after the text has been stable for 700 ms.
- The spinner wrapper has `role="status"`. `stage/a11y.css` holds `.sr-only` and a forced-colors block (system colours only).

## Delegation and review
- Antigravity: the accessibility pass. Reviewed by diff; kept within the allowed files, no colours or inline styles added, one flaw found (per-letter announcements) and fixed.
- OpenCode GLM 5.3 flash: timed out after 900 s and changed nothing; the forced-colors block was written by hand.
- Checks: tsc clean, vitest 777 pass, hard-coded colour gate unchanged (1,292).

## Not done
Screen-reader run in the real app; spinner Lottie sizing; remaining Phase 4 items.
