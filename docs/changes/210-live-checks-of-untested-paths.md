# 210: Live checks of the untested paths of Maps and reverse engineer (2026-11-26)

Isolated contributor build on a hidden desktop, quiet mode; the owner's NEXUS untouched.

## Passed live

* "avoid tolls" on the open route: no claim of avoiding, the note says it is ignored.
* A refused scan: the stack sidebar shows only "Not scanned" and the reason.
* A trip reminder fired at its time (set 19:49 for 19:52, fired 19:52:24) with the on-screen reminder. It was **not spoken**: nobody is at a hidden desktop, so it stayed a card, which is the designed behaviour.
* App scan engine on ten real installed programs (temporary test calling `app_probe::probe`, not through voice).

## Defects found and fixed

1. **System programs were never refused:** the canonical `\\?\` form did not match the Windows folder, so Notepad was scanned (and called "GameMaker").
2. A full browser (Brave) was reported as Electron and CEF.
3. Cursor, Antigravity, Devin and Brave listed Chromium or Node.js twice; the `app.asar` signal could never match.
4. A late trip reminder answered with a garbled sentence.
5. `run-isolated-test.ps1` released the hidden desktop before the app attached to it, so the app died with `0xC0000142` and an empty log (the old "dies on its first start after a rebuild"). It now waits until the app is up.

After the fixes: Notepad is refused, Brave is "Chromium, Rust", no duplicates.

## Still wrong in the app scan

qBittorrent (Qt linked into the exe) and NEXUS itself (Tauri) report nothing; `git-bash.exe` reports Avalonia because another tool's DLLs sit in the same folder tree.

## Could not be tested

* **Spoken commands:** no microphone is connected to this machine ("found 0 input devices"), and the debug fake microphone needs a real capture stream. `scripts/drive-isolated-speech.mjs` is ready; it has produced no result yet.
* Related observation: with no microphone, a capture that was started never ended (8 minutes), and the trip reminder that fired meanwhile was not spoken. Not investigated.
* **Screenshot tour, browser hand-off, desktop-app command by voice:** a hidden desktop has no foreground window; the tour also needs a Gemini key.
* Location refused (a Windows setting), live traffic (a Maps key), the Maps button (it would open the owner's browser).
