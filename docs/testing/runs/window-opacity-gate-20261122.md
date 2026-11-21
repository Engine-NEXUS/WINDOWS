# Window theming (solid panels) — gate hardening and mutation check (2026-11-22)

Source only, no NEXUS runtime. Gate: `scripts/check-window-opacity.mjs`, registry `scripts/window-opacity-registry.json`.

## Gaps found in the old gate (each reproduced before the fix)

The old gate passed when a panel's light-theme variable was set to `rgb(248 249 250 / 0.9)` and when `backdrop-filter: blur(3px)` was added to a panel stylesheet. Reasons and fixes:

1. It only read literal colours, never the variable behind `var(--x)`. Now every custom property is collected across all 31 frontend CSS files, all themes, and a surface passes only if every possible value is solid. An undefined variable with no fallback fails.
2. It missed `rgb(r g b / a)`, `hsl(... / a)`, `oklch`, `lab` and the `transparent` keyword (also inside `color-mix`). Added.
3. A rule like `.surface .child {}` was treated as the surface itself, and a `display: none` state counted as a paint. Now only rules whose subject is the surface count.
4. A stylesheet beginning with `@import` lost its first rule (the import glued onto the next selector), so for `sidebar.css` the whole dark-theme token block was invisible. Fixed and tested.
5. Blur was only checked on registered selectors. Now any `backdrop-filter` other than `none` in any frontend CSS outside `frontend/src/stage/` fails (the stage overlay is a transparent fullscreen window, not a panel).
6. Registry holes: the hub root `.hx` (the real Command Hub window), `.setup-root` and stack-v2 `.stack-view` were unregistered. Added (15 to 17 files; the gate passes on them).

## Result

`node scripts/check-window-opacity.mjs --selftest` passes 16 fixtures (was 6). `node scripts/check-window-opacity.mjs`: 17 registered files, 0 violations, no blur outside the stage.

Mutation check on the real stylesheets (`scripts/mutate-check.mjs`, spec `scripts/mutations-window-opacity.json`, detail `window-opacity-mutations-20261122.json`): **10 of 10 caught** (slash-alpha, rgba, 8-digit hex, hsla, `transparent`, `color-mix(... transparent)`, blur in an unregistered rule, in the sidebar, hub, architect, HUD, GitHub dashboard, setup and stack surfaces). The first runs found 3 survivors; each exposed one of gaps 4 and 6 above and was fixed before the 10/10 run.

## Not proven

Nothing was looked at in the running app (no screenshot of any panel in either theme); the gate reads stylesheets only and cannot see inline styles or native window material. Owner steps: `docs/testing/four-star-screen-tour-owner-script.md`, part B.
