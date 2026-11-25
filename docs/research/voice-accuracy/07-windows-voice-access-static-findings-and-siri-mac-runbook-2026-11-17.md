# Windows Voice Access: how it turns speech into the right command (static findings), plus the Siri follow-up on the Mac (2026-11-17)

Purpose: learn, from a shipping assistant on the platform NEXUS runs on, how it recovers from a misheard word. Companion to note 06 and the Siri write-up in NEXUS-PAPERS (`research/wakeword/hey-siri-reverse-engineering-rea-2026-11-06.md`).

## Scope and method (same standard as the Siri paper)

- Own hardware (this Windows 11 laptop, build 26300), **read-only**. File listing, hashes, file metadata, plain-text data files, and the readable text strings inside the binaries. **No decompilation, no debugging, no process injection, no network capture, nothing patched or launched.**
- Interoperability and learning motive: NEXUS builds its own command layer.
- **Microsoft's data files are not copied into the repository.** This note records structure, counts and file hashes only.
- Evidence versus interpretation: counts, file names, class and function names, and log-format strings are **evidence**. What the code does with them, and every threshold, is **not** recovered (strings only show log formats; the constants are in the compiled code).

## Where Voice Access really lives

`C:\Windows\System32\VoiceAccess.exe` (278 KB, 10.0.26100.8117) is only a launcher stub: it imports process and shell APIs and one function name, `VoiceAccessStub::ProcessCommandLine`. The product is a set of components inside the system app `MicrosoftWindows.Client.Core`:

| File | Size | Role (from names and strings) |
|---|---:|---|
| `VoiceAccess.dll` | 610 KB | controller: sessions, intent execution, feedback |
| `VoiceAccessCommon.dll` | 990 KB | shared logic: homophones, text correction, vocabulary, flexible commanding |
| `SpeechRecognizer.dll` | 908 KB | offline and online intent recognition around the Speech SDK |
| `VoiceAccessHost`, `Plugin`, `UserInterface` | 0.3 to 1.9 MB | host, plug-ins, overlay UI |
| `VoiceAccess\Microsoft.CognitiveServices.Speech.*` | 0.2 to 8.3 MB each | Microsoft Speech SDK with **embedded (on-device) speech recognition**, **language understanding extension**, ONNX runtime, audio-stack extension |
| `VoiceAccess\MASmodels\aec_v1.fpie` | 1.1 MB | acoustic echo cancellation model (Microsoft Audio Stack) |

Windows voice typing (Win+H) is separate (`TextInput.CarbonDictation.dll`, `TextInput.DictationUI.dll` in `MicrosoftWindows.Client.CBS`) and also bundles the embedded Speech SDK.

## The architecture, in the order a spoken command travels

1. **On-device recognition.** The embedded Speech SDK recogniser with an offline model; `SpeechRecognition_RequestWordLevelCorrections` shows it asks for word-level alternatives, not just one string.
2. **Intent recognition, two backends.** `OfflineIntentRecognition` (it **trains an offline command model on the device** from the intent definitions: `EmbeddedCommandsManager::TrainOfflineModel`) and `OnlineLUISService` (cloud language understanding) with `GetProbableIntents`, so more than one candidate intent is kept.
3. **Exact grammar first.** `IntentEntityMap.json` holds **4,983 command templates** with typed slots such as `<builtin.number>` and entity lists (for example the phonetic alphabet), plus separate sets for dictation sentences (434), setting commands (11) and user phrase additions. `VoiceCommands.xml` (151 KB) is the spoken command set.
4. **Flexible commanding for phrasing the grammar misses.** `FlexibleCommandingTrainingFile.json` (7.6 MB) holds **131 intents, each with about 12 example phrasings (1,560 in total, each with a 100-number embedding vector)**. The handler functions are `CreateEmbeddingsModel`, `PredictIntent`, `ExtractEntity`, `IsCorrectedTextSimilar`. Log formats show the decision: "Predicted intent: …, Predicted entities: …, Confidence_score: …" and "Confidence score is greater than %f. Executing command". So an unfamiliar phrasing is matched to the **nearest known intent by embedding similarity, and executed only above a confidence threshold** (the threshold value is not recoverable from strings).
5. **Three recovery modes**, named in code (`kDisambiguationMode`, `kCorrectionMode`, `kVocabularyMode`) with their own session classes:
   - **Disambiguation** (`DisambiguationSession`): when a spoken target could be several things, show options and let the user pick.
   - **Correction** (`CorrectionSession`, `TextCorrection::GetCorrectedTextAsync`, `Task.Dictation.Correct`): fix text after the fact using **word alternates** and a **spelling dictionary** (`SpellDictionary`).
   - **Vocabulary** (`VocabularyHandler::LoadUserPhrases / UpdateUserPhrasesFile`, `Task.Vocabulary.Add`): the user's own phrases saved to a file and fed back to recognition.
