# 5. Gaps, limits and next steps

## 5.1 Not verified live

| Item | Why not | What would verify it |
|---|---|---|
| A send through the **real** WhatsApp bridge | The bridge was not connected; all sends went to a fake bridge on 127.0.0.1:8765 that records what it receives | Connect the bridge, send a test message to yourself |
| Flow B confirming and opening Maps on a **real** mail | None of your recent mails contained a Maps link or a `Location:` line, so the flow stopped honestly at "couldn't find a place" | Email yourself a Google Maps link, then say "check my mail and take me to the place in that email" |
| The spoken voice round trip (wake word → sentence → spoken "yes") | Tests called the same commands the voice path calls, without a microphone | Say the sentences to NEXUS |
| Ghost mode | Flows do not run inside a Ghost session (the Ghost path returns before the flow hook) | Design decision needed |
| The 10-minute expiry in real time | Covered by unit tests and an injected old timestamp, not by waiting 10 minutes | — |

## 5.2 Known limits (by design for now)

- Only two flows exist. Adding one = a JSON file in `flows/builtin/` (+ capabilities if it needs new centers).
- `mail.check` reads mail the inbox watcher has already filed as important, not the whole inbox.
- Flow B uses the mail just read, then the 5 newest inbox mails of each connected account; it cannot yet be told
  "the mail from Acme".
- The final spoken "yes" is not speaker-checked (same as the existing WhatsApp confirm). The turn that *starts* a
  flow is.
- The confirmation prompt includes the top mail line, which is spoken by the cloud voice (same as the daily briefing).
- The "sir" guard rewrites Result lines, not confirmation prompts.
- The cloud chooser costs one request to your key per qualifying sentence (only sentences that mention mail plus
  WhatsApp or a place).
- No general "did you mean A or B?" question (L4) yet; the flows' signatures do not overlap, so it has not been needed.

## 5.3 Next phases

| Phase | Scope |
|---|---|
| F4 | Command Hub "Workflows" page: list flows, switch each on/off, type a test phrase and see which layer chose what and why; saved routines proposed as flows (you approve each) |
| F4b | More flows: mail deadline → calendar (with the existing overlap check), WhatsApp read → reply, PR summary → message (needs a saved group alias) |
| F5 | Adversarial corpus v2 (≥50 cases), real speech-to-text phrase capture, per-capability timeouts, a CI ratchet on the paraphrase scores so they can only rise |
| Option | L2 embeddings, only if the corpus shows misses the rules and the chooser cannot cover |
