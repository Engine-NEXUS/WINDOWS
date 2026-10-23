> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# NEXUS — Data Flow Graphs

> Sequence diagrams for every major path through the system.
> Each diagram shows which process owns each step and what crosses the network boundary.

---

## 1. Wake → General Request → Backend Response

The canonical "Siri-like" flow. User says something that isn't a known command, so it goes all the way to the backend.

```
 User        Tauri/Rust           Frontend (React)     Local STT        Sidecar         n8n + Ollama
  │              │                     │                   │               │                │
  │ "nexus"      │                     │                   │               │                │
  │ (speech)     │                     │                   │               │                │
  │              │ OWW KWS score>0.5   │                   │               │                │
  │              │ on 80ms chunk       │                   │               │                │
  │              │──win.eval()────────▶│                   │               │                │
  │              │                     │ state=idle→listening              │                │
  │              │                     │ getUserMedia(mic)                 │                │
  │              │                     │ startRecording (ScriptProc)       │                │
  │              │                     │ startVAD (Silero ONNX)            │                │
  │              │                     │                   │               │                │
  │ "summarize   │                     │                   │               │                │
  │  my email"   │                     │                   │               │                │
  │              │                     │ VAD: silence detected             │                │
  │              │                     │ finishCapture()                   │                │
  │              │                     │ downsample 48k→16k                │                │
  │              │                     │──PCM (multipart)─▶ 127.0.0.1:8000 │                │
  │              │                     │◀──transcript: "summarize email"───│                │
  │              │                     │ parseIntent() → unknown           │                │
  │              │                     │ openSession() retry backoff       │                │
  │              │                     │──invoke open_session──────────────▶                │
  │              │──WSS connect───────────────────────────────────────────▶                │
  │              │                     │──invoke send_transcript───────────▶                │
  │              │──ws {type:"transcript", data:"summarize email"}─────────▶                │
  │              │                     │                   │               │ get_valid_cred │
  │              │                     │                   │               │──POST /supervisor
  │              │                     │                   │               │  {transcript, credentials}
  │              │                     │                   │               │                │ classify (1.5B)
  │              │                     │                   │               │                │ route → email.summarize
  │              │                     │                   │               │                │──prompt──▶ Ollama 8B
  │              │                     │                   │               │                │◀──summary───│
  │              │                     │                   │               │◀──reply_text──│
  │              │◀──ws {type:"ack", data:"On it, sir."}───│               │                │
  │              │                     │◀──assistant:server event──────────│                │
  │              │                     │ TTS speaks "On it, sir."          │                │
  │              │                     │ state=speaking→thinking           │                │
  │              │◀──ws {type:"result", data:"You have 3..."}─────────────│                │
  │              │                     │◀──assistant:server event──────────│                │
  │              │                     │ TTS speaks result                 │                │
  │              │                     │ state=speaking                    │                │
  │              │◀──ws {type:"done"}──│                   │               │                │
  │              │                     │ state=idle, hide overlay           │                │
  │ ◀── spoken answer (local TTS)──────│                   │               │                │
```

**Network boundary:** Only the transcript text and the result text cross the wire. PCM audio goes to `127.0.0.1:8000` only.

---

## 2. Tier 3 Fixed Command (Fast Path)

A known spoken command like "open youtube" or "mute volume" is detected acoustically — no STT, no network.

```
 User        Tauri/Rust (OWW)      Frontend              Local OS
  │              │                     │                     │
  │ "open        │                     │                     │
  │  youtube"    │                     │                     │
  │              │ command classifier  │                     │
  │              │ score > threshold   │                     │
  │              │ on 80ms chunk       │                     │
  │              │──emit "command-detected" {action:"open_app", target:"youtube"}─▶│
  │              │                     │ setVisible(true)    │
  │              │                     │ state=speaking      │
  │              │                     │ TTS "Ok sir."       │
  │              │                     │──invoke execute_command──▶│
  │              │                     │                     │ app_registry::lookup("youtube")
  │              │                     │                     │ → URL fallback
  │              │                     │                     │ open::that("https://youtube.com")
  │              │                     │◀──{success:true}────│
  │              │                     │ hide after 800ms    │
  │ ◀── "Ok sir." (local TTS)─────────│                     │
```

