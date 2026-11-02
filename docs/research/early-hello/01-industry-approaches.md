# Early hello: how industry gives an instant response while the heavy part starts

Date: 2026-11-02. Research only, nothing built. Companion to `docs/features/158-early-hello-and-session-reopen-plan.md`.

Every claim is marked **[S]** (sourced, URL given) or **[I]** (inferred by me, not found in a source). Where I could not find a source (Tesla, Rabbit, Humane, smart-TV boot behaviour), I say so instead of guessing numbers.

## 0. The problem in one line

NEXUS reaches `main()` about 0.1 s after process start but needs 5 s (warm) to 26 s (logon) before its first sound (doc 158, measured). The owner wants a greeting at once, then in the evening the question "Shall I reopen the apps you were using?", and the yes/no answer must never be lost, even if it is spoken before speech recognition is ready.

## 1. What the industry does: a tiny front stage and a heavy back stage

The pattern is the same everywhere: a very small piece that is always ready (or ready first) handles the first fraction of a second. It keeps the raw audio in a ring buffer, and the heavy piece later processes that buffered audio. No serious product throws away audio it heard before the heavy stage was ready.

### 1.1 Windows voice activation (Cortana era, still the documented driver model)
- A hardware keyword spotter (HW KWS) runs on the audio DSP while the CPU sleeps. Windows also ships a software keyword spotter for when no hardware one exists. [S] https://learn.microsoft.com/windows-hardware/drivers/audio/voice-activation (updated 2025-09-18)
- **Burst buffer:** a circular buffer that "bursts up" on detection "so that all audio that triggered a KWS detection is included". While armed, the hardware keeps capturing into a FIFO, "typically hundreds of milliseconds to several seconds", and keeps capturing while waiting for the driver, "ensuring no data after the keyword is lost". [S] same page
- Every solution must keep the whole keyword **plus 250 ms before it**, with start/end timestamps. [S] same page
- Keyword-detector pins should keep **at least 5000 ms** of internal burst buffer. [S] same page
- The buffered audio is streamed to the OS **faster than real time**, with true (past) timestamps. [S] same page
- "Chained command" ("Hey Cortana, call John") is a first-class case: the assistant starts *if not already started* and then follows the command from the buffered audio. This is exactly NEXUS's case: speech arriving before the heavy part is up. [S] same page
- Modern Standby wake-on-voice is certified with **system resume latency of at most 1 second** (HLK test). [S] same page
- Agent Activation Runtime (AAR) launches the assistant app **in the background**, hands it the audio from just before the keyword, and the app verifies with a stronger detector before interrupting the user. [S] https://learn.microsoft.com/en-us/azure/ai-services/speech-service/windows-voice-assistants-overview
- Users must enable voice activation system-wide and per app; Microsoft Q&A says only UWP apps can use the AAR path. [S] same overview + https://learn.microsoft.com/en-us/answers/questions/2154674/voice-activation-assistant-questions-on-windows. So NEXUS (Win32/Tauri) cannot use the OS front stage; it has to build its own. [I]

### 1.2 Apple "Hey Siri"
- Two passes: a small DNN (5 layers × 32 units) on the low-power Always On Processor, a bigger one (5 × 192) on the main CPU once the first one fires; a server check can still cancel ("Hey Seriously"). [S] https://machinelearning.apple.com/research/hey-siri (2017-10-01)
- The first pass uses "a small proportion" of the AOP; on Watch about 5 % of compute and only after a wrist raise (a presence gate). [S] same page
- A second, lower threshold puts the detector into a more sensitive state "for a few seconds" after a near miss. [S] same page

### 1.3 Amazon Alexa (AVS devices)
- The on-device engine detects, the cloud verifies; on rejection the cloud sends `StopCapture`. [S] https://developer.amazon.com/docs/alexa-voice-service/enable-cloud-based-wake-word-verification.html
- The stream must include **exactly 500 ms of pre-roll** before the wake word (8,000 samples at 16 kHz), held in a shared ring buffer; the wake-word start index must be within 50 ms. [S] https://developer.amazon.com/docs/alexa-voice-service/streaming-requirements-for-cloud-based-wake-word-verification.html ; implementation example: https://www.xmos.com/documentation/XM-014600-PC/html/doc/wakeword/WW_integration.html

