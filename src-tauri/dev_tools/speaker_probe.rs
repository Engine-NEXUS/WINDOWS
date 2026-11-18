//! Speaker probe: how well the REAL Voice Lock speaker check tells the owner from other voices.
//!
//! For each owner recording session it enrolls exactly as the app does (5 clips enrolled, 5 held
//! out, threshold from `calibrate_owner_threshold`), then scores the owner's clips from the OTHER
//! sessions and every other-voice clip in the manifest. Same embedding path as live Voice Lock
//! (`wake_eval::speaker_embeddings`), so these numbers describe the shipped code.
//!
//! Usage (from src-tauri; --release, debug is ~20x slower):
//!   cargo run --release --example speaker_probe -- --manifest ../wake_word_data/manifest.jsonl
//!   ... --speech-commands ../wake_word_data/public/raw/speech-commands --max-people 2000
//!   ... --json ../docs/testing/runs/speaker-probe.json
//!
//! Other voices: TTS speakers from the manifest, and with --speech-commands real people from Google
//! Speech Commands (one clip each). Played-back recordings, not a live impostor in the room. Read-only:
//! audio never leaves the machine; only --json is written (scores and file names, no audio).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nexus_lib::wake_eval;
use serde::Serialize;

#[derive(Clone)]
struct Clip {
    path: PathBuf,
    speaker: String,
    session: String,
    label: String,
}

#[derive(Serialize)]
struct SessionResult {
    enroll_session: String,
    threshold: Option<f32>,
    calibration_error: Option<String>,
    owner_trials: usize,
    owner_rejected: usize,
    frr: f64,
    other_trials: usize,
    other_accepted: usize,
    far: f64,
}

#[derive(Serialize)]
struct Report {
    owner_clips: usize,
    other_clips: usize,
    other_speakers: usize,
    embedding_failures: usize,
    sessions: Vec<SessionResult>,
    pooled_eer: f64,
    pooled_eer_threshold: f32,
    owner_score_median: f32,
    other_score_median: f32,
    worst_other_speakers: Vec<(String, f64)>,
}

fn main() -> anyhow::Result<()> {
    let mut manifest = PathBuf::from("../wake_word_data/manifest.jsonl");
    let mut limit_per_speaker = 60usize;
    let mut json: Option<PathBuf> = None;
    let mut speech_commands: Option<PathBuf> = None;
    let mut max_people = 2000usize;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--manifest" => manifest = it.next().map(PathBuf::from).expect("--manifest FILE"),
            "--limit-per-speaker" => limit_per_speaker = it.next().and_then(|v| v.parse().ok()).expect("--limit-per-speaker N"),
            "--json" => json = it.next().map(PathBuf::from),
            "--speech-commands" => speech_commands = it.next().map(PathBuf::from),
            "--max-people" => max_people = it.next().and_then(|v| v.parse().ok()).expect("--max-people N"),
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    let root = manifest.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut clips = load_clips(&manifest, &root, limit_per_speaker)?;
    if let Some(dir) = &speech_commands {
        clips.extend(load_speech_commands(dir, max_people)?);
    }
    let (owner, other): (Vec<Clip>, Vec<Clip>) = clips.into_iter().partition(|c| c.speaker == "owner");
    eprintln!("owner clips {}, other-voice clips {}", owner.len(), other.len());

    let mut failures = 0usize;
    let owner_emb = embed(&owner, &mut failures)?;
    let other_emb = embed(&other, &mut failures)?;

    let mut sessions: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, c) in owner.iter().enumerate() {
        if c.label == "positive" && owner_emb[i].is_some() {
            sessions.entry(c.session.clone()).or_default().push(i);
        }
    }

    let mut results = Vec::new();
    let mut owner_scores = Vec::new();
    let mut other_scores = Vec::new();
    let mut per_speaker: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (session, idx) in &sessions {
        if idx.len() < 10 {
            continue;
        }
        let enrolled: Vec<Vec<f32>> = idx[..5].iter().map(|&i| owner_emb[i].clone().unwrap()).collect();
        let held: Vec<Vec<f32>> = idx[5..10].iter().map(|&i| owner_emb[i].clone().unwrap()).collect();
        let (threshold, calibration_error) = match wake_eval::speaker_threshold(&enrolled, &held) {
            Ok(t) => (Some(t), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let thr = threshold.unwrap_or(f32::INFINITY);
        let (mut ot, mut orj) = (0, 0);
        for (i, c) in owner.iter().enumerate() {
            if c.session == *session {
                continue;
            }
            let Some(e) = &owner_emb[i] else { continue };
            let s = wake_eval::speaker_score(&enrolled, e);
            owner_scores.push(s);
            ot += 1;
            if s < thr {
                orj += 1;
            }
        }
        let (mut xt, mut xa) = (0, 0);
        for (i, c) in other.iter().enumerate() {
            let Some(e) = &other_emb[i] else { continue };
            let s = wake_eval::speaker_score(&enrolled, e);
            other_scores.push(s);
            xt += 1;
            let entry = per_speaker.entry(c.speaker.clone()).or_default();
            entry.0 += 1;
            if s >= thr {
                xa += 1;
                entry.1 += 1;
            }
        }
        results.push(SessionResult {
            enroll_session: session.clone(),
            threshold,
            calibration_error,
            owner_trials: ot,
            owner_rejected: orj,
            frr: ratio(orj, ot),
            other_trials: xt,
            other_accepted: xa,
            far: ratio(xa, xt),
        });
    }

    let (eer, eer_t) = eer(&owner_scores, &other_scores);
    let mut worst: Vec<(String, f64)> = per_speaker.into_iter().map(|(k, (n, a))| (k, ratio(a, n))).collect();
    worst.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    worst.truncate(8);
    let report = Report {
        owner_clips: owner.len(),
        other_clips: other.len(),
        other_speakers: other.iter().map(|c| c.speaker.as_str()).collect::<std::collections::BTreeSet<_>>().len(),
        embedding_failures: failures,
        sessions: results,
        pooled_eer: eer,
        pooled_eer_threshold: eer_t,
        owner_score_median: median(&owner_scores),
        other_score_median: median(&other_scores),
        worst_other_speakers: worst,
    };

    println!("enroll session        thr    owner FRR          other-voice FAR");
    for r in &report.sessions {
        match r.threshold {
            Some(t) => println!(
                "{:<20} {:.3}  {:>4}/{:<5} {:>5.1}%   {:>4}/{:<5} {:>5.1}%",
                r.enroll_session, t, r.owner_rejected, r.owner_trials, r.frr * 100.0, r.other_accepted, r.other_trials, r.far * 100.0
            ),
            None => println!("{:<20} calibration refused: {}", r.enroll_session, r.calibration_error.as_deref().unwrap_or("")),
        }
    }
    println!(
        "pooled EER {:.1}% at {:.3}; median score owner {:.3}, other voices {:.3}",
        report.pooled_eer * 100.0, report.pooled_eer_threshold, report.owner_score_median, report.other_score_median
    );
    println!("most accepted other voices: {:?}", report.worst_other_speakers);
    if let Some(p) = json {
        std::fs::write(&p, serde_json::to_string_pretty(&report)?)?;
        eprintln!("wrote {}", p.display());
    }
    Ok(())
}

fn load_clips(manifest: &Path, root: &Path, limit_per_speaker: usize) -> anyhow::Result<Vec<Clip>> {
    let mut out = Vec::new();
    let mut per: BTreeMap<String, usize> = BTreeMap::new();
    for line in std::fs::read_to_string(manifest)?.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let (origin, label, speaker) = (s("origin"), s("label"), s("speaker_id"));
        let owner = origin == "human" && speaker == "owner";
        let other = origin == "tts" && label != "background";
        if !(owner || other) {
            continue;
        }
        if other {
            let n = per.entry(speaker.clone()).or_default();
            if *n >= limit_per_speaker {
                continue;
            }
            *n += 1;
        }
        let path = root.join(s("audio"));
        if path.exists() {
            out.push(Clip { path, speaker, session: s("session_id"), label });
        }
    }
    Ok(out)
}

