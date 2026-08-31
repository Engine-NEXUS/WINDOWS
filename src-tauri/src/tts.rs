use crate::meeting_detect::MeetingState;
use kokoro_micro::TtsEngine;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::State;

pub struct TtsState {
    pub engine: Arc<Mutex<Option<TtsEngine>>>,
}

#[tauri::command]
pub async fn speak_text(
    text: String,
    voice: Option<String>,
    state: State<'_, TtsState>,
    meeting: State<'_, Arc<MeetingState>>,
) -> Result<(), String> {
    tracing::info!("tts: speaking '{}'", text);
    
    // 1. Mark TTS as playing to suppress wake word self-trigger
    meeting.set_tts_playing(true);

    // 2. Synthesize audio
    let audio = {
        let mut lock = state.engine.lock().await;
        if let Some(engine) = lock.as_mut() {
            let voice_id = voice.unwrap_or_else(|| "af_sky".to_string());
            engine.synthesize_with_options(&text, Some(&voice_id), 1.0, 1.0, Some("en"))
                .map_err(|e| format!("TTS synthesis error: {}", e))?
        } else {
            // Engine isn't initialized yet
            return Err("TTS Engine not initialized".to_string());
        }
    };

    // 3. Play audio on a blocking thread (rodio needs the OutputStream to stay alive)
    let play_result = std::thread::spawn(move || {
        if let Ok((_stream, handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&handle) {
                // Kokoro output is standard 24kHz mono PCM (f32)
                let source = SamplesBuffer::new(1, 24000, audio);
                sink.append(source);
                sink.sleep_until_end();
                Ok(())
            } else {
                Err("Failed to create audio sink".to_string())
            }
        } else {
            Err("Failed to get audio output stream".to_string())
        }
    }).join().unwrap_or_else(|_| Err("Audio thread panicked".to_string()));

    // 4. Grace period for acoustic settling before resuming wake word detection
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    meeting.set_tts_playing(false);

    play_result
}
