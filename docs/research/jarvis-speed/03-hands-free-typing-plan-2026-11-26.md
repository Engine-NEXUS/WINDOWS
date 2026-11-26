# Hands-free typing: "Nexus, type for me" and hold Ctrl+Space (analysis and plan, 2026-11-26)

Plan only. No source code changed. Labels: **tested** = run today, **read** = read in the code, **proposed** = new.

## What you asked for

1. Hold Ctrl+Space and keep talking until you let go.
2. Hands free: "Nexus, type for me", then NEXUS finds the right box and types what you say.
3. NEXUS must know whether you are dictating or giving an order: "done" or "send" must act, not land in the message.

## What NEXUS does today

| Piece | State | How known |
|---|---|---|
| "Nexus, type for me" | Opens **Ghostwriter**, a draft page in the sidebar. It does not type into your app | tested (parser), read (`ghostwriter.rs`) |
| "type for me please" | Types the words "for me please" (a bug) | tested (parser) |
| "start typing" / "type whatever I say" | Types each sentence into the front window; ends on "stop typing", "done typing", "stop", "cancel", or 30 s silence | read (`orchestrator.rs` dictation stream, `dictation.rs`) |
| Saying "send" during that mode | **Typed as text**; nothing is sent | read (the dictation stream takes everything except stop phrases) |
| Saying only "stop" or "cancel" as message text | Ends dictation, the word is lost | tested (`is_stop_phrase`) |
| Ghostwriter "send it" | Sends the draft over the WhatsApp bridge only | read |
| Microphone between sentences | I found no code that re-opens it for dictation or Ghostwriter (only Ghost mode, the new listening session and follow-up questions re-open it), so each sentence seems to need the wake word | read, not run |
| Safety when typing | Refuses password, protected and administrator windows and NEXUS itself | read (`dictation.rs::refusal`) |
| Choosing the box | Types wherever the cursor already is; it never looks for a box | read |
| Hold keys | Added today on **other** keys (Ctrl+Alt+D to type, Ctrl+Alt+Enter for a command); Ctrl+Space is still press to call or cancel | read |

## 1. Hold Ctrl+Space (proposed; small)

Yes, it can do both without breaking the current key:

| What you do | Result |
|---|---|
| Tap (under 0.4 s) | as today: calls NEXUS; silence ends the turn |
| Hold, talk, let go | NEXUS listens as long as you hold (up to 60 s); letting go ends the turn |
| Press while NEXUS is busy | as today: cancel |

While the key is down, silence is ignored; on release after a long hold the turn ends at once. What you said is a command,
unless it starts with "type", which types the rest. Reuses today's held capture. Effort about 1 day.
Risk: some apps use Ctrl+Space (code completion); NEXUS already takes it, so nothing changes there.

## 2. How NEXUS knows: text or order?

| Method | How it works | Good | Bad |
|---|---|---|---|
| A. Mode | "type for me" starts typing mode; everything is text until you leave | predictable | needs a way to leave |
| B. Alone after a pause | "done" or "send it" counts only if it is the **whole** utterance | natural; "send it to me tomorrow" stays text | a message that really is just "done" is taken as an order |
| C. Name first | "Nexus, send" / "Nexus, done" | almost never wrong | two more syllables |
| D. Key | holding the key = text; letting go = finished | no ambiguity | not hands free |
| E. AI guesses | a model decides for every sentence | flexible | slower, sometimes wrong; a wrong "send" cannot be undone |

**Recommendation: A, with C for sending and B for harmless orders. Not E.**

| You say (alone, after a pause) | Effect |
|---|---|
| "done" / "stop typing" / "Nexus, done" | leave typing mode; the text stays, **nothing is sent** |
| "new line", "new paragraph", "scratch that" | edit (new line and scratch already work) |
| "Nexus, send" | send (see section 4) |
| "send" / "send it" without the name | NEXUS asks "Send it?"; "yes" sends, "no" types the words |
| "type the word send" / "literally done" | types the word |
| anything else, including "send me the file" | text |

Change from today: bare "stop" and "cancel" should end typing only with the name ("Nexus, stop"), so they can be written.
Your decision (see the end): whether bare "send it" should send at once.

## 3. Finding the right box (proposed)

Order, stopping at the first that fits:

1. The box that already has the cursor, if it accepts text.
2. The box under the mouse pointer.
3. The window's single message box (chat apps: the large box at the bottom, never the search box).
4. Several possible: number them on screen; you say "two".
5. None: say so and type nothing.

NEXUS then puts the cursor there, checks it really got it, and remembers the window and box. Before **every** sentence it
checks both again; if you switched away it holds the text and says so. Password, protected and administrator boxes stay
refused. "Nearest" in the sense of "closest on screen" is only used in step 2, because the geometrically closest box is
often the wrong one. Naming works too: "type in the search box", "reply to this".

What to type: word for word, with punctuation and um/uh removed (exists). Writing it **for** you ("tell him politely
I'll be late") is a different command, "write ...", which shows a preview first. Later phase.

## 4. Sending (proposed)

| App | How |
|---|---|
| WhatsApp, Slack, Discord, Telegram | Enter in the message box |
| Gmail, Outlook | Ctrl+Enter, or the button named "Send" |
| anything else | the button named "Send" if there is exactly one; otherwise "I can't find a send button" |

Rules: only the window and box the session started with; never press twice; confirm by seeing the box empty, otherwise
say "I pressed send but couldn't confirm it"; never send an empty box; never from speech the directed-speech gate rejected;
with Voice Lock on, only your voice.

## 5. The microphone problem (must be solved first)

Today's pieces are not enough for flowing speech:

* The listening session closes the microphone while the sentence is transcribed and for 0.8 s after. Words spoken then are
  **lost**. Typing mode needs recording that continues while the previous sentence is being transcribed (a queue of
  sentences, typed in order).
* The open-microphone silence wait (0.24 s) would chop sentences into pieces with wrong full stops. Typing mode needs about
  0.7 s, and joins pieces.
* A turn is capped at 10 s; typing mode needs a cut at the next pause instead of a hard stop.
* The TV or another person gets typed too. Voice Lock is the only real defence; without it, the mode should end after
  30 s of silence (as today) and show clearly that it is listening.

## Plan

| Phase | What | Gate | Effort |
|---|---|---|---|
| T0 | Fix "type for me please"; decide what "type for me" opens | phrase tests | 0.5 day |
| T1 | Hold Ctrl+Space | tap and cancel unchanged; held 30 s sentence arrives complete | 1 day |
| T2 | Continuous recording and sentence queue | a 2-minute read-aloud with no missing or reordered words | 3-4 days |
| T3 | Order rules of section 2 | 200 test sentences: 0 wrong sends, at most 1% orders typed or text obeyed | 2 days |
| T4 | Box finding | right box in 19 of 20 tries across WhatsApp, Gmail, Slack, Notepad, a browser form | 3 days |
| T5 | Sending | 20 sends per app: 0 wrong window, 0 double sends | 2-3 days |
| T6 | "write ..." composed by AI with preview (optional) | never inserts without a yes | 3 days |

About 2-3 weeks. T0 and T1 can start now. T2 to T5 need your microphone to be called working; until then they are only
unit-tested. No percentage here is measured.

## Decisions for you

1. Bare "send it": ask "Send it?" first (recommended), or send at once?
2. "Nexus, type for me": type into the app (recommended; Ghostwriter stays under "ghostwriter"), or keep opening the
   draft page?
