//! TEST HOOK, debug builds only (the module is not compiled into a release): plays a speech file through the REAL
//! microphone path as if someone were talking, so the capture, the endpointing and the live captions can be tested without a
//! person or a loudspeaker.
//!
//! It is off unless the environment variable `NEXUS_FAKE_MIC_DIR` names a folder. While it is set, every `*.wav` that appears
//! in that folder (16 kHz, mono, 16-bit, as made by `scripts/make-test-speech.ps1`) is queued and renamed to `*.wav.played`.
//! The microphone callback then REPLACES the microphone's own samples with the queued speech, 1280 samples at a time at the
//! real rate (the callback's own pace), so the audio travels exactly the way microphone audio does. Nothing is ever played
//! aloud and nothing is recorded from a real microphone by this module.

use std::collections::VecDeque;
use std::sync::Mutex;

static QUEUE: Mutex<VecDeque<f32>> = Mutex::new(VecDeque::new());

fn dir() -> Option<std::path::PathBuf> {
    std::env::var_os("NEXUS_FAKE_MIC_DIR").map(std::path::PathBuf::from)
}

/// 16 kHz mono 16-bit PCM WAV bytes to samples in -1..1. Anything else is refused (a test must not silently play garbage).
pub(crate) fn wav_to_samples(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let mut pos = 12;
    let mut fmt_ok = false;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]]) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            let tag = u16::from_le_bytes([bytes[body], bytes[body + 1]]);
            let ch = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            let rate = u32::from_le_bytes([bytes[body + 4], bytes[body + 5], bytes[body + 6], bytes[body + 7]]);
            let bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            if tag != 1 || ch != 1 || rate != 16000 || bits != 16 {
                return Err(format!("need 16 kHz mono 16-bit PCM, got tag {tag}, {ch} ch, {rate} Hz, {bits} bit"));
            }
            fmt_ok = true;
        } else if id == b"data" {
            if !fmt_ok {
                return Err("data before fmt".into());
            }
            let end = (body + len).min(bytes.len());
            return Ok(bytes[body..end].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect());
        }
        pos = body + len + (len & 1);
    }
    Err("no data chunk".into())
}

/// Called by the microphone callback with the samples it just produced (16 kHz): while speech is queued, they are replaced.
pub(crate) fn overlay(produced: &mut Vec<f32>) {
    let mut q = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    if q.is_empty() {
        return;
    }
    for s in produced.iter_mut() {
        match q.pop_front() {
            Some(v) => *s = v,
            None => break,
        }
    }
}

fn queued_samples() -> usize {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner()).len()
}

/// Starts the folder watcher once, only when `NEXUS_FAKE_MIC_DIR` is set.
pub(crate) fn start() {
    let Some(folder) = dir() else { return };
    let _ = std::fs::create_dir_all(&folder);
    println!("[FAKE-MIC] test speech from {} is played through the microphone path (debug build only)", folder.display());
    std::thread::Builder::new()
        .name("fake-mic-watch".into())
        .spawn(move || loop {
            if let Ok(rd) = std::fs::read_dir(&folder) {
                let mut found: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |x| x == "wav")).collect();
                found.sort();
                for p in found {
                    match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| wav_to_samples(&b)) {
                        Ok(samples) => {
                            println!("[FAKE-MIC] playing {} ({} ms of speech) into the microphone path", p.display(), samples.len() / 16);
                            QUEUE.lock().unwrap_or_else(|e| e.into_inner()).extend(samples);
                        }
                        Err(e) => println!("[FAKE-MIC] {} refused: {e}", p.display()),
                    }
                    let _ = std::fs::rename(&p, p.with_extension("wav.played"));
                    // one file at a time: wait until it has been played
                    while queued_samples() > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, ch: u16, bits: u16, data: &[i16]) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend((36 + data.len() as u32 * 2).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(ch.to_le_bytes());
        v.extend(rate.to_le_bytes());
        v.extend((rate * ch as u32 * bits as u32 / 8).to_le_bytes());
        v.extend((ch * bits / 8).to_le_bytes());
        v.extend(bits.to_le_bytes());
        v.extend(b"data");
        v.extend((data.len() as u32 * 2).to_le_bytes());
        for s in data {
            v.extend(s.to_le_bytes());
        }
        v
    }

    #[test]
    fn a_16k_mono_wav_is_read_and_anything_else_is_refused() {
        let ok = wav_to_samples(&wav(16000, 1, 16, &[0, 16384, -16384])).unwrap();
        assert_eq!(ok, vec![0.0, 0.5, -0.5]);
        assert!(wav_to_samples(&wav(44100, 1, 16, &[0])).is_err());
        assert!(wav_to_samples(&wav(16000, 2, 16, &[0, 0])).is_err());
        assert!(wav_to_samples(b"not a wav").is_err());
    }

    #[test]
    fn queued_speech_replaces_the_microphone_samples_and_then_stops() {
        QUEUE.lock().unwrap().clear();
        QUEUE.lock().unwrap().extend([0.9f32, 0.8, 0.7]);
        let mut chunk = vec![0.1f32; 5];
        overlay(&mut chunk);
        assert_eq!(chunk, vec![0.9, 0.8, 0.7, 0.1, 0.1], "the speech replaces the head, the rest is the real microphone");
        let mut again = vec![0.2f32; 2];
        overlay(&mut again);
        assert_eq!(again, vec![0.2, 0.2], "once played, the microphone is untouched");
    }
}
