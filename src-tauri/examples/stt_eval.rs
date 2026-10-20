//! STT evaluator: labelled WAV clips (or raw transcripts) through the REAL
//! production speech-to-text text pipeline.
//!
//!   GROQ_API_KEY=... cargo run --release --example stt_eval -- \
//!       --manifest eval_set/manifest.jsonl --root eval_set \
//!       [--engine groq|local] [--bias owner|commands|neutral] [--kinds command,silence,noise] \
//!       [--out stt_results.jsonl]
//!
//!   cargo run --release --example stt_eval -- --text-only --manifest rows.jsonl
//!
//!   # one-breath rows ("nexus open chrome"): feed STT only what the command
//!   # capture would hear after the REAL wake engine fires; sweep the offset
//!   # (ms from the detection point; default production value 0) to choose wakeSeedOffsetMs
//!   cargo run --release --example stt_eval -- --manifest M --root R --capture-offset-ms -160
//!
//! Manifest rows are JSON lines:
//!   kind              command | silence | noise | text
//!   audio             WAV path relative to --root          (not for text-only)
//!   transcript        raw STT hypothesis                    (text-only rows)
//!   expected_text     what the speaker said (for WER)
//!   expected_intent   resolved intent label ("open_app", ... ; "none" for noise)
//! Every other field is copied through so the scorecard can slice by speaker,
//! device, distance, volume and phrase.
//!
//! Stages applied, in production order: acoustic pre-gate (200 ms / RMS 0.005) ->
//! STT call (Groq or local Moonshine, with the chosen decoder bias) -> the real
//! hallucination filter -> wake-prefix strip + deterministic parser.
//! `--text-only` skips the first two and scores supplied hypotheses, which is how
//! filter / parser regressions are checked without a microphone or a key.

use std::io::Write;
use std::path::{Path, PathBuf};

use nexus_lib::voice_eval as ve;
use serde_json::{json, Value};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    std::process::exit(rt.block_on(run(args)));
}