**Latency:** ~200 ms from speech to action. **Network:** none.

---

## 3. Tier 3 Parameterized Command

A command pattern like "play <X> in spotify" is detected acoustically, then the parameter is captured via a short STT recording.

```
 User        Tauri/Rust (OWW)      Frontend              Local STT          Local OS
  │              │                     │                     │                  │
  │ "play        │                     │                     │                  │
  │  <song>      │                     │                     │                  │
  │  in spotify" │                     │                     │                  │
  │              │ cmd classifier      │                     │                  │
  │              │ fires w/ needs_param│                     │                  │
  │              │──emit "command-detected" {action:"spotify_play", needs_param:true}─▶│
  │              │                     │ state=speaking      │                  │
  │              │                     │ TTS "On it sir"     │                  │
  │              │                     │ wait for TTS end    │                  │
  │              │                     │ state=listening     │                  │
  │              │                     │ captureParameter(3s)│                  │
  │              │                     │──PCM 3s────────────▶│ 127.0.0.1:8000   │
  │              │                     │◀──"bohemian rhapsody"│                  │
  │              │                     │ state=thinking      │                  │
  │              │                     │──invoke execute_command {action:"spotify_play", query:"bohemian rhapsody"}─▶│
  │              │                     │                     │                  │ spotify:search:<urlencoded>
  │              │                     │                     │                  │ cmd /c start spotify:...
  │              │                     │◀──{success:true, message:"Playing bohemian rhapsody on Spotify, sir."}──│
  │              │                     │ TTS speaks message  │                  │
  │              │                     │ hide after 800ms    │                  │
  │ ◀── "Playing bohemian rhapsody..." (local TTS)───────────│                  │
```

**Latency:** ~3-5 s (dominated by the 3 s parameter capture + STT). **Network:** none (STT is local).

---

## 4. Boot Greeting

On a fresh system boot (uptime < 15 min), NEXUS speaks a greeting without any user action.

```
 Windows boot   nexus.exe (autostart)    Rust setup        Frontend           Local TTS
  │                  │                       │                  │                  │
  │ login            │                       │                  │                  │
  │─────────────────▶│ launch                │                  │                  │
  │                  │──Tauri builder───────▶│                  │                  │
  │                  │                       │ spawn sidecar    │                  │
  │                  │                       │ (background)     │                  │
  │                  │                       │ spawn OWW engine │                  │
  │                  │                       │ spawn meeting    │                  │
  │                  │                       │  detection loop  │                  │
  │                  │                       │ spawn sleep-wake │                  │
  │                  │                       │  watcher         │                  │
  │                  │                       │ window_manager   │                  │
  │                  │                       │ mic_permissions  │                  │
  │                  │                       │ hotkey           │                  │
  │                  │                       │ network bridge   │                  │
  │                  │                       │──webview loads──▶│                  │
  │                  │                       │                  │ main.tsx runs    │
  │                  │                       │                  │──invoke frontend_ready─▶│
  │                  │                       │                  │                  │
  │                  │                       │ uptime = sysinfo │                  │
  │                  │                       │  ::System::uptime│                  │
  │                  │                       │ fresh_boot =     │                  │
  │                  │                       │  uptime < 900s   │                  │
  │                  │                       │ meeting? paused? │                  │
  │                  │                       │ should_greet =   │                  │
  │                  │                       │  fresh_boot &&   │                  │
  │                  │                       │  !meeting &&     │                  │
  │                  │                       │  !paused         │                  │
  │                  │                       │──return true─────│                  │
  │                  │                       │                  │ greet()          │
  │                  │                       │                  │ setVisible(true) │
  │                  │                       │                  │ state=speaking   │
  │                  │                       │                  │──speak("Hello sir, how can I assist you today?")─▶│
  │                  │                       │                  │                  │ SpeechSynthesis
  │ ◀──────── "Hello sir, how can I assist you today?" ────────────────────────────│
  │                  │                       │                  │ setVisible(false)│
  │                  │                       │                  │ reset() after 550ms│
```

