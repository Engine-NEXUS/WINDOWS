# 212: "Type for me" and hold Ctrl+Space (2026-11-26)

Phases T0 and T1 of `docs/research/jarvis-speed/03-hands-free-typing-plan-2026-11-26.md`.

## "Type for me"

* "type for me", also with "can you / please / hey / now / sir" around it, now **starts dictation into the window in
  front**. Before, "type for me" opened the Ghostwriter draft page and "type for me please" typed "for me please".
* Ghostwriter still opens with "ghostwriter", "take a letter", "take dictation" and its other phrases. "type for me" is
  no longer one of them (the plan's recommended choice; say if you want it back).
* "stop typing please" and similar now stop dictation.

## Hold Ctrl+Space

* Tap (under 0.4 s): as before; silence ends the turn.
* Hold, talk, let go: silence is ignored while the key is down (60 s cap); letting go ends the turn. The words are then
  handled like any other turn: a command, "type ..." types, and in dictation mode they are typed.
* Pressing while NEXUS is busy cancels, as before.
* While a talk key is held the screen's "listening for too long" and "nothing heard for 8 s" timeouts are paused. This
  also fixes the same cut-off for Ctrl+Alt+Enter (added in 211): a hold over about 30 s would have been closed.
* Always on; there is no switch (a tap behaves as before).

## Checks

* Rust: 3,683 library tests and 21 integration tests pass. Frontend: 1,296 tests and the type check pass. Gates and
  dead-code ratchet pass; the `mock-wake` build compiles; the isolated build links.
* Isolated app, typed turns through the real orchestrator: "type for me please" and "Can you type for me?" started
  dictation; "stop typing" and "stop typing please" stopped it.

## Not run

* **Ctrl+Space was not pressed.** In the isolated app it did not even register ("already registered"; which program holds
  it was not established, a running NEXUS would). Tap, hold, release and cancel are unproven live; only the tap/hold
  threshold has a new test.
* No voice was used (no microphone), so nothing was typed into a window.
* Dictation still needs the wake word or a held key for **each** sentence; continuous listening is phase T2.
* A hold in which nothing is said ends like any empty turn.