/// Google Speech Commands (CC-BY 4.0): one-second words from thousands of real people. The file
/// name starts with an anonymous speaker id; one clip per person, words rotated so the set mixes words.
fn load_speech_commands(dir: &Path, max_people: usize) -> anyhow::Result<Vec<Clip>> {
    let mut words: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && !p.file_name().map_or(true, |n| n.to_string_lossy().starts_with('_')))
        .collect();
    words.sort();
    let mut lists: Vec<Vec<PathBuf>> = words
        .iter()
        .map(|w| {
            let mut f: Vec<PathBuf> = std::fs::read_dir(w)
                .map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().map_or(false, |x| x == "wav")).collect())
                .unwrap_or_default();
            f.sort();
            f
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    let mut round = 0usize;
    while out.len() < max_people && lists.iter().any(|l| round < l.len()) {
        for l in lists.iter_mut() {
            if out.len() >= max_people {
                break;
            }
            let Some(p) = l.get(round) else { continue };
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let speaker = format!("sc:{}", name.split("_nohash").next().unwrap_or(&name));
            if seen.insert(speaker.clone()) {
                out.push(Clip { path: p.clone(), speaker, session: String::new(), label: "word".into() });
            }
        }
        round += 1;
    }
    Ok(out)
}

fn embed(clips: &[Clip], failures: &mut usize) -> anyhow::Result<Vec<Option<Vec<f32>>>> {
    let audio: Vec<Vec<f32>> = clips
        .iter()
        .map(|c| match wake_eval::read_wav(&c.path) {
            Ok(w) => wake_eval::resample_linear(&w.samples, w.sample_rate, 16_000),
            Err(_) => Vec::new(),
        })
        .collect();
    let emb = wake_eval::speaker_embeddings(&audio)?;
    *failures += emb.iter().filter(|e| e.is_none()).count();
    Ok(emb)
}

fn ratio(a: usize, n: usize) -> f64 {
    if n == 0 { 0.0 } else { a as f64 / n as f64 }
}

fn median(v: &[f32]) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[s.len() / 2]
}

/// Equal error rate over pooled scores: the threshold where owner rejections equal other accepts.
fn eer(owner: &[f32], other: &[f32]) -> (f64, f32) {
    if owner.is_empty() || other.is_empty() {
        return (f64::NAN, f32::NAN);
    }
    let mut best = (f64::INFINITY, 1.0f64, 0.0f32);
    for step in 0..=400 {
        let t = -1.0 + step as f32 * 0.005;
        let frr = owner.iter().filter(|&&s| s < t).count() as f64 / owner.len() as f64;
        let far = other.iter().filter(|&&s| s >= t).count() as f64 / other.len() as f64;
        let gap = (frr - far).abs();
        if gap < best.0 {
            best = (gap, (frr + far) / 2.0, t);
        }
    }
    (best.1, best.2)
}