**Network:** none. The greeting is entirely local. The sidecar may still be starting up in the background — the greeting doesn't wait for it.

---

## 5. Sleep / Wake Greeting

A background thread watches the wall clock. If `thread::sleep(10s)` actually takes much longer (because the system slept), NEXUS greets on resume.

```
 sleep-wake-watch thread          System clock          Frontend           Local TTS
  │                                  │                      │                  │
  │ t0 = SystemTime::now()           │                      │                  │
  │ thread::sleep(10s)               │                      │                  │
  │   ...system sleeps...            │                      │                  │
  │   ...system wakes...             │                      │                  │
  │ t1 = SystemTime::now()           │                      │                  │
  │ gap = t1 - t0  (e.g. 4 hours)    │                      │                  │
  │ gap > 60s?  YES                  │                      │                  │
  │ meeting? paused?  NO             │                      │                  │
  │──emit "app:greeting"───────────────────────────────────▶│                  │
  │                                  │                      │ greet()          │
  │                                  │                      │──speak("Hello sir, ...")─▶│
  │ ◀──────── "Hello sir, ..." ─────────────────────────────────────────────────│
```

**Why this works:** `thread::sleep` uses the monotonic clock, which stops during sleep. `SystemTime` uses the wall clock, which jumps forward. The delta between them reveals the sleep duration.

---

## 6. Meeting Detection → Suppression

A 2-second polling loop detects when another app is using the microphone. When detected, wake + Tier 3 + TTS are all suppressed.

```
 meeting detection loop (2s poll)     WASAPI / process scan       MeetingState (atomics)      OWW audio callback
  │                                       │                            │                            │
  │ tick                                  │                            │                            │
  │──check_wasapi_microphone_usage()─────▶│                            │                            │
  │                                       │ enumerate capture sessions │                            │
  │                                       │ skip our PID + AudioSrv    │                            │
  │                                       │ any active session?        │                            │
  │                                       │──YES──────────────────────▶│                            │
  │                                       │                            │ meeting_active = true     │
  │                                       │                            │                            │
  │                                       │                            │                            │ on next 80ms chunk:
  │                                       │                            │◀──should_suppress_wake()?─│
  │                                       │                            │  true (meeting)           │
  │                                       │                            │──return early (no KWS)───▶│
  │                                       │                            │                            │
  │                                       │                            │ frontend calls speak()    │
  │                                       │                            │──should_suppress_tts()?   │
  │                                       │                            │  true → TTS silenced      │
```

**Hotkey still works** during a meeting — it's an explicit user action, not suppressed.

---

## 7. OAuth Connect Flow (Google)

The setup page connects a Google account. The client secret never leaves the server.

```
 Setup page (React)         System browser          Google OAuth          Sidecar /oauth/*        SQLite DB
  │                            │                       │                     │                       │
  │ user clicks "Connect Google"                     │                     │                       │
  │ generateCodeVerifier()    │                       │                     │                       │
  │ generateCodeChallenge()   │                       │                     │                       │
  │──GET /oauth/auth-url?provider=google&code_challenge=...──────────────────▶│                     │
  │                            │                       │                     │ build auth URL        │
  │                            │                       │                     │  (includes client_id, │
  │                            │                       │                     │   redirect, challenge)│
  │◀──{url:"https://accounts.google.com/o/oauth2/v2/auth?..."}────────────────│                     │
  │──open(url) via shell──────▶│                       │                     │                       │
  │                            │ user logs in          │                     │                       │
  │                            │ grants consent        │                     │                       │
  │                            │──redirect nexus://oauth/callback?code=XXX───│                     │
  │                            │                       │                     │                       │
  │  Tauri deep-link plugin catches the redirect      │                     │                       │
  │──emit "deep-link://oauth-callback" with URL       │                     │                       │
  │  handleOAuthRedirect(url)                         │                     │                       │
  │  extract code + state                             │                     │                       │
  │──POST /oauth/exchange {provider, code, code_verifier, user_id}──────────▶│                     │
  │                            │                       │                     │──POST token endpoint─▶│
  │                            │                       │◀──{access_token, refresh_token, expires_in}──│
  │                            │                       │                     │──store_oauth_token()──▶│
  │                            │                       │                     │                       │ INSERT/REPLACE
  │◀──{ok:true, provider:"google", connected:true}───│                     │                       │
```

