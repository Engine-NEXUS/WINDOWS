//! Missed Intent Logger — logs unmatched voice transcripts to disk.
//!
//! When STT produces a transcript that cannot be matched to any deterministic,
//! NLU, or local command intent, this module records it to:
//! `%APPDATA%/com.nexus.assistant/missed_intents.jsonl`
//!
//! This provides an empirical audit trail of user commands and STT mishearings,
//! allowing continuous extension of the phonetic alias map and intent dataset.

use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_FILE_SIZE_BYTES: u64 = 5 * 1024 * 1024; // 5 MB rotation limit

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissedIntentRecord {
    pub timestamp_ms: u64,
    pub datetime: String,
    pub transcript: String,
    pub source: String,
    pub reason: String,
}

/// Resolves path to the NEXUS data dir (…/com.nexus.assistant/missed_intents.jsonl)
pub fn get_missed_intents_file_path() -> PathBuf {
    crate::app_dirs::nexus_data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("missed_intents.jsonl")
}

/// Rotates log file if it exceeds MAX_FILE_SIZE_BYTES
fn maybe_rotate_log(path: &PathBuf) {
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.len() > MAX_FILE_SIZE_BYTES {
            let mut backup_path = path.clone();
            backup_path.set_extension("old.jsonl");
            let _ = fs::rename(path, backup_path);
        }
    }
}

/// Log an unmatched or failed voice transcript to missed_intents.jsonl
pub fn log_missed_intent(transcript: &str, source: &str, reason: &str) {
    let text = transcript.trim();
    if text.is_empty() {
        return;
    }
    // Memory hygiene: this log feeds phrase suggestions and holds the user's own words. Fragments,
    // hallucinations and ambient chatter ("then the", "love us.") are noise AND needless private
    // text on disk; log only real, multi-word speech.
    if crate::memcore::hygiene::transcript_quality(text).is_err() {
        return;
    }

    let path = get_missed_intents_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    maybe_rotate_log(&path);

    let now = SystemTime::now();
    let timestamp_ms = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Simple formatted timestamp representation
    let datetime = match now.duration_since(UNIX_EPOCH) {
        Ok(dur) => {
            let secs = dur.as_secs();
            let hours = (secs / 3600) % 24;
            let mins = (secs / 60) % 60;
            let s = secs % 60;
            format!("{:02}:{:02}:{:02} UTC", hours, mins, s)
        }
        Err(_) => "unknown".to_string(),
    };

    let record = MissedIntentRecord {
        timestamp_ms,
        datetime,
        transcript: text.to_string(),
        source: source.to_string(),
        reason: reason.to_string(),
    };

    if let Ok(json_line) = serde_json::to_string(&record) {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(file, "{}", json_line);
            tracing::info!(
                "[missed_intent] logged unmatched transcript: '{}' (reason: {}, src: {})",
                text, reason, source
            );
        }
    }
}

/// Retrieve recent missed intent records (newest first).
pub fn read_recent_missed_intents(limit: usize) -> Vec<MissedIntentRecord> {
    let path = get_missed_intents_file_path();
    if !path.exists() {
        return Vec::new();
    }

    let file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for line in reader.lines().flatten() {
        if let Ok(rec) = serde_json::from_str::<MissedIntentRecord>(&line) {
            records.push(rec);
        }
    }

    records.reverse(); // Newest first
    if records.len() > limit {
        records.truncate(limit);
    }
    records
}

/// Tauri command to fetch recent missed intents
#[tauri::command]
pub async fn get_missed_intents(limit: Option<usize>) -> Result<Vec<MissedIntentRecord>, String> {
    let lim = limit.unwrap_or(50);
    Ok(read_recent_missed_intents(lim))
}

