# Comparable assistants: who owns acknowledgement, loading state, and listening-indicator visibility

Research date: 2026-10-29. General web research, not about NEXUS code. Anything not verified from a fetched primary page is marked as such.

## 1. How do commercial assistants and design guidance treat acknowledgements and long-running tasks, and what latency thresholds apply?

### Takeaway
Nielsen's 0.1 s / 1 s / 10 s limits are the standard thresholds: no feedback under 0.1 s, a "working" signal past 1 s, and explicit progress with a way to interrupt past about 10 s. Alexa's progressive response is the only commercial voice mechanism I could verify in detail. It is skill-initiated, capped at 5, and does not extend the roughly 8 s response window. I could not verify any Google, Siri, or Microsoft rule that gives a numeric threshold for voice acknowledgements.

### Cited Findings
- Nielsen's three limits: 0.1 s feels instantaneous and needs no feedback beyond the result; 1.0 s keeps the user's flow of thought uninterrupted, though the delay is noticed; 10 s is the limit for keeping attention on the task, and longer delays need feedback — [NN/g, Response Times: The 3 Important Limits](https://www.nngroup.com/articles/response-times-3-important-limits/)
- Over about 10 s, NN/g says to use a percent-done indicator (preferably a graphic bar) and give a clear way to interrupt. For unknown total work, show running progress in absolute terms, such as naming each item as it is processed. For 2-10 s a full percent-done indicator may be excessive, and a less conspicuous busy cue is enough. A spinner is a last resort when progress cannot be measured. — [NN/g](https://www.nngroup.com/articles/response-times-3-important-limits/)
- The numbers are a 1993 synthesis by Nielsen, not a primary experimental finding, and NN/g says the exact maximum varies by context — [NN/g video page](https://www.nngroup.com/videos/3-response-time-limits-interaction-design/); critical view in [jmduke.com](https://www.jmduke.com/posts/response-times.md)
- Alexa progressive response is for when a skill needs time to prepare the answer. Uses include a short confirmation that the request is being processed, a short soundmark, or other engaging content, to reduce perceived latency. — [Alexa: send a progressive response](https://developer.amazon.com/docs/custom-skills/send-the-user-a-progressive-response.html)
- Alexa constraints: up to five progressive responses before the full response, none after it. They do not extend the window, so the skill has about 8 s for the full response including progressive ones. They can only be sent during LaunchRequest or IntentRequest. A progressive response only plays if it arrives before Alexa receives the full response. Content must be SSML; embedded MP3 audio is capped at 30 s. — [Alexa docs](https://developer.amazon.com/docs/custom-skills/send-the-user-a-progressive-response.html)
- API detail: `VoicePlayer.Speak` is the only directive type, and the speech text is capped at 600 characters. — [Progressive Response REST API reference](https://developer.amazon.com/en-GB/docs/alexa/custom-skills/progressive-response-api-reference.html)
- Alexa's APL latency guidance is about rendering visuals fast (vector graphics rather than images, update via APL commands instead of round trips to the skill). It is not about wait indicators. — [APL: Reduce Latency](https://developer.amazon.com/en-US/docs/alexa/alexa-presentation-language/reduce-latency-for-your-apl-documents.html)
- Apple HIG on progress indicators: use determinate indicators when the duration is known and indeterminate ones when it is not. People read a stationary indicator as a frozen app, so keep it moving. A fast start followed by a very slow finish makes people doubt it is still working. Vague labels like "loading" add little. — [Apple HIG progress indicators (mirror)](https://developer-mdn.apple.com/design/human-interface-guidelines/components/status/progress-indicators)
- Google's conversation design guidance on earcons says they add cognitive load because users must learn their meaning. Use few, distinguishable, consistent, brief sounds. — [Google conversation design: earcons](https://developers.google.com/assistant/conversation-design/earcons)
- Google Conversational Actions were discontinued on 13 June 2023, so that documentation is archival. — [developers.google.cn notice (Japanese)](https://developers.google.cn/assistant/sdk/guides/service/python/best-practices/audio?hl=ja)

### Inferences
- A defensible mapping for a voice UI: under about 0.1 s nothing extra; about 1 s a non-verbal "working" cue; about 2-8 s a short spoken ack, as Alexa's progressive response does; beyond about 10 s a persistent visible task with a way to cancel, and a spoken hand-off ("I'll tell you when it's done"). These are my synthesis of NN/g and Alexa, not a published voice-specific rule.
- Alexa's "at most 5, only before the final response, and dropped if late" shows the ack is treated as best-effort and subordinate to the final answer.

### Gaps
- No Siri-specific guidance found. No Microsoft (Cortana or Copilot voice) guidance found. No current Google (Gemini or Home) guidance on loading prompts or long-running tasks found.
- No official Alexa design-guide rule giving a numeric latency threshold for an ack. Only the roughly 8 s timeout is stated. The Alexa on-screen "working" or thinking visual states were not verified.

## 2. How do open-source assistants structure it: one state machine that emits events the UI renders, or do skills drive the UI?

### Takeaway
The more recent frameworks (Home Assistant Assist, LiveKit Agents, Pipecat) have one pipeline or agent that owns state and emits typed events or frames; UIs and LEDs render those. OVOS and Mycroft are mixed: the core emits bus events for listening state, but skills can also post to the GUI and enclosure directly.

### Cited Findings
- Home Assistant Assist pipeline events: run-start, wake_word-start/end, stt-start, stt-vad-start/end, stt-end, intent-start, intent-progress (some agents; can carry a `tts_start_streaming` signal), intent-end, tts-start, tts-end, run-end, and error. On a timeout, run-end arrives first and then an error with code `timeout`, so clients must keep listening after run-end. — [HA contributors: Assist pipelines](https://contributors.home-assistant.io/core/internals/assist-pipelines/)
- HA Assist satellite entity has four states: IDLE, LISTENING, PROCESSING, RESPONDING. The state is updated automatically when a pipeline runs, except RESPONDING to IDLE: the device must call `tts_response_finished` when it has finished speaking. — [HA developers: Assist satellite entity](https://developers.home-assistant.io/docs/core/entity/assist-satellite)
- HA "became idle" fires only after the whole run completes, including playback of the reply. Docs examples suggest using the state triggers to switch indicator lights on and off. — [HA assist_satellite idle trigger](https://www.home-assistant.io/triggers/assist_satellite.idle/)
- LiveKit Agents states: initializing, idle, listening, thinking, speaking, plus client-side connecting, pre-connect-buffering, disconnected, failed. The agent publishes state as the `lk.agent.state` participant attribute, and the SDK emits an `agent_state_changed` event. Frontends should use boolean getters (`canListen`, `isPending`, `isFinished`) instead of raw states. — [LiveKit: Agent state](https://docs.livekit.io/frontends/build/agent-state/)
- LiveKit `thinking` covers both processing input and performing actions (tool work). There is no separate acknowledgement state. Details such as the active tool need custom state sync or RPC. — [LiveKit: Agent state](https://docs.livekit.io/frontends/build/agent-state/)
- Pipecat is frame-based: processors in a pipeline pass frames, and SystemFrames get priority and are not cancelled on interruption. — [Pipecat pipeline docs](https://docs.pipecat.ai/learn/pipeline)
- Pipecat frames relevant to UI: UserStartedSpeakingFrame / UserStoppedSpeakingFrame, BotStartedSpeakingFrame / BotStoppedSpeakingFrame (emitted by the output transport, broadcast in both directions), BotSpeakingFrame (continuous), FunctionCallsStartedFrame, and ExternalFunctionCallInProgressFrame / Result / Cancel. RTVIObserver relays these to the client, and RTVIServerMessageFrame carries custom UI messages. — [Pipecat system frames](https://docs.pipecat.ai/server/frames/system-frames)
- OVOS dinkum listener (one service) emits `recognizer_loop:wakeword`, `recognizer_loop:record_begin`, and `recognizer_loop:record_end`. — [OVOS message spec (dinkum)](https://openvoiceos.github.io/message_spec/dinkum)
- OVOS GUI service keeps track of what should be rendered but does not render it. State is organised in namespaces, usually one per skill_id, each with pages. So skills write GUI state; a service holds it; a client renders it. — [OVOS technical manual: GUI service](https://openvoiceos.github.io/ovos-technical-manual/104-gui_service)
- The legacy Mycroft message-types reference lists `mycroft.skill.handler.start` and `mycroft.skill.handler.complete` under skill core events. — [Mycroft message types](https://mycroft-ai.gitbook.io/docs/mycroft-technologies/mycroft-core/message-types)

### Inferences
- Pattern A (HA, LiveKit, Pipecat): a single pipeline or agent owns the lifecycle and publishes it; displays subscribe and render. Pattern B (OVOS/Mycroft): a shared bus plus per-skill GUI namespaces, so UI authority is distributed.
- HA and LiveKit both collapse everything between "heard" and "speaking" into one state (PROCESSING, thinking). None of them has a first-class ack state. The ack is just TTS output inside the same turn, and rich progress is custom.
- HA's manual RESPONDING-to-IDLE handshake exists because only the device knows when audio playback has finished, so the central machine cannot decide this alone.

### Gaps
- I did not verify the Mycroft/OVOS handler start/complete emitters, `enclosure.*` behaviour, or whether skills in practice drive the display directly. The OVOS fetches did not cover them.
- I did not fetch Rhasspy 3 / Wyoming docs, or Vapi/Retell agent-state docs. No findings on them.
- I did not verify how the HA frontend or satellite firmware (for example ESPHome LED handling) consumes events.

## 3. Known failure modes when the indicator disappears or conflicts with an ack or loading state, and fixes

### Takeaway
I found no authoritative write-up of this specific failure mode. The sources do show the structural cause and the usual fix: lifecycle events are emitted from a single owner, and the end state (idle) waits for a handshake from whoever actually knows playback has finished.

### Cited Findings
- HA exempts RESPONDING to IDLE from its automatic state updates and requires the device to call `tts_response_finished`, so the indicator cannot go idle while audio is still playing. — [HA Assist satellite entity](https://developers.home-assistant.io/docs/core/entity/assist-satellite)
- HA documents that after a pipeline timeout, run-end arrives before the error event, so a UI that hides on run-end can miss the error. — [HA Assist pipelines](https://contributors.home-assistant.io/core/internals/assist-pipelines/)
- LiveKit recommends deriving UI from boolean getters rather than raw states, so UI logic does not break when states are added. — [LiveKit: Agent state](https://docs.livekit.io/frontends/build/agent-state/)
- A forum user reports that with streamed LLM output, HA's tts-start only fired after intent-end, despite a streaming threshold being reached. Single user report; no official statement. — [HA community thread](https://community.home-assistant.io/t/tts-streaming-of-llm-chat-messages-activated-but-not-working/1005088)
- Pipecat broadcasts BotStartedSpeaking and BotStopped Speaking from the output transport (the component that actually plays audio), not from the LLM stage. — [Pipecat system frames](https://docs.pipecat.ai/server/frames/system-frames)

### Inferences
- The lesson common to these designs: visibility should be a pure function of lifecycle state (for example visible = listening OR thinking OR speaking OR a task is pending), computed in one place. The "end" transition should come from the component that owns the audio output, not from the component that produced the text.
- Two components independently showing and hiding the same element is the anti-pattern; none of the frameworks above lets that happen, because UI subscribes and does not command. This is my inference from their architecture, not something their docs say.
- Both the ack and the loading indicator are naturally derived from the same state. An ack is an output event; a loading indicator is a state-derived visual. Neither needs to own visibility.

### Gaps
- No primary source describing a "vanishing avatar" bug class or a general "single source of truth / event-sourced UI" recommendation for voice assistants. Treat the principle as my reasoning, not a cited consensus.

## 4. Patterns for background tasks that outlive the turn

### Takeaway
Alexa moves post-turn results out of band with a ProactiveEvents notification (yellow light or on-screen banner, read on request). The open-source frameworks I checked have no first-class background-task concept; they stay inside the turn.

### Cited Findings
- Alexa ProactiveEvents API: a skill publishes an event to Alexa, which handles subscriber lookup and delivery. A permission and declared event schemas are required. The user is notified by a yellow light on devices without screens or a banner on devices with screens, and can then ask Alexa to read it. Users can turn notifications off per skill. Events must follow predefined schemas, each with a template that Alexa reads back. — [Alexa ProactiveEvents launch](https://developer.amazon.com/en-US/blogs/alexa/alexa-skills-kit/2018/12/proactiveeventsapi-launch-announcement) and [Medium guide](https://medium.com/swlh/get-started-with-amazon-alexa-skills-proactive-events-api-5b082bcb282c)
- The alexa-samples/proactive-events-demo repository is archived (Dec 2023), so check current Amazon docs before relying on it. — [GitHub](https://www.github.com/alexa-samples/proactive-events-demo)
- LiveKit says `thinking` is the signal for tool work and gives no background-task state. Details need custom state sync or RPC. — [LiveKit: Agent state](https://docs.livekit.io/frontends/build/agent-state/)
- Pipecat exposes in-progress external function-call frames that RTVIObserver relays, so a client can render a per-call in-progress chip. — [Pipecat system frames](https://docs.pipecat.ai/server/frames/system-frames)

### Inferences
- Observed pattern: end the turn quickly with a spoken hand-off, keep a separate persistent task object (chip, card, or light) with its own lifecycle, and deliver completion as a notification that is announced only when the user is not mid-turn. Alexa's light/banner-then-ask-to-read approach is the cleanest verified example.
- This means the task's lifecycle is independent of the turn's lifecycle. A turn-scoped state machine should not own task chips.

### Gaps
- No verified source on Siri, Google Assistant, or Home Assistant handling of minutes-long tasks (for example HA timers or long conversation-agent jobs).
- No verified rule for when to interrupt with a completion announcement versus defer it.

## Overall recommendation for a designer (synthesis, marked as inference)

- Evidence favours one authoritative lifecycle/state source that emits events (HA pipeline events, LiveKit `lk.agent.state`, Pipecat frames), with UI components as pure subscribers. Skills or handlers should request an ack or progress through that source rather than show or hide UI themselves.
- Whether the "presentation/feedback manager" is a separate module is a design choice. The frameworks I read implement it as subscribers of a single event stream, not as a second owner of state. A derived presentation layer (visible = f(state, pending tasks, speech playing)) fits that evidence; a second writer of visibility does not.
- Concrete thresholds supported by sources: about 0.1 s no feedback; about 1 s a working cue; up to 5 interim acks before the final answer and an overall budget near 8 s (Alexa); past about 10 s show progress plus a way to cancel (NN/g); the end of "responding" must be signalled by whoever plays the audio (HA).