**Client secret stays server-side.** The client only ever has the PKCE verifier. Tokens are stored per-user in SQLite, encrypted at rest with Fernet for API keys.

---

## 8. API Key Add Flow (Claude / Devin / etc.)

For services that don't support OAuth (or where the user prefers a simple key), the user pastes an API key in the setup page.

```
 Setup page (React)                Sidecar /apikeys/add           SQLite DB (Fernet-encrypted)
  │ user types provider + key           │                            │
  │──POST /apikeys/add {user_id, provider, api_key}──────────────▶  │
  │                                     │ store_api_key()            │
  │                                     │  Fernet.encrypt(api_key)   │
  │                                     │──INSERT INTO api_keys────▶│
  │◀──{ok:true, provider:"claude", stored:true}──────────────────│
```

**At request time**, `get_valid_credentials(user_id)` decrypts the key and injects it into the n8n webhook payload. The key never appears in the client after storage.

---

## 9. Sidecar Auto-Spawn (Non-Blocking)

On NEXUS startup, the sidecar is spawned in a background thread so the frontend can load immediately.

```
 Tauri setup hook          sidecar_manager::init() (bg thread)      pythonw.exe          Frontend wsBridge
  │                            │                                        │                    │
  │ std::thread::spawn(init)   │                                        │                    │
  │──return Ok(()) immediately│                                        │                    │
  │                            │ is_sidecar_healthy(49152)?             │                    │
  │                            │  TCP connect → NO                      │                    │
  │                            │ find_python() → "pythonw"              │                    │
  │                            │ resolve_sidecar_dir()                  │                    │
  │                            │ spawn_sidecar()                        │                    │
  │                            │──cmd pythonw -m uvicorn sidecar.sidecar:app --host 127.0.0.1 --port 49152─▶│
  │                            │                                        │ import sidecar     │
  │                            │                                        │ FastAPI startup    │
  │                            │                                        │ init_db()          │
  │                            │                                        │ bind 127.0.0.1:49152│
  │                            │ wait_for_health() poll 500ms           │                    │
  │                            │  TCP connect → YES                     │                    │
  │                            │ log "sidecar healthy"                  │                    │
  │                            │                                        │                    │ openSession() retry:
  │                            │                                        │                    │  attempt 1 → fail (sidecar not ready)
  │                            │                                        │                    │  wait 1s
  │                            │                                        │                    │  attempt 2 → fail
  │                            │                                        │                    │  wait 2s
  │                            │                                        │                    │  attempt 3 → success
```

**Key insight:** The frontend's WebSocket retry (1s → 2s → 4s → 8s backoff) connects once the sidecar is ready. The user sees the orb immediately, even if the sidecar takes 5-8 seconds to cold-start.

---

## 10. Cancel / Barge-In

The user can interrupt NEXUS at any point by saying "nexus" again or pressing the hotkey.

```
 User        OWW KWS / hotkey       Frontend                  TTS
  │              │                     │                        │
  │ NEXUS is speaking "You have 3..."  │                        │
  │ "nexus" (wake during TTS)          │                        │
  │              │ wake fires          │                        │
  │              │──win.eval()────────▶│                        │
  │              │                     │ state=speaking→listening
  │              │                     │ stopTts() ────────────▶│ speechSynthesis.cancel()
  │              │                     │ stopVad()              │
  │              │                     │ abortCapture()         │
  │              │                     │ startListening() again │
```

**Barge-in works at any state** except `idle`. The TTS `interrupted` error is avoided because `stopTts()` is called before starting a new utterance.
