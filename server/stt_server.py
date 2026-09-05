"""
Minimal faster-whisper STT server for NEXUS.

This server runs LOCALLY on the user's device (127.0.0.1:39217).
Audio is sent from the NEXUS client to this local server, transcribed,
and only the resulting TEXT is sent to the remote NEXUS server.
Audio NEVER leaves the device.

For devices without a GPU, use:
  WHISPER_DEVICE=cpu WHISPER_COMPUTE=int8
  WHISPER_MODEL=tiny.en   (fastest, ~0.5s transcription, ~150MB RAM)

Requirements:
  pip install faster-whisper fastapi uvicorn python-multipart

Run locally on the device:
  uvicorn stt_server:app --host 127.0.0.1 --port 39217

Environment:
  WHISPER_MODEL    ΓÇö model name (default: tiny.en)
  WHISPER_DEVICE   ΓÇö "cuda" or "cpu" (default: cpu)
  WHISPER_COMPUTE  ΓÇö compute type (default: int8)

Models (CPU):
  - tiny.en:  ~40MB, fastest (~0.5s), good accuracy for commands (recommended)
  - base.en:  ~75MB, slower (~1.5s), slightly better accuracy
  - small.en: ~250MB, slow (~3s), best accuracy

Models (GPU):
  - large-v3:           ~1.5GB VRAM with int8_float16
  - distil-large-v3:    ~750MB VRAM, faster, slightly lower accuracy
"""

from __future__ import annotations

import os
import logging
import time

from fastapi import FastAPI, UploadFile, File
from fastapi.responses import JSONResponse
from faster_whisper import WhisperModel

log = logging.getLogger("NEXUS.stt")
logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s %(message)s")

# Default to base.en — better accuracy than tiny.en for command recognition.
# tiny.en (39M params) mishears "give me the PR list" as "Google me the PR list".
# base.en (74M params) is ~1.5s on CPU but much more accurate.
# The .en variant is English-only, which is faster and smaller than multilingual.
MODEL_NAME = os.getenv("WHISPER_MODEL", "base.en")
DEVICE = os.getenv("WHISPER_DEVICE", "cpu")
COMPUTE_TYPE = os.getenv("WHISPER_COMPUTE", "int8")

# Hotwords ΓÇö biases the Whisper decoder toward known app/brand names so that
# mispronunciations like "gamail" are more likely to be transcribed as "gmail".
# This is faster-whisper's built-in hotword feature (PR #731, merged May 2024).
# It adds the hotwords as a prompt to every transcription window, unlike
# initial_prompt which only affects the first window.
#
# DYNAMIC HOTWORDS: In addition to the built-in list, the server reads a
# hotwords file (default: %APPDATA%\com.nexus.assistant\stt_hotwords.txt on
# Windows, ~/.config/nexus/stt_hotwords.txt on Linux/Mac). NEXUS writes the
# user's GitHub repo names to this file so Whisper recognises them correctly.
# The file is re-read on every transcription request (hot-reload, no restart).
_DEFAULT_HOTWORDS = [
    # Web apps / services
    "gmail", "youtube", "github", "google", "chrome", "brave", "firefox",
    "twitter", "instagram", "facebook", "reddit", "linkedin", "whatsapp",
    "netflix", "amazon", "wikipedia", "twitch", "spotify", "discord",
    "slack", "notion", "figma", "chatgpt", "claude", "gemini",
    # Google suite
    "drive", "docs", "sheets", "slides", "maps", "calendar", "translate",
    "photos", "meet", "chat",
    # Native apps
    "notepad", "calculator", "explorer", "terminal", "powershell",
    "paint", "settings", "outlook", "word", "excel", "powerpoint",
    "vscode", "code", "steam", "zoom", "teams", "skype", "telegram",
    # Action words
    "open", "launch", "start", "search", "find", "close",
    # Analysis commands (improves recognition of NEXUS commands)
    "analyse", "analyze", "analysis", "review", "pull request", "PR",
    "branch", "commit", "merge", "diff",
    # PR list commands — STT was mishearing "give me the PR list" as
    # "Google me the PR list" and "show me the PR list" as "So, you have to list"
    "PR list", "PRs", "list PRs", "show PRs", "give me", "show me",
    "list", "open PRs", "closed PRs",
    # Architecture mapper commands — comprehensive coverage
    "architecture", "architect", "mapper", "codebase", "dependency",
    "dependencies", "diagram", "graph", "viewer", "explorer",
    "architecture mapper", "architect mapper", "architecture map",
    "codebase mapper", "dependency mapper", "architecture diagram",
    # Known user repos (add more via stt_hotwords.txt)
    "servx", "NEXUS", "ULTRON", "zync", "ledger ai", "ledger-ai",
]

# Path to the dynamic hotwords file ΓÇö NEXUS writes repo names here.
import platform
if platform.system() == "Windows":
    _appdata = os.getenv("APPDATA", "")
    HOTWORDS_FILE = os.path.join(_appdata, "com.nexus.assistant", "stt_hotwords.txt")
else:
    HOTWORDS_FILE = os.path.expanduser("~/.config/nexus/stt_hotwords.txt")


