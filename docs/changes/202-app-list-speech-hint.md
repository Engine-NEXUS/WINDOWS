# 202 — App-list hint for speech recognition (opt-in)

`sttCommandHints` (default OFF, unchanged) used to send a fixed sentence list ("Open Chrome. Open WhatsApp. …") to Groq. `scripts/prompt_ab.py`
(42 synthetic clips, 4 prompts, 168 calls) showed that list made Whisper emit command words on **6 of 6** silent/noise clips and gained nothing.
A bare list of the user's own apps fixed the misses ("anti-gravity" -> "antigravity": 25 -> 28 of 30 commands) with no new text on silence.

Now `DecoderBias::Commands` sends "Brave, Spotify, WhatsApp." built from the user's most-used launched apps (max 12 names / 180 chars, installers
excluded, cleaned to letters/digits). With no used apps it sends the neutral prompt. `stt_groq::entity_prompt_text`, `entity_decoder_prompt`;
`app_registry::pick_hint_names`/`hint_names`. App names (not paths) are sent to Groq on those turns only when the setting is on.

Not verified: real microphone audio (the test clips are clean synthetic speech, so the accuracy gain is an upper bound), the setting in the running app.
