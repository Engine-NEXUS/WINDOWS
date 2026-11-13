# 194: a clicked alert now shows even when another sidebar view is open

**Bug.** Clicking an alert's toast with the Command Hub (or the PR list or the architect map) open did nothing visible.
Rust asks the unified sidebar to switch to a view named `coderabbit`, but the sidebar did not know that name and ignored
the request, so the alert was drawn in the assistant view, which stayed hidden behind the open view. It worked only when
the sidebar was closed or already on the assistant view. Found 2026-11-14 while clicking a test alert from the Features page.

**Fix.** `sidebarView.ts` maps `coderabbit` to the assistant view (the view that draws the detail); `UnifiedSidebar.tsx` uses it for both
`sidebar:set_view` and the pending view read on mount. Three tests (the mapping, every real view unchanged, unknown names
ignored); removing the alias makes the mapping test fail.

**Verified:** sidebar tests 135, tsc, production build. **Not run:** the click with the Hub open in the live app (needs a rebuild).
Live check: open the Command Hub, send a test alert from the Features page, click the toast; the alert detail should replace the Hub with a Back path.