6. **Homophones.** `Homophones.json` has **602 English groups (about 1,290 words)** of sound-alikes ("aisle / isle / I'll"), consumed by `VoiceAccessCommon.Homophones` and `WordAlternates`.
7. **Feedback.** Events named `FlexibleCommandWrongActionFeedback`, `Incorrect`, `IncorrectAction`, `FlexibleCommandIntentAndScore`, `FlexibleCommandExecuteFeedback` show it records when a flexible command ran the wrong thing, which is the signal for improving the model (how it is used was not recovered).
8. **Mic hygiene.** An echo-cancellation model and a voice-isolation component are part of the pipeline, so its own speech and noise are removed before recognition.

## What this means for NEXUS (new compared with note 06)

1. **Two matching tiers, not one.** Exact templates for the common commands, plus an **embedding nearest-intent tier gated by a confidence threshold** for natural phrasing. NEXUS already has a BERT-Mini NLU, so this tier exists in spirit; what is missing is the **confidence-gated execute-or-ask rule** and logging of the score.
2. **Three explicit recovery modes** map one-to-one onto the plan in note 06: disambiguation (numbered choices), correction (alternates plus homophones), vocabulary (user-owned phrase list). Build them as named modes with their own small state, as Microsoft did.
3. **Homophone and alternates lists are plain data**, about 600 groups. NEXUS should generate its own from CMUdict plus its entity list (do not copy Microsoft's file) and keep it editable.
4. **Train the offline command model on the device from the user's own commands and entities**, as `TrainOfflineModel` does, instead of shipping a fixed grammar.
5. **Record intent plus score for every command and every "wrong action" complaint.** That is the data that lets thresholds be set from evidence rather than guessed.
6. **Echo cancellation and voice isolation are shipped as models**, which supports note 05's finding that NEXUS's input path is the weakest layer.

## What this pass could not tell us

- No thresholds, scores, timings or control flow (strings only; no decompiler).
- How the cloud (LUIS) and offline paths are chosen, and the exact fusion of grammar and embedding results.
- The recogniser's own weights and vocabulary biasing internals (inside the Speech SDK extension DLLs; not examined).
- Behaviour on real speech: no audio was run through it. Black-box accuracy tests would need a human voice and are a separate step.

## Evidence (hashes are the first 16 hex characters of SHA-256)

| File | Size | SHA-256 prefix |
|---|---:|---|
| VoiceAccess.exe (stub) | 278,528 | 0AC98D6EBF859296 |
| VoiceAccess.dll | 610,304 | 92053773F20D2659 |
| VoiceAccessCommon.dll | 989,696 | 3FFA884DE41FCE86 |
| SpeechRecognizer.dll | 907,776 | FCFF6924A4D1D74E |
| Homophones.json | 31,114 | 052099B5C46A384E |
| IntentEntityMap.json | 1,634,385 | B23B2A8FB8923363 |
| FlexibleCommandingTrainingFile.json | 7,592,015 | B4DB02F991922B7D |
| VoiceCommands.xml | 151,254 | AA67F8F1F269A398 |
| aec_v1.fpie | 1,142,488 | 1EAA07F073FA65F2 |

Reproduce (read-only, PowerShell and Python 3): list `C:\Windows\SystemApps\MicrosoftWindows.Client.Core_cw5n1h2txyewy`, hash the files above, read the three JSON files with `json.load`, and extract printable ASCII and UTF-16 strings from the three DLLs (keyword filter: `homophon|confiden|alternat|correct|vocabular|embedding|intent|disambig`).

## Siri follow-up on the Mac (runbook; needs the owner's Mac)

The existing Siri paper stopped at the wake word. Its own entitlement list points at the recognition side, which is what this question needs. Same rules: your Mac, read-only, SIP on, nothing launched or patched, no data files copied into the repository.

1. Re-run the paper's section 10 commands to confirm the same `corespeechd` hash (`ca671b6a871dd90f26624d6850121f9832c77993de79c50aac2939836cfec5a1`) or record the new one.
2. **Open `regex.json`** (47 KB, `CoreSpeech.framework/.../Resources/`): report its structure (top-level keys, number of patterns, what categories they cover) without copying contents. This is Apple's command pattern set, the closest analogue to Microsoft's `IntentEntityMap.json`.
3. List the personalisation pieces the entitlement list names: `speech-dictation-personalization`, `SiriVocabulary` storage, `speech-model-training`, and the `DistributedEvaluation ... asr` entries. With the REA commands, list the Objective-C classes and selectors whose names contain `Vocabulary`, `Personal`, `Correction`, `Rescor`, `Contact`, `Entity`; record names only.
4. Search the same daemon's class list for the **second-chance** and **speaker** nodes already recorded, and add any class that names a *language-model* or *rescoring* step.
5. If REA offers a Hopper or Ghidra engine on the Mac, decompile only the functions named in step 3 for **structure** (what inputs they take), not to extract keys or bypass protections.
6. Black-box check, optional: on the Mac dictate 20 sound-alike pairs ("sink / sync", "notepad / note pad", a contact name) into Dictation with and without the name in Contacts, and record which are fixed by context. This needs a human voice.

Deliverable: a short appendix in the NEXUS-PAPERS Siri document, same evidence-versus-interpretation labelling.
