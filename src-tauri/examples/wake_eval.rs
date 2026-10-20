//! Wake-word evaluator: runs labelled WAV clips through the REAL wake engine.
//!
//!   cargo run --release --example wake_eval -- \
//!       --manifest eval_set/manifest.jsonl --root eval_set \
//!       [--threshold 0.68] [--kinds wake,negative,room] [--stage2] \
//!       [--resources <dir containing oww/>] [--out wake_results.jsonl]
//!       [--dsp legacy|aligned]    front-end chain (default legacy = shipped behaviour)
//!
//!   cargo run --release --example wake_eval -- --selftest
//!
//! Manifest rows are JSON lines with at least `audio` (path relative to
//! `--root`) and `kind` (`wake` | `negative` | `room`); every other field
//! (speaker_id, phrase, device_class, distance_cm, volume, ...) is copied to
//! the output so the scorecard can slice results by condition.
//!
//! `--stage2` additionally sends each triggered clip's 2.5 s verifier window to
//! Groq (needs GROQ_API_KEY) and applies the production two-key transcript gate,
//! so the output carries the FULL-gate result next to the stage-1 result.

use std::io::Write;
use std::path::{Path, PathBuf};

use nexus_lib::voice_eval as ve;
use serde_json::{json, Map, Value};

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
    let code = rt.block_on(run(args));
    std::process::exit(code);
}

async fn run(args: Vec<String>) -> i32 {
    let resources = arg(&args, "--resources").map(PathBuf::from);
    let threshold = arg(&args, "--threshold").and_then(|t| t.parse::<f32>().ok());
    let mut engine = match ve::new_wake_engine(resources.as_deref()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cannot build the wake engine: {e}");
            return 2;
        }
    };

    let dsp = arg(&args, "--dsp").unwrap_or_else(|| "legacy".to_string());
    engine.set_dsp(&dsp);

    if flag(&args, "--selftest") {
        return selftest(&mut engine, resources.as_deref(), threshold).await;
    }

    let (Some(manifest), Some(root)) = (arg(&args, "--manifest"), arg(&args, "--root")) else {
        eprintln!("usage: wake_eval --manifest <jsonl> --root <dir> [--threshold T] [--kinds a,b] [--stage2] [--out file]  |  --selftest");
        return 2;
    };
    let kinds: Vec<String> = arg(&args, "--kinds")
        .unwrap_or_else(|| "wake,negative,room".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let stage2 = flag(&args, "--stage2");
    let groq_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
    if stage2 && groq_key.is_empty() {
        eprintln!("--stage2 needs GROQ_API_KEY in the environment");
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

    let effective_thr = threshold.unwrap_or(engine.threshold);
    let mut n = 0usize;
    for (ln, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(Value::Object(mut row)) = serde_json::from_str::<Value>(line) else {
            eprintln!("manifest line {}: not a JSON object, skipped", ln + 1);
            continue;
        };
        let kind = row.get("kind").and_then(Value::as_str).unwrap_or("").to_string();
        if !kinds.contains(&kind) {
            continue;
        }
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
        let mut res = Map::new();
        res.insert("threshold".into(), json!(effective_thr));
        res.insert("dsp".into(), json!(dsp));
        res.insert("duration_ms".into(), json!(audio.len() as u64 * 1000 / ve::SR as u64));
        if kind == "room" {
            // Long recording: stream it (no padding / per-clip reset) and count triggers.
            ve::reset_engine_state(&mut engine);
            let run = ve::run_wake_engine(&mut engine, &audio, threshold);
            res.insert("n_fires".into(), json!(run.fires.len()));
            res.insert("fires_ms".into(), json!(run.fires.iter().map(|f| f.t_ms).collect::<Vec<_>>()));
            res.insert("fire_probs".into(), json!(run.fires.iter().map(|f| f.prob).collect::<Vec<_>>()));
            res.insert("max_prob".into(), json!(run.max_prob));
        } else {
            let v = ve::eval_wake_clip(&mut engine, &audio, threshold);
            res.insert("fired".into(), json!(v.fired));
            res.insert("n_fires".into(), json!(v.n_fires));
            res.insert("first_fire_ms".into(), json!(v.first_fire_ms));
            res.insert("onset_ms".into(), json!(v.onset_ms));
            res.insert(
                "fire_after_onset_ms".into(),
                json!(match (v.first_fire_ms, v.onset_ms) {
                    (Some(f), Some(o)) => Some(f.saturating_sub(o)),
                    _ => None,
                }),
            );
            res.insert("first_fire_prob".into(), json!(v.first_fire_prob));
            res.insert("max_prob".into(), json!(v.max_prob));
            if stage2 && v.fired {
                let samples = ve::stage2_samples(&v.stage2_window);
                let started = std::time::Instant::now();
                match ve::transcribe(&samples, ve::SttEngine::Groq, ve::Bias::Neutral, &groq_key, &client).await {
                    Ok(t) => {
                        let prob = v.first_fire_prob.unwrap_or(0.0);
                        res.insert("stage2_text".into(), json!(t));
                        res.insert("stage2_pass".into(), json!(ve::stage2_gate(&t, prob)));
                    }
                    Err(e) => {
                        // Production fails OPEN on an STT error; record that explicitly.
                        res.insert("stage2_error".into(), json!(e));
                        res.insert("stage2_pass".into(), json!(true));
                        res.insert("stage2_failed_open".into(), json!(true));
                    }
                }
                res.insert("stage2_ms".into(), json!(started.elapsed().as_millis() as u64));
                // Groq free tier: 20 requests/minute.
                tokio::time::sleep(std::time::Duration::from_millis(3200)).await;
            }
        }
        for (k, val) in res {
            row.insert(k, val);
        }
        writeln!(out, "{}", Value::Object(row)).ok();
        n += 1;
        if n % 25 == 0 {
            eprintln!("... {n} clips evaluated");
        }
    }
    eprintln!("done: {n} clips evaluated (threshold {effective_thr})");
    0
}

/// Positive-control sanity check with the bundled offline Piper voice.
/// Synthetic voice: proves the harness can detect speech; NOT an accuracy number.
async fn selftest(engine: &mut nexus_lib::voice_eval::WakeEngine, resources: Option<&Path>, thr: Option<f32>) -> i32 {
    let phrases = ["nexus", "hey nexus", "okay nexus", "next us", "lexus", "hello there", "open chrome"];
    let clips = match ve::synth_controls(resources, &phrases).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("selftest cannot synthesize controls: {e}");
            return 2;
        }
    };
    println!("{:<14} {:>7} {:>6} {:>8}", "phrase", "fired", "fires", "maxprob");
    for (phrase, audio) in clips {
        let v = ve::eval_wake_clip(engine, &audio, thr);
        println!("{:<14} {:>7} {:>6} {:>8.3}", phrase, v.fired, v.n_fires, v.max_prob);
    }
    println!("(synthetic Piper voice: a harness sanity check, not an accuracy measurement)");
    0
}