### 1.4 Google Assistant "Quick Phrases"
- On Pixel (and Pixel Buds Pro), a few words work **without** "Hey Google": "stop"/"snooze" for alarms and timers, "answer"/"decline"/"silence" for calls. That is a tiny, context-limited, on-device vocabulary that is armed only while a specific question is "open" (a ringing phone, a sounding alarm). [S] https://www.androidcentral.com/you-can-now-use-google-assistant-without-hey-google-hotword ; https://9to5google.com/2023/11/23/pixel-buds-quick-phrases/
- Google warns that the call feature can answer by accident (e.g. you say "answer" while the phone is on silent). [S] https://9to5google.com/2023/09/10/pixel-assistant-quick-phrases/
- This is the closest commercial precedent to NEXUS's startup yes/no: a small vocabulary that is listened for only during a short, explicit window. [I]

### 1.5 Bixby, Copilot key, Tesla/automotive, smart TVs, Rabbit, Humane
- I did not find primary sources with startup or time-to-ready numbers for Bixby, the Windows Copilot key, Tesla voice at ignition, smart-TV boot chimes, Rabbit R1 or Humane AI Pin. I am not quoting numbers for them.
- General pattern I would expect (not verified): automotive head units show a logo/chime from the boot loader long before the voice stack is ready; smart speakers play an earcon or a "one moment" line while the cloud session is set up. [I]

## 2. Operating-system and app-startup techniques

| Technique | What it does | Source |
|---|---|---|
| Windows Fast Startup | On shut down, user sessions are logged off and the **kernel session is hibernated**, so the next boot resumes it; it does not apply to Restart. User apps still start cold. | [S] https://learn.microsoft.com/en-us/troubleshoot/windows-client/setup-upgrade-and-drivers/fast-startup-causes-system-hibernation-shutdown-fail ; https://learn.microsoft.com/pt-pt/archive/blogs/b8/delivering-fast-boot-times-in-windows-8 |
| Modern Standby | Processes keep running at low power; resume with voice is certified at ≤ 1 s. On resume NEXUS is already running, so the instant hello is easy there. | [S] voice-activation page above; [I] for NEXUS |
| Windows startup-app delay | Run-key / Startup-folder items start after a delay (default about 10 s, `Explorer\Serialize\StartupDelayInMSec`). Community sources only. | [S, community] https://www.thewindowsclub.com/disable-windows-10-startup-delay-startupdelayinmsec ; https://www.elevenforum.com/t/enable-or-disable-delay-of-running-startup-apps-in-windows-11.9144/post-199015 |
| Scheduled task at logon | Not subject to that Explorer delay (NEXUS already uses a logon task, AGENTS.md). | [I] |
| Android splash + TTID/TTFD | Two metrics: time to first frame (TTID) and time until usable (TTFD). Play vitals flag cold start ≥ 5 s, warm ≥ 2 s, hot ≥ 1.5 s. Fix = lazy init, show placeholder first, load the rest on other threads. | [S] https://developer.android.com/topic/performance/vitals/launch-time ; https://developer.android.com/google/play/vitals/launch-time |
| iOS prewarming (iOS 15+) | The system may run an app's early launch before the user taps it; poorly documented, Keychain/UserDefaults may be unavailable during it. | [S, forum] https://developer.apple.com/forums/thread/706782 ; https://openradar.appspot.com/FB9780579 |
| Android zygote, app snapshots | Pre-forked runtime / saved state so apps do not start from scratch. | [I] (well known, not re-sourced here) |

**Perception budget (Nielsen, 1993, updated 2014)** [S] https://www.nngroup.com/articles/response-times-3-important-limits/
- 0.1 s: feels instantaneous.
- 1 s: flow of thought not interrupted.
- 10 s: limit for keeping attention; beyond it give feedback.

