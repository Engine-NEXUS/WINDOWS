"""Measures the LOCAL live-caption source (server/stt_server.py, the /stream WebSocket) with real speech, no microphone.

For each WAV made by scripts/make-test-speech.ps1 it sends the audio at real speaking pace (100 ms chunks, one chunk per 100 ms
of wall time, the way the microphone delivers it), records when every caption update arrives, and reports:
  * time to the first caption (ms after the speech started)
  * for every spoken word: how long after it STARTED to be spoken it first appeared in a caption (the lag a reader sees)
  * whether captions only grew (text that is later changed or removed is "flicker")
  * the last caption against the sentence that was spoken (word error rate)
  * the same audio sent in one piece to /transcribe (the batch result the command is really run from)

  python scripts/measure-live-captions.py D:\\nexus-test-speech\\short.wav [more.wav ...] [--port 39217] [--json out.json]
Needs: pip install websockets  (and the server running: NEXUS_STT_PORT=39217 python server/stt_server.py)
"""
import asyncio, json, re, sys, time, wave, urllib.request, argparse, uuid
import websockets

CHUNK_MS = 100


def read_pcm(path):
    with wave.open(path, "rb") as w:
        assert w.getframerate() == 16000 and w.getnchannels() == 1 and w.getsampwidth() == 2, "need 16 kHz mono 16-bit"
        return w.readframes(w.getnframes())


def norm(t):
    return re.sub(r"[^a-z0-9' ]+", " ", t.lower()).split()


def wer(ref, hyp):
    r, h = norm(ref), norm(hyp)
    d = list(range(len(h) + 1))
    for i in range(1, len(r) + 1):
        prev, d[0] = d[0], i
        for j in range(1, len(h) + 1):
            cur = d[j]
            d[j] = min(d[j] + 1, d[j - 1] + 1, prev + (r[i - 1] != h[j - 1]))
            prev = cur
    return d[len(h)] / max(1, len(r))


def batch(path, port):
    """The same audio, whole, to /transcribe (multipart)."""
    boundary = uuid.uuid4().hex
    data = open(path, "rb").read()
    body = (f"--{boundary}\r\nContent-Disposition: form-data; name=\"audio\"; filename=\"a.wav\"\r\nContent-Type: audio/wav\r\n\r\n").encode() + data + f"\r\n--{boundary}--\r\n".encode()
    req = urllib.request.Request(f"http://127.0.0.1:{port}/transcribe", data=body, headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})
    t0 = time.monotonic()
    out = json.loads(urllib.request.urlopen(req, timeout=60).read())
    return out.get("text", ""), (time.monotonic() - t0) * 1000


async def run_one(path, port):
    pcm = read_pcm(path)
    meta = {}
    try:
        meta = json.load(open(path.rsplit(".", 1)[0] + ".words.json", encoding="utf-8"))
    except Exception:
        pass
    step = 16000 * 2 * CHUNK_MS // 1000
    events = []  # (ms since the first audio chunk was sent, text)
    async with websockets.connect(f"ws://127.0.0.1:{port}/stream", max_size=None) as ws:
        t0 = time.monotonic()

        async def reader():
            try:
                async for m in ws:
                    events.append(((time.monotonic() - t0) * 1000, json.loads(m).get("text", "")))
            except Exception:
                pass

        task = asyncio.create_task(reader())
        sent_until = 0
        for i in range(0, len(pcm), step):
            await ws.send(pcm[i : i + step])
            sent_until = i + step
            # one chunk per CHUNK_MS of wall time, like a microphone
            await asyncio.sleep(max(0, (i // step + 1) * CHUNK_MS / 1000 - (time.monotonic() - t0)))
        end_of_speech_ms = (time.monotonic() - t0) * 1000
        await ws.send(json.dumps({"cmd": "stop"}))
        await asyncio.sleep(2.5)  # the server flushes what is left when it stops
        task.cancel()
    audio_ms = len(pcm) / 2 / 16
    texts = [t for _, t in events if t]
    first_ms = events[0][0] if events else None
    final = texts[-1] if texts else ""
    # flicker: an update that does not start with the words already shown
    flicker = 0
    shown = []
    for _, t in events:
        w = norm(t)
        if shown and w[: len(shown)] != shown[: len(w)] and not (len(w) >= len(shown) and w[: len(shown)] == shown):
            flicker += 1
        shown = w
    # per-word lag: when did word k first appear (as the k-th word of an update), minus when it began to be spoken
    lags = []
    for k, wd in enumerate(meta.get("words", [])):
        appeared = next((ms for ms, t in events if len(norm(t)) > k), None)
        if appeared is not None:
            lags.append((wd["word"], appeared - wd["startMs"]))
    batch_text, batch_ms = batch(path, port)
    ref = meta.get("text", "")
    return {
        "file": path, "audio_ms": round(audio_ms), "updates": len(events), "first_caption_ms": None if first_ms is None else round(first_ms),
        "end_of_speech_ms": round(end_of_speech_ms), "flicker_updates": flicker, "final_caption": final, "reference": ref,
        "wer_stream": None if not ref else round(wer(ref, final), 3), "batch_text": batch_text, "batch_ms": round(batch_ms),
        "wer_batch": None if not ref else round(wer(ref, batch_text), 3),
        "word_lag_ms": [(w, round(l)) for w, l in lags],
        "lag_median_ms": round(sorted(l for _, l in lags)[len(lags) // 2]) if lags else None,
        "lag_max_ms": round(max((l for _, l in lags), default=0)) if lags else None,
        "timeline": [(round(ms), t) for ms, t in events][:60],
    }


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="+")
    ap.add_argument("--port", type=int, default=39217)
    ap.add_argument("--json")
    a = ap.parse_args()
    out = []
    for f in a.files:
        r = await run_one(f, a.port)
        out.append(r)
        print(f"\n== {f}  ({r['audio_ms']} ms of speech)")
        print(f"   first caption after {r['first_caption_ms']} ms; {r['updates']} updates; flicker updates: {r['flicker_updates']}")
        print(f"   word lag (ms after the word began): median {r['lag_median_ms']}, worst {r['lag_max_ms']}")
        print(f"   spoken : {r['reference']}")
        print(f"   caption: {r['final_caption']}   (WER {r['wer_stream']})")
        print(f"   batch  : {r['batch_text']}   (WER {r['wer_batch']}, {r['batch_ms']} ms)")
    if a.json:
        json.dump(out, open(a.json, "w", encoding="utf-8"), indent=2)


asyncio.run(main())