def _load_hotwords() -> str:
    """Load hotwords from the built-in list + the dynamic hotwords file."""
    words = list(_DEFAULT_HOTWORDS)
    # Override with env var if set (for testing)
    env_hotwords = os.getenv("WHISPER_HOTWORDS")
    if env_hotwords:
        return env_hotwords
    # Read the dynamic hotwords file (hot-reload on every request)
    try:
        if os.path.exists(HOTWORDS_FILE):
            with open(HOTWORDS_FILE, "r", encoding="utf-8") as f:
                for line in f:
                    w = line.strip()
                    if w and w not in words:
                        words.append(w)
    except Exception:
        pass
    return " ".join(words)

app = FastAPI(title="NEXUS STT", version="0.2.0")

# EAGER model loading ΓÇö load at startup so the first transcription is fast.
# The previous lazy loading caused a 10-15s delay on the first command.
log.info("loading whisper model=%s device=%s compute=%s", MODEL_NAME, DEVICE, COMPUTE_TYPE)
_load_start = time.monotonic()
_model: WhisperModel = WhisperModel(MODEL_NAME, device=DEVICE, compute_type=COMPUTE_TYPE)
log.info(
    "whisper model loaded in %.1fs ΓÇö ready for transcription",
    time.monotonic() - _load_start,
)


def _get_model() -> WhisperModel:
    return _model


@app.get("/health")
async def health() -> JSONResponse:
    hw = _load_hotwords()
    return JSONResponse({
        "ok": True,
        "model": MODEL_NAME,
        "device": DEVICE,
        "hotwords": hw[:80] + "..." if len(hw) > 80 else hw,
        "hotwords_file": HOTWORDS_FILE,
    })


@app.post("/transcribe")
async def transcribe(audio: UploadFile = File(...)) -> JSONResponse:
    """Transcribe raw audio bytes. Returns {"text": "transcript"}.

    Accepts WAV, MP3, FLAC, or any format supported by PyAV.
    Raw 16-bit PCM is also accepted ΓÇö we wrap it in a WAV header so
    PyAV/faster-whisper can decode it.
    """
    import io
    import struct
    audio_bytes = await audio.read()
    if not audio_bytes:
        return JSONResponse({"text": ""}, status_code=400)

    # Check if the bytes start with a RIFF/WAV header. If not, assume raw
    # 16-bit LE mono PCM at 16kHz and wrap it in a WAV header so PyAV can
    # decode it.
    is_wav = len(audio_bytes) >= 12 and audio_bytes[:4] == b"RIFF" and audio_bytes[8:12] == b"WAVE"
    if not is_wav:
        sample_rate = 16000
        num_channels = 1
        bits_per_sample = 16
        data_len = len(audio_bytes)
        header = struct.pack(
            "<4sI4s4sIHHIIHH4sI",
            b"RIFF",
            36 + data_len,
            b"WAVE",
            b"fmt ",
            16,  # fmt chunk size
            1,   # PCM
            num_channels,
            sample_rate,
            sample_rate * num_channels * bits_per_sample // 8,  # byte rate
            num_channels * bits_per_sample // 8,  # block align
            bits_per_sample,
            b"data",
            data_len,
        )
        audio_bytes = header + audio_bytes
        log.info("wrapped raw PCM (%d bytes) in WAV header", data_len)

    audio_file = io.BytesIO(audio_bytes)
    audio_file.name = "audio.wav"  # hint for PyAV format detection

    model = _get_model()
    _transcribe_start = time.monotonic()
    try:
        # Reload hotwords on every request (hot-reload from file)
        hotwords = _load_hotwords()
        # initial_prompt gives the model context about expected vocabulary.
        # This is especially important for tiny.en (39M params) which struggles
        # with hotwords alone. The prompt biases the decoder toward recognised
        # words like "servx", "analyse", "PR" without forcing them.
        initial_prompt = (
            "The user is giving voice commands to a desktop assistant called NEXUS. "
            "Common commands include: open architecture mapper, open the architecture mapper, "
            "show architecture, show me the architecture, launch architecture mapper, "
            "open architect, open codebase mapper, open dependency mapper, "
            "open architecture diagram, open architecture graph, "
            "analyse PR 5 in servx, review PR 3 in servx, "
            "analyse PR 24 in ledger ai, open gmail, "
            "search youtube, close notepad, open architect mapper. "
            "Recognised names: servx, NEXUS, ULTRON, ledger ai, ledger-ai, github, gmail, "
            "architecture, architect, mapper, codebase, dependency, diagram, graph."
        )
        segments, _info = model.transcribe(
            audio_file,
            language="en",
            hotwords=hotwords,
            initial_prompt=initial_prompt,
            # Use VAD but with gentle parameters ΓÇö default VAD is too aggressive
            # on short command clips (<2s) and discards them entirely.
            # min_silence_duration_ms=1500 matches the frontend VAD timing.
            vad_filter=True,
            vad_parameters=dict(
                min_silence_duration_ms=1500,
                speech_pad_ms=250,
                threshold=0.3,
            ),
            # Greedy decoding ΓÇö much faster than beam search (beam_size=5).
            # For short voice commands, the accuracy difference is negligible.
            beam_size=1,
        )
        text = " ".join(seg.text for seg in segments).strip()
    except Exception as e:
        log.error("transcription failed: %s", e)
        return JSONResponse({"text": "", "error": str(e)}, status_code=500)

    _elapsed = time.monotonic() - _transcribe_start
    log.info(
        "transcribed %d bytes ΓåÆ %d chars in %.2fs",
        len(audio_bytes), len(text), _elapsed,
    )
    return JSONResponse({"text": text})


if __name__ == "__main__":
    import uvicorn

    port = int(os.environ.get("NEXUS_STT_PORT", "39217"))
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="warning")
