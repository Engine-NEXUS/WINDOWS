# NEXUS: brief for ElevenLabs Architect

Date: 2026-10-25. Status: prepared locally; not submitted to ElevenLabs. No Architect opinion has been received. ElevenLabs MCP authentication is not complete. The official ElevenLabs plugin is installed and enabled, but its tools are not exposed in this active chat.

## Request

Review the voice architecture of NEXUS, a Windows desktop assistant implemented with Rust/Tauri and React. Give a prioritized table of weaknesses, proposed changes, tradeoffs, tests, and measurable release criteria. Distinguish documented capabilities from hypotheses requiring experiments. Advice only; do not create, update, publish, or purchase resources.

## Owner constraints

- Windows 1.0 first. One feature at a time; speaker verification is the active work card.
- Fast conversational response; cloud Edge TTS online, local Kokoro offline. Smart standby is intended to warm local components when connectivity is unstable, rather than keep everything resident.
- Preserve a free/open-source path and user-supplied cloud credentials. If recommending a paid ElevenLabs component, show the free alternative and explain what the paid component would improve. Do not assume a provider migration.
- Voice Lock is off by default until acceptance proof passes. When enabled, unavailable or uncertain verification must not permit executable actions. Keyboard/settings recovery must remain available.
- No production-ready claim without real-device evidence, complete exact-source automated checks, and an independent cross-check of logs and behavior.

## Current implementation and evidence

- openWakeWord wake detection; Groq transcription with a local fallback; deterministic command routing and optional model routing; Edge TTS with offline Kokoro.
- Voice enrollment uses ten clips: five profile clips and five held-out owner clips for provisional threshold calibration. Both halves include Nexus / Hey Nexus / OK Nexus.
- Enrollment pauses native wake/capture for the whole microphone session, stops browser recording before native resume, and handles navigation/cancellation. Profiles are encrypted locally and the lock requires a valid profile.
- Verification currently reuses the OWW feature pipeline. Please assess whether wake-word embeddings are suitable for speaker identity at all, including on arbitrary command speech. We have not established that suitability with independent non-owner data.
- Frozen source 50dcda219018d3bbfff8be388b77610bee135749 passed 2,524 library + 19 integration tests, 656 frontend tests, 145 Worker tests, 48 Python tests, NLU/repository guards, and 15 safety mutations. These passes apply to that source only.
- Live diagnostic build 3d1932b68b2c0f15 differs from the frozen candidate. Its results must not inherit the frozen suite proof.
- An owner profile in the diagnostic build has provisional threshold 0.92640746. One owner command was verified at 0.944 but transcribed incorrectly. A later ten-attempt owner batch yielded seven identifiable completed captures, all verified; three attempts remain unmatched. We cannot claim 10/10 or a general acceptance rate.
- Command transcripts included duplicated verbs/targets, one repeat-loop filter rejection, and one unrelated agent-hook command during the instructed Notepad batch. The owner-command decoder vocabulary contains complete command examples; prompt-induced hallucination is a hypothesis, not an established cause.
- Owner reports enrollment isolation and wake/cancellation recovery passed, but build switching prevents exact assessed-source attribution. Repeat proof is required.
- The owner is alone. No measured non-owner or TV/playback acceptance corpus exists. Own-voice changes are not a substitute for independent speakers.
- Settings UI uses draft fields plus Save. Owner reported Saved, but two native readbacks still returned false; a subsequent authorized native settings_patch enabled the lock and both readbacks returned true. The UI/save discrepancy remains unresolved.

## Questions

1. What speaker-verification architecture would you use for a desktop action gate? Evaluate dedicated speaker embeddings versus the current wake-word feature reuse. Address short wake phrases, arbitrary command speech, noise, distance, enrollment size, and uncertainty.
2. How should we separate speaker identity, speech recognition, replay resistance, and permission to execute? What must never be delegated to STT or an LLM?
3. How should thresholds be calibrated with independent owner/impostor recordings, without tuning on acceptance data? Specify FAR/FRR reporting and limitations of zero observed errors.
4. What useful tests can a solo developer run? Distinguish synthetic voices, TV/recorded speakers, owner replay, and live unfamiliar speakers; identify which claims each can support.
5. How would you diagnose duplicate speech, complete-command prompt leakage, missing captures, stale audio, and TTS echo without weakening hallucination or authorization guards?
6. What per-capture identifiers, stage timestamps, rejection reasons, and privacy-preserving logs are needed to reconcile every attempt to a result?
7. How would you make Voice Lock's effective native state clear in the UI, including unsaved changes, save failures, corrupt profiles, and recovery?
8. What latency, interruption, endpointing, microphone ownership, and cloud/offline fallback design changes would you prioritize? Separate inference latency from measured end-of-speech-to-audio latency.
9. Which ElevenLabs APIs or tools could help with evaluation or voice interaction, and which do not provide speaker authentication? Link official documentation; disclose costs and limits rather than estimating them from memory.

## Acceptance contract to critique

At least 95% owner acceptance; zero observed executable non-owner actions across at least 300 negative trials, with sample size, conditions, repeated-recording dependence, and statistical uncertainty reported. Playback tests must be labeled separately from live non-owner tests. Missing/corrupt profile, model failure, deletion, restart, confirmation handoff, and keyboard recovery require separate proof. No next feature until this work card meets its release gates.

## Desired response

Return (1) a concise architecture critique, (2) a highest-risk-first action table, (3) a solo testing protocol, (4) a non-owner/replay protocol, and (5) explicit unknowns and evidence needed. Do not treat the reported failures as diagnosed root causes.

## Connection reference

Official external-assistant documentation: https://elevenlabs.io/docs/eleven-agents/operate/architect/external-assistants

Documented hosted MCP endpoint: https://api.elevenlabs.io/v1/mcp (OAuth). The documented MCP server exposes agent-management tools; it should not be described as a direct Architect consultation API without discovering such a tool.


## Connection attempt outcome

- Added official hosted MCP endpoint; automatic OAuth client registration failed because dynamic registration is unsupported.
- Retried using the public client ID from ElevenLabs' official plugin. The signed-in authorization page returned Invalid redirect_uri for the CLI loopback callback; no authenticated connection was established.
- Installed official marketplace elevenlabs/plugin and plugin elevenlabs@elevenlabs version 0.4.0. Installation reports enabled=true. Removed only the standalone MCP configuration created for the failed attempt, to avoid a duplicate connection.
- Current chat does not expose ElevenLabs MCP tools. Reloading the host and completing the installed plugin's Connect flow remains necessary.
- Browser fallback reached the signed-in Architect composer. Entering the prompt repeatedly timed out; no Start conversation action was taken, so no request submission or Architect opinion is confirmed. A partial draft may remain in the browser.
- No ElevenLabs agents, deployments, subscriptions, or account permissions were changed. No OAuth token or API key was copied or printed.