Applied: a hello at 1-3 s after the desktop appears is in the "noticed but fine" band; 26 s is well beyond the 10 s attention limit. [I]

The Android TTID/TTFD split maps directly onto NEXUS: "early hello" is TTID (something the user notices), "assistant ready" is TTFD. [I]

## 3. Hearing a yes/no before the full recogniser is ready

### Options
1. **Buffer then transcribe later.** Open the microphone into a RAM ring buffer as soon as the question is asked; when STT is ready, feed it the buffered audio (faster than real time). This is exactly the Windows burst buffer (≥ 5 s, plus 250 ms pre-keyword) and the Alexa pre-roll (500 ms). [S] for the pattern; [I] for applying it to an answer instead of a keyword.
2. **Tiny dedicated yes/no keyword model.** Keyword spotting of "yes"/"no" is a standard benchmark task: Google Speech Commands (Warden 2018) is built for detecting a small set of target words with few false positives, with 12 classes in the TF version and 1-second clips; "yes" and "no" are among the core words. [S] https://arxiv.org/abs/1804.03209 ; https://www.tensorflow.org/datasets/catalog/speech_commands (that yes/no are in the set: [I] from the paper's word list, the TFDS page does not list the words).
3. **Grammar-constrained recogniser.** Restrict the recogniser to a short list. Microsoft's own speech guidance: frame questions so the answer is one of a few options, "a small grammar is much easier to author and results in much more accurate recognition", prefer phrases of two or more syllables, avoid look-alike phrases. [S] https://learn.microsoft.com/en-us/windows/uwp/ui-input/speech-interactions ; Vosk grammar mode trades vocabulary for accuracy and speed [S] https://deepwiki.com/Bear-03/vosk-rs/5.2-grammar-based-recognition ; Picovoice Rhino does on-device speech-to-intent [S, vendor] https://picovoice.ai/products/voice/speech-to-intent/
4. **IVR pattern (VoiceXML).** The `boolean` built-in grammar maps yes/no to true/false; `bargeintype="hotword"` lets the prompt keep playing until a grammar match, while `speech` stops the prompt on any sound. [S] https://docs.nuance.com/nvp-for-speech-suite/devref/nvpr_builtin_grammar_types_vxml.html ; https://learn.microsoft.com/en-us/previous-versions/office/developer/communication-server-2007/bb857781(v=office.12). Telephone systems also always offer a fallback key (DTMF) and a no-input timeout. [I]

