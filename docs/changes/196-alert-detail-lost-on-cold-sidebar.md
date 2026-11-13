# 196: clicking an alert could open an empty sidebar

**Bug (found 2026-11-14, live).** Clicking an alert (or simulating the click) when the sidebar window did not exist yet opened
the panel but showed only "Esc to close": the alert detail never appeared. `show_event_detail` created the window, then
spent a few hundred ms capturing the blurred backdrop, and only then stored the content the new window reads once on mount.
If the window mounted first it read nothing ("no pending content — sidebar will be empty"). It worked in earlier tests by timing luck.

**Fix.** The content is stored before the window is created; the backdrop is attached afterwards only if the window has not
already taken the content (`attach_backdrop`), so a late store cannot resurrect a stale alert in the next cold sidebar.

**Tests.** Two unit tests (backdrop attaches to waiting content; taken content is not stored again). The race itself is checked live:
fresh start, send an alert, click, repeat from a cold sidebar several times.
