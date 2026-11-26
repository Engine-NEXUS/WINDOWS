# 213: One key, one name: simpler voice typing (2026-11-27)

Owner: "one command is enough, there are too many commands". This replaces the extra shortcuts of 211.

| You do | NEXUS does |
|---|---|
| Tap Ctrl+Space, or say "Nexus" | runs what you say as a command |
| Hold Ctrl+Space with the cursor in a text box, talk, let go | types what you said there |
| Hold Ctrl+Space with no text box under the cursor | runs it as a command |
| "Nexus, type for me" | keeps listening and types each sentence |
| "done" (alone, after a pause), "stop typing", "Nexus, stop" | ends typing; the text stays, nothing is sent |
| "Nexus, send" (alone) | presses send once (Enter; Ctrl+Enter in Gmail and Outlook) and ends typing |

"send", "send it", "stop" and "cancel" without the name are now **typed**. Sending needs the name because a sent
message cannot be taken back.

## Removed

Ctrl+Alt+D, Ctrl+Alt+Enter, Ctrl+Alt+L, the listening session, and the switches `holdToTalk`, `holdToCommand`,
`listenSession`. No new switch: all of this is always on.

## How it works (`typing.rs`)

* A text box is: an edit field that is not read-only; a document or container that says its value can be changed; or
  the page of Notepad, Word, WordPad, OneNote or LibreOffice. A web page itself is not.
* Hands-free typing opens the next recording within about 20 ms of the previous one ending, while the previous sentence
  is still being transcribed; sentences are typed in the order spoken. A sentence ends after 0.7 s of silence and is
  cut at 30 s.
* It types only into the window the first sentence went into; a switch ends typing. Send is refused in another window,
  with nothing typed, or in a protected, elevated or password target.
* It ends with "done", send, Ctrl+Space, 30 quiet seconds, a meeting or Ghost mode. NEXUS's own voice is never
  recorded. Ghost mode keeps its old dictation.

## Checks

* Rust: 3,682 library tests and 21 integration tests pass on the final code. Frontend: 1,296 tests and type check pass. Gates and dead-code ratchet pass; `mock-wake` builds.
* Isolated app (typed turns, no microphone; built before the spoken notice after 30 quiet seconds was added): "Nexus, type for me." started hands-free typing, the recording opened
  after the announcement, it ended after 30 quiet seconds; "stop typing" ended a second run.

## Not run, and limits

* **No voice and no real key press.** Nothing has been typed into a real window. The text-box detection has never been
  run against a real app (WhatsApp, Gmail, Slack, browser forms, Word).
* If NEXUS wrongly sees no text box, a hold runs as a **command**, and hands-free typing says it cannot see one.
* A hold whose text box check takes over 0.5 s counts as a command.
* "I pressed send" is not confirmed by looking at the message box.
* Hands-free typing finds no box by itself: click into it first. Choosing the message box is phase T4.
* Words spoken while NEXUS itself is speaking are not recorded. Audio between two recordings (under about 0.1 s) can
  be lost; not measured.
* Without Voice Lock, anyone's speech (or the TV) is typed.
* "type for me" said while holding Ctrl+Space in a text box is typed, not obeyed.
