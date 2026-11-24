# Windows 11 toast UX and non-activating overlay mechanics for a Tauri 2 (WebView2) notification toast

Scope note: ~12 tool calls. Search results were mostly snippet-level; where I could only get a snippet, or nothing, it is marked as Gap or Inference. Nothing here was tested on a real machine.

## 1. Windows 11 / Fluent toast UX: size, position, duration, stacking, DND, accessibility

### Takeaway
Microsoft's guidance is about *what* a notification should be (informative, not noisy, deep-linking on click, honour Focus Sessions), not pixel sizes for custom windows. The only hard numbers found are for the native toast: 5 s default display, user-adjustable 5 s to 5 min in Accessibility settings, and 48x48 logo, 364x180 hero image, 3 text lines, 5 buttons. A custom toast should mimic these and honour the user's duration setting and the notification-state APIs.

### Cited Findings
- Native toast anatomy: the shell draws the attribution area (app name + icon on Windows 11), a close button and an ellipsis menu; these cannot be overridden in the payload. — [App notification content](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)
- Text: at least 1, at most 3 text elements; title up to 2 lines, the two body elements up to 4 lines combined. — [same](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)
- Image sizes at 100% scale: app logo override 48x48, hero 364x180; button icons 16x16; remote images limited to 3 MB (1 MB metered), dropped if exceeded while the rest of the toast still shows. — [same](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)
- Max 5 buttons (context-menu items count toward it). If icons are used on one button, all buttons need icons. Icon-only buttons should carry a tooltip so Narrator can read them. Provide a high-contrast variant of icons. — [same](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)
- UX principles: notifications should save time, not distract; too many interruptions make users turn the channel off; clicking the toast should open the app in the notification's context; clear stale notifications from Notification Center once the content is viewed. — [Notifications design basics](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)
- Windows 11 Focus Sessions suppress notifications; apps can detect them with `FocusSessionManager` and adapt. A suppressed toast goes straight to Notification Center without a banner. — [Notifications design basics](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)
- Default banner time is 5 s; the user can raise it to a max of 5 minutes (Settings > Accessibility > Visual effects, "Dismiss notifications after this amount of time"; label varies by build). Stored as `MessageDuration` under `HKCU\Control Panel\Accessibility`, values 5-300 s, lower values behave as 5. Secondary (third-party how-to) sources, consistent with each other. — [Eleven Forum](https://www.elevenforum.com/t/change-how-long-notifications-stay-open-in-windows-11.819), [KapilArya](https://www.kapilarya.com/how-to-change-notifications-duration-in-windows-11)
- `SHQueryUserNotificationState` / `QUERY_USER_NOTIFICATION_STATE` values: 1 QUNS_NOT_PRESENT (screen saver, locked, inactive FUS session), 2 QUNS_BUSY (full-screen app running or Presentation Settings applied), 3 QUNS_RUNNING_D3D_FULL_SCREEN (exclusive-mode D3D), 4 QUNS_PRESENTATION_MODE, 5 QUNS_ACCEPTS_NOTIFICATIONS (free to send), 6 QUNS_QUIET_TIME (first hour after new account/OS upgrade), 7 QUNS_APP (a Windows Store app is running). During quiet time, if one of the blocked states (1-4) applies, only that value is returned. — [QUERY_USER_NOTIFICATION_STATE](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state)
- Microsoft advises calling it before showing balloon-like notification UI. — [search summary of same docs](https://learn.microsoft.com/tr-tr/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state)
- Toast scenarios: Reminder/Alarm stay on screen until dismissed (need at least one button, else treated as a normal toast); Urgent can break through Focus Assist if the user allows it for the app (needs Insider build 22546+ at time of doc). — [App notification content](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)

### Inferences
- A 5 s custom toast equals the OS default but would be shorter than what a user with the Accessibility duration raised expects. Read `MessageDuration` (HKCU\Control Panel\Accessibility) and use max(5 s, that value); this is also the main accessibility minimum-duration lever.
- Hover-to-pause, pause on keyboard focus, and a visible close affordance mirror native toast behaviour (the native one has a close button). I did not find a Microsoft source stating native hover-pause behaviour; treat it as common practice, not a guideline.
- Suggested suppress rule: do not show the window when state is 1, 2, 3 or 4; for 6/7 and Focus Sessions queue the item to the in-app sidebar instead of a banner. Queue (don't stack) with max 1 visible; new items replace or follow after the previous one ends; coalesce bursts.
- Position: native Windows 11 toasts sit bottom-right just above the taskbar (screenshot in the Microsoft UX page shows "above the task bar"; position itself not stated in text). Use monitor work area, not full monitor rect, so the taskbar is never covered.
- Reduced motion: respect `prefers-reduced-motion` in CSS (WebView2 follows the OS animation setting) and avoid slide animations when set; high contrast: use forced-colors CSS. These are standard web platform behaviours, not verified here for WebView2 specifically.

### Gaps
- No Microsoft source found giving exact pixel size/margins of the native Windows 11 toast, stacking limit, or hover-to-pause behaviour.
- UIA live-region/Narrator behaviour for custom windows: not researched in sources. Inference only: a custom toast should set an ARIA live region (`role="status"`/`aria-live="polite"`, or `role="alert"` for urgent) in the page; whether Narrator announces text of a non-focused WebView2 window reliably is unverified and needs a test.

## 2. Tauri 2 / tao / WebView2 non-activating overlay techniques

### Takeaway
Tauri 2 exposes `focusable(false)` / `set_focusable`, `always_on_top`, `skip_taskbar`, `transparent`, `shadow`, `set_ignore_cursor_events`, and `Monitor::work_area()`. The raw Win32 approach for "visible and clickable but never activating" is WS_EX_NOACTIVATE (blocks activation, but see Raymond Chen) or `WM_MOUSEACTIVATE -> MA_NOACTIVATE`, plus `SW_SHOWNOACTIVATE`. Per-region click-through is not a built-in; the existing project approach (cursor poll toggling `set_ignore_cursor_events`) is the documented workaround.

### Cited Findings
- Tauri `WebviewWindowBuilder` has `focusable` ("whether the window will be focusable or not"), `focused` (initially focused), `shadow`, `transparent`, `skip_taskbar`, `always_on_top`. — [docs.rs WebviewWindowBuilder](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html)
- Windows `shadow` note: `false` has no effect on a decorated window; `true` on an *undecorated* window gives a 1px white border and, on Windows 11, rounded corners. — [same](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html)
- Windows `transparent` note: `no_redirection_bitmap` can help avoid a white flash when creating a transparent window. — [same](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html)
- tao 0.34.1 added `WindowBuilder::with_focusable` and `Window::set_focusable` (PR #1120). On macOS an already-focused window cannot be unfocused by set_focusable(false). — [tao release 0.34.1](https://v2.tauri.app/release/tao/v0.34.1/), [Tauri JS docs mirror](https://jetbrains.github.io/kotlin-wrappers/kotlin-tauri-apps-api/tauri.apps.api/-window/set-focusable.html)
- A Tauri commit "respect `focused: false` for webview" (closes #7519) indicates the webview itself previously ignored focused:false; it also notes tao sends Focused(true) even for invisible windows. — [tauri commit e6d9b67](https://www.github.com/tauri-apps/tauri/commit/e6d9b670b0b314ed667b0e164f2c8d27048e678f)
- Raymond Chen: WS_EX_NOACTIVATE prevents activation but the window then ignores input in the described scenario; instead answer `WM_MOUSEACTIVATE` with `MA_NOACTIVATE` and the window still receives WM_LBUTTONDOWN/MOUSEMOVE/LBUTTONUP. `MA_NOACTIVATEANDEAT` discards the activating click (avoid). — [The Old New Thing](https://devblogs.microsoft.com/oldnewthing/?p=94295)
- Electron analogue: on Windows `focusable:false` toggles WS_EX_NOACTIVATE and also implies skipTaskbar:true. — [Electron commit](https://ayakael.net/mirrors/electron/commit/50f18370976410abb33d94bbb76946b482f050dd)
- `set_ignore_cursor_events` was added to tao in PR #421 and applies to the whole window; there is no per-region mode. Community workaround: transparent window, toggle ignore when the cursor is over/outside the drawn area; a Tauri plugin (tauri-plugin-polygon) does this by polling and turning click-through off over registered regions. — [tao PR 421](https://github.com/tauri-apps/tao/pull/421), [tauri-plugin-polygon source](https://docs.rs/crate/tauri-plugin-polygon/latest/source/src/desktop.rs)
- Tauri issue #11461 (Windows 10): `setIgnoreCursorEvents(true)` on a child WebviewWindow did not let clicks through; closed "not planned" Jul 2025, no confirmed workaround. — [issue 11461](https://github.com/tauri-apps/tauri/issues/11461)
- When a window ignores cursor events the page cannot detect pointer enter/leave (reported); JS workarounds need focus. — [tao/tauri search results](https://github.com/tauri-apps/tauri/issues/9250)
- Tauri `Monitor` exposes `work_area()` (physical rect), the OS work area excluding taskbar/appbars; taskbar position is per-monitor on Windows, so a fixed bottom-right guess may be wrong on secondary monitors. Microsoft's guidance is to use the work area from `GetMonitorInfo`. — [tauri-runtime-wry monitor](https://docs.rs/crate/tauri-runtime-wry/latest/source/src/monitor/mod.rs), [Old New Thing on per-monitor taskbar](https://devblogs.microsoft.com/oldnewthing/?p=43353), [Win32 multi-monitor positioning](https://learn.microsoft.com/en-us/windows/win32/gdi/positioning-objects-on-multiple-display-monitors)
- `tauri-plugin-positioner` appears to compute from full monitor bounds, risking taskbar overlap (snippet-level). — [positioner source](https://docs.rs/crate/tauri-plugin-positioner/2.3.1/source/src/ext.rs)
- WebView2 can steal focus on first navigation completion in some hosts (WinForms report, workaround not universal). — [WebView2Feedback #862](https://github.com/MicrosoftEdge/WebView2Feedback/issues/862)
- Another WebView control forum: WS_EX_NOACTIVATE on a popup broke focus return; fix was clearing the bit. — [EssentialObjects thread](https://www.essentialobjects.com/forum/postst12425_Bug-when-switching-back-and-forth-with-popup.aspx)

### Inferences
- Recommended recipe (unverified on this build): create the window hidden with `focusable(false)`, `focused(false)`, `always_on_top(true)`, `skip_taskbar(true)`, `decorations(false)`, `transparent(true)`, `shadow(false)` (draw the shadow in CSS inside padding); position with `work_area()` of the monitor under the cursor/active window, converting with that monitor's scale factor; then show with Win32 `ShowWindow(hwnd, SW_SHOWNOACTIVATE)` plus `SetWindowPos(..., HWND_TOPMOST, SWP_NOACTIVATE)` instead of Tauri `show()`, because tao's show path may activate. Verify with a test that the foreground window (GetForegroundWindow) is unchanged and caret stays in the previous app.
- If `focusable(false)` (WS_EX_NOACTIVATE) makes the toast unclickable (per Chen), subclass the HWND and return MA_NOACTIVATE from WM_MOUSEACTIVATE, and avoid the NOACTIVATE ex-style. Because WebView2 hosts a child HWND that has its own mouse-activate handling, the subclass may need to hit the child windows too; this is a risk to test.
- Click-through for the transparent area: simplest is to make the toast window exactly the toast size (no oversized transparent canvas), which avoids needing set_ignore_cursor_events at all. The project's 420x(half sidebar height) window is small, so this is preferable to the polling workaround.
- Create-on-demand vs keep hidden: project notes (AGENTS.md) record that destroying a dynamic WebView2 window frees ~250 MB and that async create is needed to avoid deadlock; a hidden pre-created toast window costs that RAM permanently, but create-on-demand adds WebView2 startup latency (seconds in the project's cold-boot notes) and risks the transparent white flash. Compromise: create on demand, reuse for a burst, destroy after an idle period (e.g. 30-60 s); or pre-create at first notification. Not measured.
- Full-screen suppression: poll SHQueryUserNotificationState at show time (state 2/3/4 => skip) rather than relying on topmost behaviour; a TOPMOST window can draw over borderless full-screen video.

### Gaps
- Did not read tao's Windows source to confirm `with_focusable` sets WS_EX_NOACTIVATE or whether it also hides the taskbar button; confirm in tao `src/platform_impl/windows`.
- No confirmed WebView2-in-Tauri report of focus theft for `focusable(false)` windows; behaviour with WebView2's child HWND not verified.
- Drop shadow + transparency, z-order vs taskbar (TOPMOST band vs Shell_TrayWnd) and animation cost: no dedicated sources found; only the Tauri `shadow` doc above.

## 3. Native WinRT / Windows App SDK toasts vs custom window

### Takeaway
Native toasts give OS-consistent look, Notification Center history, Focus Assist/DND handling and accessibility for free, but fix layout (3 text lines, 5 buttons, fixed image sizes) and do not allow a precise 5 s lifetime or a 420 px custom layout. A custom window gives full control but must reimplement DND, accessibility and queueing.

### Cited Findings
- Payload is toast XML (or Windows App SDK `AppNotificationBuilder`, since WinAppSDK 1.2); `launch` attribute passes arguments back on click for deep linking; buttons carry `arguments`. — [App notification content](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)
- Button activation: foreground with arguments, or protocol launch of another app; background activation explicitly supported for UWP only; Windows App SDK apps always launch foreground (the app can inspect activation args and exit if it only wanted to handle the action); system snooze/dismiss supported. — [same](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)
- Duration: no per-toast timeout in the content docs; scenarios Reminder/Alarm/IncomingCall persist until dismissed; otherwise the user's system duration applies. — [same](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content), [duration setting](https://www.elevenforum.com/t/change-how-long-notifications-stay-open-in-windows-11.819)
- Toast schema limits: 3 text elements; searches found no explicit payload byte limit in the schema pages. — [App notifications schema](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-schema)
- Can set expiration/suppress popup (goes straight to Notification Center) — the UX page describes suppression; API names not verified here. — [Notifications design basics](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)

### Inferences
- A native toast cannot be forced to disappear at exactly 5 s except by removing it programmatically from history (e.g. `Remove` / `ExpirationTime` APIs; not verified in this research). It also needs an AppUserModelID/registration; for an unpackaged Tauri app this is a known extra setup step (not researched; Tauri's notification plugin on Windows is a related path).
- Native is the better choice for "must respect DND and accessibility with zero effort"; custom is the better choice for the 420 px sidebar-styled card with click-to-open-sidebar and exact timing. A hybrid is possible: custom window normally, native toast as fallback when state is Focus Session (shown only in Notification Center).

### Gaps
- Exact byte/size limit of toast payload, expiration API names, unpackaged-app activation details: not retrieved.
- Whether Tauri 2's official notification plugin supports click callbacks/action buttons on Windows: not researched.

## 4. Pitfalls checklist (mostly inference from the above)
- White flash on transparent window creation (Tauri doc points to `no_redirection_bitmap`).
- Shadows: Tauri `shadow(true)` on an undecorated window yields a 1px white border on Windows; prefer `shadow(false)` and a CSS shadow inside transparent padding.
- Focus theft: tao may emit Focused(true) at creation; verify GetForegroundWindow before/after.
- `set_ignore_cursor_events` is window-wide and has an unresolved Windows report (#11461); avoid it by sizing the window to the toast.
- Multi-monitor/DPI: use `work_area()` physical coordinates with the target monitor's scale factor; taskbar may be on any edge and differs per monitor.
- Full-screen/presentation: honour SHQueryUserNotificationState (values 2, 3, 4, 1) and Focus Sessions.
- Memory: each WebView2 window is a large process tree (project notes: ~250 MB per window freed on destroy).