/// Summary report produced by replaying a JSONL transcript log through `parse_deterministic`.
/// Test-only: the replay is an audit tool run by hand (see the ignored live test), not part of the app.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplayReport {
    /// Total non-empty lines encountered in the log.
    pub(crate) total_lines: usize,
    /// Number of distinct transcript texts seen.
    pub(crate) unique_transcripts: usize,
    /// Number of distinct transcripts recognized by `parse_deterministic`.
    pub(crate) rescued: usize,
    /// Up to 10 distinct rescued transcripts paired with their canonical intent label.
    pub(crate) rescued_samples: Vec<(String, String)>,
    /// Up to 10 most frequent distinct transcripts still unmatched, paired with their occurrence count.
    pub(crate) still_unmatched: Vec<(String, usize)>,
}

#[cfg(test)]
#[derive(Deserialize)]
struct TranscriptOnly {
    transcript: String,
}

/// Replay a JSONL transcript log through `crate::intent_parser::parse_deterministic`.
///
/// Pure function that extracts verbatim transcripts from JSON lines, tracks occurrence
/// frequencies, evaluates each unique transcript against the deterministic parser (and its
/// phonetic normalisation pass), and aggregates rescued commands versus still-unmatched entries.
#[cfg(test)]
pub(crate) fn replay(jsonl: &str) -> ReplayReport {
    let mut total_lines = 0;
    let mut unique_order = Vec::new();
    let mut counts: HashMap<String, usize> = HashMap::new();

    for line in jsonl.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        total_lines += 1;

        let transcript = if let Ok(record) = serde_json::from_str::<TranscriptOnly>(trimmed) {
            record.transcript.trim().to_string()
        } else if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            match val.get("transcript").and_then(|v| v.as_str()) {
                Some(t) => t.trim().to_string(),
                None => continue,
            }
        } else {
            continue;
        };

        if transcript.is_empty() {
            continue;
        }

        let entry = counts.entry(transcript.clone()).or_insert(0);
        if *entry == 0 {
            unique_order.push(transcript);
        }
        *entry += 1;
    }

    let unique_transcripts = unique_order.len();
    let mut rescued = 0;
    let mut rescued_samples = Vec::new();
    let mut unmatched = Vec::new();

    for transcript in unique_order {
        if let Some(parse_res) = crate::intent_parser::parse_deterministic(&transcript) {
            let label = crate::intent_parser::intent_to_label(&parse_res.intent);
            if label != "unknown" {
                rescued += 1;
                if rescued_samples.len() < 10 {
                    rescued_samples.push((transcript, label.to_string()));
                }
                continue;
            }
        }
        let count = counts.get(&transcript).copied().unwrap_or(1);
        unmatched.push((transcript, count));
    }

    // Sort unmatched by count descending, then alphabetically for deterministic ordering
    unmatched.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if unmatched.len() > 10 {
        unmatched.truncate(10);
    }

    ReplayReport {
        total_lines,
        unique_transcripts,
        rescued,
        rescued_samples,
        still_unmatched: unmatched,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_serialization() {
        let record = MissedIntentRecord {
            timestamp_ms: 1727280000000,
            datetime: "21:00:00 UTC".to_string(),
            transcript: "goes to mode".to_string(),
            source: "orchestrator".to_string(),
            reason: "unknown_intent".to_string(),
        };
        let serialized = serde_json::to_string(&record).unwrap();
        assert!(serialized.contains("goes to mode"));
        let deserialized: MissedIntentRecord = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.transcript, "goes to mode");
    }

    /// Mishearings seen in the owner's real log (2026-10-20) now reach the right command instead of the cloud.
    #[test]
    fn mishearings_from_the_real_log_are_normalised_and_understood() {
        use crate::intent_parser::{normalize_phonetic_mishearings, parse_deterministic};
        for (heard, fixed) in [
            ("openbread.", "open brave"),
            ("Open bread.", "open brave"),
            ("open brief", "open brave"),
            ("Command hop.", "command hub"),
            ("command roker", "command hub"),
        ] {
            assert_eq!(normalize_phonetic_mishearings(heard), fixed, "{heard}");
            assert!(parse_deterministic(&normalize_phonetic_mishearings(heard)).is_some(), "{heard} -> {fixed} must parse");
        }
        // ordinary speech is left alone
        assert_eq!(normalize_phonetic_mishearings("open the door"), "open the door");
    }

    #[test]
    fn replay_counts_rescued_lines_from_an_in_memory_log() {
        let lines = [
            // 4 phrases that parse_deterministic now understands:
            r#"{"timestamp_ms":1001,"datetime":"12:00:01 UTC","transcript":"open notepad","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1002,"datetime":"12:00:02 UTC","transcript":"shut down my laptop","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1003,"datetime":"12:00:03 UTC","transcript":"play bohemian rhapsody on spotify","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1004,"datetime":"12:00:04 UTC","transcript":"what's on my calendar today","source":"orchestrator","reason":"deterministic_miss"}"#,
            // 4 phrases that must NOT turn into commands:
            r#"{"timestamp_ms":1005,"datetime":"12:00:05 UTC","transcript":"uh the weather is nice today","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1006,"datetime":"12:00:06 UTC","transcript":"thank you so much for that","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1007,"datetime":"12:00:07 UTC","transcript":"I was thinking about dinner","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1008,"datetime":"12:00:08 UTC","transcript":"mm hmm","source":"orchestrator","reason":"deterministic_miss"}"#,
            // 4 duplicates (open notepad, mm hmm x2, thank you so much for that):
            r#"{"timestamp_ms":1009,"datetime":"12:00:09 UTC","transcript":"open notepad","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1010,"datetime":"12:00:10 UTC","transcript":"mm hmm","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1011,"datetime":"12:00:11 UTC","transcript":"mm hmm","source":"orchestrator","reason":"deterministic_miss"}"#,
            r#"{"timestamp_ms":1012,"datetime":"12:00:12 UTC","transcript":"thank you so much for that","source":"orchestrator","reason":"deterministic_miss"}"#,
        ];

        let jsonl = lines.join("\n");
        let report = replay(&jsonl);

        assert_eq!(report.total_lines, 12);
        assert_eq!(report.unique_transcripts, 8);
        assert_eq!(report.rescued, 4);
        assert_eq!(report.rescued_samples.len(), 4);
        assert_eq!(report.still_unmatched.len(), 4);

        assert_eq!(
            report.rescued_samples,
            vec![
                ("open notepad".to_string(), "open_app".to_string()),
                ("shut down my laptop".to_string(), "power_action".to_string()),
                (
                    "play bohemian rhapsody on spotify".to_string(),
                    "play_music".to_string(),
                ),
                (
                    "what's on my calendar today".to_string(),
                    "calendar_agenda".to_string(),
                ),
            ]
        );

        assert_eq!(
            report.still_unmatched,
            vec![
                ("mm hmm".to_string(), 3),
                ("thank you so much for that".to_string(), 2),
                ("I was thinking about dinner".to_string(), 1),
                ("uh the weather is nice today".to_string(), 1),
            ]
        );
    }

    // Warning: the real missed-intent log records verbatim user voice transcripts
    // which may contain private speech. This live replay output is intended strictly
    // for the machine's owner during local manual debugging.
    #[test]
    #[ignore = "reads the real log on this machine; run by hand"]
    fn live_replay_of_the_real_missed_intent_log() {
        let path = get_missed_intents_file_path();
        if !path.exists() {
            println!("[PROOF] no log");
            return;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                println!("[PROOF] no log");
                return;
            }
        };

        let report = replay(&content);
        println!(
            "[PROOF] total={} unique={} rescued={}",
            report.total_lines, report.unique_transcripts, report.rescued
        );

        for (text, intent) in report.rescued_samples.iter().take(10) {
            println!("[PROOF] rescued: \"{text}\" -> {intent}");
        }

        for (text, count) in report.still_unmatched.iter().take(10) {
            println!("[PROOF] still missed x{count}: \"{text}\"");
        }
    }
}
