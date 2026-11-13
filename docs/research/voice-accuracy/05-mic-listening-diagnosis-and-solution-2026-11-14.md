# Microphone, wake and command capture: diagnosis and solution (2026-11-14)

Evidence: today's real log (`%APPDATA%\com.nexus.assistant\logs\nexus.log`, 07:16 to 16:57 UTC on 2026-11-13, one owner, one laptop) read with the new
`scripts/voice_funnel_report.py`, plus two direct microphone tests. Nothing here is an accuracy figure for the product: the log shows what the pipeline
did, never what was said, so **missed wakes leave no trace** and a controlled trial is still needed (protocol at the end).

## What the log shows (measured)

| Stage | Result |
|---|---|
| Microphone readings | 96.5% SILENT (29,365 of 30,435), 3.5% LIVE; 1,706 "mic silent" warnings in one day |
| Neural wake stage | 45 detections, scores 0.68 to 0.97 (median 0.81); 35 more near misses peaking 0.35 to 0.68 |
| Speech check | accepted 5 of 45 (11%), rejected 38 (84%); 7 of the rejections heard nothing at all (empty transcript) |
| What fooled the detector | ordinary speech ("She's a little bit of a ricotta", "What time is this?", "The time is 7:19 pm") |
| Speaker check at wake | 45 of 45 accepted, similarity 0.932 to 0.985, including the 38 the speech check threw out |
| Command capture | 8 started, 2 delivered, 6 cancelled with no speech heard (several were test hotkey presses, so this is not a user failure rate) |
| Recogniser output | 14 hallucinations filtered of 84 outputs (17%): foreign-language noise, "Thanks.", repetition loops |

## Direct tests on this laptop

- **Quiet room, 15 s, raw capture of the same device:** median level 0.0, loudest 1e-5, peak 6e-5 (about -84 dBFS), no exact-zero frame. A normal laptop microphone sits near -60 dBFS.
  The Intel Smart Sound DSP is gating the room to almost nothing.
- **Speaker-to-microphone loopback (synthesised "Nexus" played through the speakers, 15 trials, five voices):** the microphone recorded exact zero every time.
  The driver cancels the laptop's own playback. A loopback rig therefore **cannot** test this microphone; only a human voice can.
- Windows shows the capture endpoint at 100% and not muted; NEXUS already holds a silent render stream to keep the DSP awake.

## Diagnosis, ranked by confidence

1. **The microphone front end is the weakest link (high confidence it matters, medium on the exact mechanism).** The driver gates quiet audio to near nothing and cancels playback.
   A gate that opens late clips the start of "Nexus" and of a command, which looks like "it didn't listen" or "it heard something else". It is not hallucination: the recogniser only
   invents text on the audio it is given (14 times today, all filtered).
2. **The first-stage detector is too eager on speech and not eager enough on the owner.** 38 ordinary-speech triggers in about 10 hours, and 35 near misses just under the threshold.
   Both are the signature of a model that has not seen enough hard negatives and varied positives (its last measured held-out recall was 63%; see 03-current-evidence).
3. **The second stage works but costs privacy and cannot hear gated audio.** It rejected all 38 false triggers, which is why wrong wakes are rare. But every trigger uploads ambient audio to a
   cloud recogniser (38 uploads today), and 7 came back empty because the gated audio held nothing.
4. **The speaker check does not discriminate (high confidence on the symptom).** 45 of 45 accepted at 0.93 to 0.99, including audio the recogniser said was not the wake word, so most
   were presumably not the owner or at least not a wake. The check uses a mean of the wake model's own embeddings, which describe sounds, not voices. Voice Lock is not protecting today.
5. **Hallucination is real but contained.** The filter and VAD already stop it reaching commands; it is a cost (17% of recogniser calls are wasted), not the cause of "broken".

## What other systems do

- **Two-stage cascade is the industry standard.** Alexa verifies a cheap on-device detection in the cloud; Apple runs a lightweight detector then a second, more accurate on-device check,
  and reports that delaying the decision for 3% of triggers cut false rejections by 66% (Apple, Progressive Voice Trigger Detection). Design rule: fire cheaply and eagerly, filter in stage two.
- **openWakeWord** adds a VAD gate and a custom verifier trained on the deployment's own false activations; its project targets under 0.5 false accepts/hour and under 5% false rejects.
- **Whisper hallucinations:** gate with VAD before the recogniser, then drop a segment only when silence probability and low confidence agree; do not condition on previous text.
- **Turn ends:** Silero VAD to detect speech, a small semantic model (Smart Turn) to decide the sentence is finished; a fixed silence timer alone cuts people off.
- **Windows capture:** request a raw stream (`AUDCLNT_STREAMOPTIONS_RAW`) where the driver supports it; otherwise Communications-mode processing (noise gating, echo cancellation) shapes the audio.
- **On-device recognition:** Moonshine tiny streaming matches Whisper tiny accuracy (12.0% vs 12.8% WER, vendor figures) at a fraction of the latency, so a local second stage is practical.

## Recommended solution for NEXUS (ordered; each step is measured before the next)

1. **Measure first (done here + one owner session).** `voice_funnel_report.py` turns any log into the table above; the trial below gives the missing ground truth (misses, command errors).
2. **Fix the input path.** Probe `RawProcessingSupported` for the active capture endpoint and, if true, open the wake stream raw (keep the keepalive); log the first 300 ms of every capture so
   onset clipping is visible; compare raw vs processed on the same trials.
3. **Move the speech check on-device.** Run the second stage with the already-shipped Moonshine streaming model (no upload of ambient audio, no rate limits, lower latency), keep the cloud only as
   a fallback after a local accept. Add a Silero VAD gate in front of both so empty clips never reach it.
4. **Retune stage one with the verifier behind it.** Lower the neural threshold from 0.68 toward 0.60 only if the trial shows owner misses in the 0.55 to 0.68 band, and feed today's 38 rejected clips
   (and the owner's near misses) to the trainer as hard negatives and positives.
5. **Replace the speaker check** with a real speaker-embedding model benchmarked by `scripts/speaker_benchmark.py`; until that passes, do not describe Voice Lock as protection.
6. **End-of-turn:** keep the current silence endpointing; evaluate Smart Turn only if the trial shows commands cut off.

## Live trial (needs the owner; about 10 minutes)

Run NEXUS normally. Say each line once with 3 s between, at arm's length, in the usual room, and note the clock time you started:

1. "Nexus" ten times, then "Hey Nexus" ten times, without any command.
2. Then twenty commands, each preceded by "Nexus": five app opens ("open notepad"), five questions ("what time is it"), five two-part ("open calculator then close it"), five long (about ten words).
3. Write down any attempt you know failed (nothing happened, wrong text, cut off).

Then run `python scripts/voice_funnel_report.py --since <your start time in UTC>`. Compare: attempts you made vs wakes fired (recall), near misses (what was lost just under the threshold),
captures delivered vs your failures list (command loss), and the transcripts for command errors. That one session gives the recall, command accuracy and failure split this document can only estimate.