async fn run(args: Vec<String>) -> i32 {
    let Some(manifest) = arg(&args, "--manifest") else {
        eprintln!("usage: stt_eval --manifest <jsonl> [--root <dir>] [--engine groq|local] [--bias owner|commands|neutral] [--kinds a,b] [--text-only] [--out file]");
        return 2;
    };
    let root = arg(&args, "--root").map(PathBuf::from).unwrap_or_default();
    let text_only = flag(&args, "--text-only");
    let engine = match arg(&args, "--engine").as_deref() {
        Some("local") => ve::SttEngine::Local,
        Some("groq") | None => ve::SttEngine::Groq,
        Some(other) => {
            eprintln!("unknown --engine {other} (groq|local)");
            return 2;
        }
    };
    let bias = match arg(&args, "--bias").as_deref() {
        Some("owner") => ve::Bias::Owner,
        Some("commands") => ve::Bias::Commands,
        Some("neutral") | None => ve::Bias::Neutral,
        Some(other) => {
            eprintln!("unknown --bias {other} (owner|commands|neutral)");
            return 2;
        }
    };
    let capture_offset: Option<i32> = arg(&args, "--capture-offset-ms").and_then(|v| v.parse().ok());
    let wake_threshold = arg(&args, "--threshold").and_then(|t| t.parse::<f32>().ok());
    let mut wake_engine = if capture_offset.is_some() && !text_only {
        match ve::new_wake_engine(arg(&args, "--resources").map(PathBuf::from).as_deref()) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("--capture-offset-ms needs the wake engine: {e}");
                return 2;
            }
        }
    } else {
        None
    };
    let kinds: Vec<String> = arg(&args, "--kinds")
        .unwrap_or_else(|| "command,silence,noise,text".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let groq_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
    if !text_only && engine == ve::SttEngine::Groq && groq_key.is_empty() {
        eprintln!("--engine groq needs GROQ_API_KEY (or use --engine local / --text-only)");
        return 2;
    }
    let client = reqwest::Client::new();

    let text = match std::fs::read_to_string(&manifest) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{manifest}: {e}");
            return 2;
        }
    };
    let mut out: Box<dyn Write> = match arg(&args, "--out") {
        Some(p) => Box::new(std::fs::File::create(p).expect("create --out file")),
        None => Box::new(std::io::stdout()),
    };

    let mut n = 0usize;
    for (ln, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(Value::Object(mut row)) = serde_json::from_str::<Value>(line) else {
            eprintln!("manifest line {}: not a JSON object, skipped", ln + 1);
            continue;
        };
        let kind = row.get("kind").and_then(Value::as_str).unwrap_or("command").to_string();
        if !kinds.contains(&kind) {
            continue;
        }

        let mut stats = ve::SegStats::default();
        let mut gate: Option<&'static str> = None;
        let mut stt_error: Option<String> = None;
        let mut stt_ms: Option<u64> = None;
        let raw: String = if text_only {
            row.get("transcript").and_then(Value::as_str).unwrap_or("").to_string()
        } else {
            let Some(rel) = row.get("audio").and_then(Value::as_str).map(str::to_string) else {
                eprintln!("manifest line {}: no `audio`, skipped", ln + 1);
                continue;
            };
            let audio = match ve::load_audio_16k(&Path::new(&root).join(&rel)) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("{e}");
                    row.insert("error".into(), json!(e));
                    writeln!(out, "{}", Value::Object(row)).ok();
                    continue;
                }
            };
            // One-breath rows: replace the clip by what the capture would receive.
            let mut audio = audio;
            if let (Some(off), Some(eng)) = (capture_offset, wake_engine.as_mut()) {
                if row.get("one_breath").and_then(Value::as_bool) == Some(true) {
                    row.insert("capture_offset_ms".into(), json!(off));
                    match ve::simulate_capture(eng, &audio, wake_threshold, off) {
                        Some(a) => audio = a,
                        None => {
                            // The wake word never fired: production would do nothing.
                            row.insert("wake_missed".into(), json!(true));
                            audio = Vec::new();
                        }
                    }
                }
            }
            let samples = ve::to_i16(&audio);
            gate = ve::acoustic_gate(&samples);
            if gate.is_some() {
                String::new()
            } else {
                let started = std::time::Instant::now();
                let r = ve::transcribe_verbose(&samples, engine, bias, &groq_key, &client).await;
                stt_ms = Some(started.elapsed().as_millis() as u64);
                if engine == ve::SttEngine::Groq {
                    // Groq free tier: 20 requests/minute.
                    tokio::time::sleep(std::time::Duration::from_millis(3200)).await;
                }
                match r {
                    Ok((t, st)) => {
                        stats = st;
                        t
                    }
                    Err(e) => {
                        stt_error = Some(e);
                        String::new()
                    }
                }
            }
        };

        let t = ve::text_stage(&raw);
        let expected_text = row.get("expected_text").and_then(Value::as_str).map(str::to_string);
        let expected_intent = row.get("expected_intent").and_then(Value::as_str).map(str::to_string);

        row.insert("raw".into(), json!(t.raw));
        row.insert("filtered".into(), json!(t.filtered));
        row.insert("dropped_by_filter".into(), json!(t.dropped_by_filter));
        row.insert("intent".into(), json!(t.intent));
        if let Some(g) = gate {
            row.insert("acoustic_gate".into(), json!(g));
        }
        if stats.n_segments > 0 {
            // Inputs of the (off-by-default) confidence gate; the scorecard sweeps them.
            row.insert("n_segments".into(), json!(stats.n_segments));
            row.insert("min_avg_logprob".into(), json!(stats.min_avg_logprob));
            row.insert("max_avg_logprob".into(), json!(stats.max_avg_logprob));
            row.insert("max_compression_ratio".into(), json!(stats.max_compression_ratio));
            row.insert("max_no_speech_prob".into(), json!(stats.max_no_speech_prob));
            row.insert("gate_rejects".into(), json!(stats.gate_rejects));
        }
        if let Some(e) = stt_error {
            row.insert("stt_error".into(), json!(e));
        }
        if let Some(ms) = stt_ms {
            row.insert("stt_ms".into(), json!(ms));
        }
        if let Some(exp) = expected_text {
            // WER is scored on the RAW hypothesis (what the engine heard), and the
            // filtered text separately, so filter damage is visible on its own.
            let (edits_raw, ref_words) = ve::word_edits(&exp, &t.raw);
            let (edits_filtered, _) = ve::word_edits(&exp, &t.filtered);
            row.insert("ref_words".into(), json!(ref_words));
            row.insert("edits_raw".into(), json!(edits_raw));
            row.insert("edits_filtered".into(), json!(edits_filtered));
        }
        if let Some(exp) = expected_intent {
            row.insert("intent_ok".into(), json!(exp == t.intent));
        }
        // Silence/noise clips: ANY surviving text is a hallucination leak.
        if kind == "silence" || kind == "noise" {
            row.insert("leaked".into(), json!(!t.filtered.trim().is_empty()));
        }
        writeln!(out, "{}", Value::Object(row)).ok();
        n += 1;
        if n % 25 == 0 {
            eprintln!("... {n} rows evaluated");
        }
    }
    eprintln!("done: {n} rows evaluated");
    0
}
