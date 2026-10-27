"""Consented one-clip diagnostic: decrypt only in memory, compare Groq prompts.

No key, PCM, WAV or decrypted record is printed or written to disk. Uses only
the named NEXUS credentials; never enumerates the credential store. Replays
the consented clip to the existing Groq transcription provider, not a new service.
"""
import array
import base64
import ctypes
from ctypes import wintypes as w
import hashlib
import io
import json
import os
from pathlib import Path
import re
import time
import urllib.request
import urllib.error
import uuid
import wave
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
import numpy as np


class Credential(ctypes.Structure):
    _fields_ = [('Flags', w.DWORD), ('Type', w.DWORD), ('TargetName', w.LPWSTR),
                ('Comment', w.LPWSTR), ('LastWritten', w.FILETIME),
                ('CredentialBlobSize', w.DWORD), ('CredentialBlob', ctypes.POINTER(w.BYTE)),
                ('Persist', w.DWORD), ('AttributeCount', w.DWORD), ('Attributes', ctypes.c_void_p),
                ('TargetAlias', w.LPWSTR), ('UserName', w.LPWSTR)]


def secret(user):
    lib = ctypes.WinDLL('Advapi32.dll', use_last_error=True)
    read = lib.CredReadW
    read.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, ctypes.POINTER(ctypes.POINTER(Credential))]
    read.restype = w.BOOL
    lib.CredFree.argtypes = [ctypes.c_void_p]
    pointer = ctypes.POINTER(Credential)()
    if not read(user + '.com.nexus.assistant', 1, 0, ctypes.byref(pointer)):
        raise RuntimeError('Named NEXUS credential unavailable')
    try:
        return ctypes.string_at(pointer.contents.CredentialBlob,
                                pointer.contents.CredentialBlobSize).decode('utf-16-le')
    finally:
        lib.CredFree(pointer)


def transcribe(wav, key, prompt):
    boundary = 'NexusDiagnostic' + uuid.uuid4().hex
    parts = []
    for name, value in [('model', 'whisper-large-v3-turbo'), ('language', 'en'),
                        ('temperature', '0'), ('response_format', 'json'), ('prompt', prompt)]:
        parts.append((f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n').encode())
    parts += [(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="audio.wav"\r\nContent-Type: audio/wav\r\n\r\n').encode(), wav,
              f'\r\n--{boundary}--\r\n'.encode()]
    request = urllib.request.Request('https://api.groq.com/openai/v1/audio/transcriptions',
                data=b''.join(parts), headers={'Authorization': 'Bearer ' + key,
                'Content-Type': 'multipart/form-data; boundary=' + boundary,
                'User-Agent':'NEXUS-one-clip-diagnostic/1.0'})
    started = time.monotonic()
    with urllib.request.urlopen(request, timeout=30) as response:
        text = json.load(response).get('text', '')
    return {'text': text, 'elapsedMs': round((time.monotonic()-started)*1000)}


def main():
    folder = Path(os.environ['APPDATA'])/'com.nexus.assistant/stt_diagnostics'
    # Exact file created during the explicitly consented one-clip session.
    path = folder/'1792953656448-6.nxm'
    blob = path.read_bytes()
    if blob[:4] != b'NXM1':
        raise RuntimeError('Expected sealed NEXUS diagnostic')
    key = bytes.fromhex(secret('memcore_data_key_v1'))
    record = json.loads(AESGCM(key).decrypt(blob[4:16], blob[16:], b'NXM1'))
    if record['session'] != 6 or record['captured_at'] != 1792953656:
        raise RuntimeError('Unexpected diagnostic identity')
    pcm = base64.b64decode(record['audio_pcm16_b64'], validate=True)
    values = np.frombuffer(pcm, dtype='<i2').astype(np.float64)/32768
    frames = values[:len(values)//320*320].reshape(-1,320)
    energy = np.sqrt(np.mean(frames**2,axis=1))
    active = energy > max(0.015, float(energy.max())*0.12)
    islands=[]
    for i,value in enumerate(active):
        if value and (i==0 or not active[i-1]): start=i
        if value and (i+1==len(active) or not active[i+1]): islands.append([round(start*.02,2), round((i+1)*.02,2)])
    buffer=io.BytesIO()
    with wave.open(buffer,'wb') as output:
        output.setnchannels(1);output.setsampwidth(2);output.setframerate(16000);output.writeframes(pcm)
    wav=buffer.getvalue()
    source=Path('C:/Users/Chitkul Lakshya/.codex/worktrees/windows-readiness/ULTRON/src-tauri/src/stt_groq.rs').read_text()
    match=re.search(r'pub const NEXUS_VOCABULARY: &str = "(.*?)";',source,re.S)
    owner=re.sub(r'\\\r?\n','',match.group(1))
    api_key=secret('nexus-apikey-groq')
    # Same audio, same model/language/temperature; vary only prompt. Two runs
    # per condition expose simple nondeterminism without tuning any threshold.
    results=[]
    for name,prompt in [('neutral','Transcribe this English audio exactly as spoken.'),('owner',owner)]:
        for repeat in range(2): results.append({'condition':name,'repeat':repeat+1,**transcribe(wav,api_key,prompt)})
    print(json.dumps({'session':6,'capturedAt':record['captured_at'], 'sealedSha256':hashlib.sha256(blob).hexdigest(),
        'pcmSha256':hashlib.sha256(pcm).hexdigest(),'sampleCount':len(values),'durationMs':round(len(values)/16),
        'peak':float(np.max(np.abs(values))),'clippingFraction':float(np.mean(np.abs(values)>=.999)),
        'energyIslandsSeconds':islands,'originalStats':record['stats'],'replays':results,
        'limits':'One diagnostic clip; not held-out accuracy, acoustic echo proof, threshold calibration or production readiness. No plaintext audio persisted.'},indent=2))


if __name__ == '__main__':
    try: main()
    except urllib.error.HTTPError as error:
        print(json.dumps({'failed':True,'errorType':'HTTPError','status':error.code}))
        raise SystemExit(1)
    except Exception as error:
        # Avoid provider/credential error strings that could contain request data.
        print(json.dumps({'failed':True,'errorType':type(error).__name__}))
        raise SystemExit(1)