### What is best practice and why
- Industry never relies on "the recogniser will be ready in time"; it **buffers raw audio from the moment listening matters** and processes it later (Windows, Alexa). [S]
- Small, context-armed vocabularies (Quick Phrases, VoiceXML boolean) are the standard way to get a fast, reliable answer, but they cause accidental accepts if armed too long or too widely (Google's own warning). [S]
- Safe default on silence or unclear answer = do nothing (no apps opened). This is consistent with Microsoft's "ask for confirmation when confidence is low" guidance. [S] UWP speech page; [I] for the specific rule.
- The owner's argument (people answer a yes/no question with yes/no) is consistent with the "frame the question so the answer space is tiny" guidance, but the system should still treat anything else as "not an answer" rather than forcing it into yes/no. [S]/[I]

## 4. Risks and dark patterns

| Risk | Note | Mark |
|---|---|---|
| Talking to an empty room | Logon itself proves presence (someone typed a PIN/used Hello), but an auto-logon or a reboot after updates does not. Use "a real logon happened in the last N seconds" or the existing keyboard/mouse-input check before speaking. | [I] |
| Speaking aloud in public (class, office, train) | A greeting plus "the apps you were using" reveals what was open (e.g. a project name). Keep app names out of the spoken question by default, or only say a count ("three apps"). Respect system mute and the meeting detector. | [I] |
| Volume | At logon the system volume is whatever it was last time (could be 100 %). Cap the greeting volume or follow the existing volume lease rules. | [I] |
| Accessibility | Narrator may also be speaking at logon; do not talk over it. Offer an on-screen Yes/No as well as voice. | [I] |
| Always-listening at startup | Open the microphone only for the answer window, keep the audio in RAM, discard it after the decision, never upload the pre-roll if the answer is decided locally. Windows shows the mic-in-use indicator, so users will see it. | [I] |
| Accidental "yes" | Google warns Quick Phrases can answer calls by accident. Keep the window short (about 10 s) and the action reversible (reopening apps is low-risk; never chain destructive actions to it). | [S] 9to5google above; [I] rest |
| Regulation | Voice recordings are personal data; the FTC/DOJ fined Amazon $25 million (2023) for keeping children's Alexa recordings and not honouring deletion. Lesson: do not retain answer audio. | [S] https://www.engadget.com/amazon-will-pay-25-million-to-settle-ftc-lawsuit-over-alexa-privacy-for-kids-193330840.html |

## 5. Comparison table

| Approach | Who uses it | Latency achieved (if known) | What it requires | Fit for NEXUS (Windows desktop, Rust/Tauri) |
|---|---|---|---|---|
| Hardware DSP keyword spotter + burst buffer | Windows HW-KWS, Apple AOP, Pixel DSP | Resume ≤ 1 s (Windows HLK) | OEM driver / DSP firmware | Not available to a Win32 app. Poor. [S]/[I] |
| OS agent runtime (AAR) | Cortana, Windows voice assistants | not published | UWP app + system voice activation | Poor (UWP only, deprecated Cortana path). [S] |
| Early sound from `main()` (pre-rendered clip) | Splash screens, boot chimes | NEXUS: `main()` at 0.09-0.14 s; first sound est. 1-3 s (device readiness) | Cached WAV in the user's voice, rodio thread before Tauri | Excellent. Already planned (doc 158 P0). [I] |
| Pre-roll / ring buffer, transcribe later | Alexa (500 ms), Windows (≥ 5 s) | No speech lost by design | Mic capture thread (cpal) started early, RAM ring of 10-20 s, STT fed later | Excellent. Cheap (cpal already used); closes the "lost yes" gap. [S]/[I] |
| Tiny yes/no keyword model | Speech Commands-style KWS, Quick Phrases | KWS models run in tens of ms per frame [I] | Small ONNX head; ideally reuse the openWakeWord mel+embedding front end | Good as a fast path; needs training data and false-accept testing. [I] |
| Grammar-constrained recogniser | IVR (VoiceXML boolean), Windows SRGS, Vosk, Rhino | not measured for yes/no | Extra engine (Vosk model / Rhino licence) or Windows SAPI | Medium. Windows SAPI/WinRT grammars exist but the WinRT path is packaged-app oriented; Vosk adds a model download. [I] |
| Delay the question until the assistant is ready | Most apps (TTFD) | = app ready time (5-25 s) | nothing new | Safe but the question comes late; current doc 158 plan step 4. [I] |
| Cloud check of the buffered answer | Alexa cloud verification | network round-trip | Groq STT key + network | Good once network is up; fallback local. [S]/[I] |

## 6. Ranked recommendation

1. **Early hello from `main()` (doc 158 P0).** Pre-rendered clip in the chosen voice, played by a thread before Tauri starts, gated on presence and mute/meeting state, capped volume. Matches splash-screen / boot-chime practice and Nielsen's 1 s band. [I]
2. **Ask the question early, but start a RAM microphone ring buffer the moment it is asked** (the Windows burst-buffer / Alexa pre-roll idea). Capture with cpal from a thread in `main()`, keep about 15 s, nothing written to disk. When STT is ready, transcribe the buffered window first (faster than real time), then classify with the existing strict yes/no classifier (`offer::classify_reopen`). The answer is never lost even if it came at second 2 and STT was ready at second 20. Silence/unclear = do nothing. This directly satisfies the owner's requirement. [S] pattern, [I] application.
3. **Optional fast path: a tiny yes/no head on the openWakeWord front end** that is loaded early, so a clear "yes"/"no" can be acted on in under a second without waiting for STT; STT on the buffer remains the authority when the head is unsure. Build only after measuring how often step 2 is slow. [I]
4. **Keep the "ask only when ready" fallback** (doc 158 step 4) for cases where the microphone could not be opened early (Bluetooth headset not connected yet, mic busy). [I]
5. **Do not** depend on the Windows OS voice-activation stack (UWP/HW-KWS only) or a grammar engine add-on for this one question. [S]/[I]

Privacy rules for 2-3: buffer in RAM only, open only for the answer window, show the orb/caption as soon as the stage exists, discard audio after the decision, keep app names out of the spoken question when other people may be around (or make that a setting).

## Sources
- Microsoft, Voice Activation (driver docs, updated 2025-09-18): https://learn.microsoft.com/windows-hardware/drivers/audio/voice-activation
- Microsoft, Voice assistants on Windows (AAR): https://learn.microsoft.com/en-us/azure/ai-services/speech-service/windows-voice-assistants-overview
- Microsoft Q&A, voice activation UWP-only: https://learn.microsoft.com/en-us/answers/questions/2154674/voice-activation-assistant-questions-on-windows
- Microsoft, Speech interactions (UWP guidance): https://learn.microsoft.com/en-us/windows/uwp/ui-input/speech-interactions
- Microsoft, Fast Startup: https://learn.microsoft.com/en-us/troubleshoot/windows-client/setup-upgrade-and-drivers/fast-startup-causes-system-hibernation-shutdown-fail ; https://learn.microsoft.com/pt-pt/archive/blogs/b8/delivering-fast-boot-times-in-windows-8
- Apple ML Research, Hey Siri (2017-10-01): https://machinelearning.apple.com/research/hey-siri
- Amazon AVS cloud wake-word verification and 500 ms pre-roll: https://developer.amazon.com/docs/alexa-voice-service/enable-cloud-based-wake-word-verification.html ; https://developer.amazon.com/docs/alexa-voice-service/streaming-requirements-for-cloud-based-wake-word-verification.html ; https://www.xmos.com/documentation/XM-014600-PC/html/doc/wakeword/WW_integration.html
- Google Quick Phrases: https://www.androidcentral.com/you-can-now-use-google-assistant-without-hey-google-hotword ; https://9to5google.com/2023/09/10/pixel-assistant-quick-phrases/ ; https://9to5google.com/2023/11/23/pixel-buds-quick-phrases/
- Android startup: https://developer.android.com/topic/performance/vitals/launch-time ; https://developer.android.com/google/play/vitals/launch-time
- iOS prewarming (forum): https://developer.apple.com/forums/thread/706782 ; https://openradar.appspot.com/FB9780579
- Nielsen, response time limits: https://www.nngroup.com/articles/response-times-3-important-limits/
- Startup delay (community): https://www.thewindowsclub.com/disable-windows-10-startup-delay-startupdelayinmsec ; https://www.elevenforum.com/t/enable-or-disable-delay-of-running-startup-apps-in-windows-11.9144/post-199015
- Speech Commands (Warden 2018): https://arxiv.org/abs/1804.03209 ; https://www.tensorflow.org/datasets/catalog/speech_commands
- VoiceXML boolean grammar and barge-in: https://docs.nuance.com/nvp-for-speech-suite/devref/nvpr_builtin_grammar_types_vxml.html ; https://learn.microsoft.com/en-us/previous-versions/office/developer/communication-server-2007/bb857781(v=office.12)
- Vosk grammar mode: https://deepwiki.com/Bear-03/vosk-rs/5.2-grammar-based-recognition ; Picovoice Rhino (vendor): https://picovoice.ai/products/voice/speech-to-intent/
- FTC/Amazon Alexa settlement (2023): https://www.engadget.com/amazon-will-pay-25-million-to-settle-ftc-lawsuit-over-alexa-privacy-for-kids-193330840.html
