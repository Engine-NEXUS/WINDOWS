//! Learning from corrections (Memory v2, phase M2).
//!
//! Speech recognition keeps mishearing the same words ("open brief" for "open brave"). When the user
//! corrects it - "no, I said open brave" - or simply says the command again with the one word changed,
//! that pair (heard, meant) is evidence. One sighting proves nothing; the same pair seen on two separate
//! occasions becomes a learned correction, applied to later command transcripts before they are parsed.
//!
//! Guard rails, because this rewrites what the user said:
//! * the two phrases must look alike (letters or consonant skeleton), so "open chrome" followed by
//!   "open notepad" is two commands, not a correction;
//! * a re-issued command only counts within 25 s of one that ran, an explicit cue within 2 minutes;
//! * two sightings at least 60 s apart are needed, and a rival answer for the same heard phrase with an
//!   equal count blocks promotion (see `Store::corrections_all`);
//! * learned words are applied only to transcripts that start like a command, never to ordinary talk
//!   ("give me a brief summary" is left alone);
//! * the Memory page lists every learned correction and can remove it.

use std::path::Path;
use std::sync::{Mutex, RwLock};

/// A command repeated with one word changed counts as a correction only this soon after the first.
const REISSUE_WINDOW_SECS: i64 = 25;
/// An explicit "no, I said ..." may refer to a turn up to this old.
const CUE_WINDOW_SECS: i64 = 120;
/// How alike heard and meant must be (0..1) for the pair to be plausible.
const MIN_SIMILARITY: f64 = 0.5;

/// Spoken openings that mark the next words as the intended ones. Longest first.
const CUES: &[&[&str]] = &[
    &["no", "i", "said"],
    &["no", "i", "meant"],
    &["no", "i", "mean"],
    &["no", "not", "that"],
    &["no", "it", "is"],
    &["no", "its"],
    &["no", "say"],
    &["thats", "wrong"],
    &["i", "said"],
    &["i", "meant"],
    &["i", "mean"],
    &["not", "that"],
    &["actually"],
    &["wrong"],
    &["nope"],
    &["no"],
];

/// Words that carry no identity of their own; a "correction" made only of these is noise.
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "to", "in", "on", "it", "is", "my", "me", "i", "you", "that", "this",
    "please", "thanks", "thank", "yes", "no", "ok", "okay", "now", "then", "for", "with", "up", "down",
];

/// Only transcripts that begin with one of these are rewritten; everything else is ordinary talk.
const COMMAND_VERBS: &[&str] = &[
    "open", "launch", "start", "run", "close", "quit", "play", "pause", "call", "message", "text", "send",
    "whatsapp", "search", "find", "go", "switch", "show", "read", "type",
];

// ---- Text helpers (pure) ----

/// Lowercase words without punctuation or apostrophes, with a leading wake word removed.
pub fn tokens(s: &str) -> Vec<String> {
    let cleaned: String = s
        .to_lowercase()
        .chars()
        .filter(|c| *c != '\'' && *c != '\u{2019}')
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let mut words: Vec<String> = cleaned.split_whitespace().map(str::to_string).collect();
    for wake in [["hey", "nexus"], ["ok", "nexus"], ["okay", "nexus"]] {
        if words.len() > 2 && words[0] == wake[0] && words[1] == wake[1] {
            words.drain(..2);
        }
    }
    if words.len() > 1 && words[0] == "nexus" {
        words.remove(0);
    }
    words
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn letter_similarity(a: &str, b: &str) -> f64 {
    let ca: Vec<char> = a.chars().filter(|c| !c.is_whitespace()).collect();
    let cb: Vec<char> = b.chars().filter(|c| !c.is_whitespace()).collect();
    let longest = ca.len().max(cb.len());
    if longest == 0 {
        return 0.0;
    }
    1.0 - levenshtein(&ca, &cb) as f64 / longest as f64
}

fn skeleton(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric() && !"aeiou".contains(*c)).collect()
}

/// 0..1: how alike two phrases are by letters, or by consonants alone (speech recognition mostly
/// confuses vowels: "brief" / "brave").
pub fn similarity(a: &str, b: &str) -> f64 {
    let letters = letter_similarity(a, b);
    let (sa, sb) = (skeleton(a), skeleton(b));
    let consonants = if sa.chars().count() >= 2 && sb.chars().count() >= 2 { letter_similarity(&sa, &sb) } else { 0.0 };
    letters.max(consonants)
}

/// Words shared at the start and at the end of two token lists, and what is left in the middle.
fn strip_common(a: &[String], b: &[String]) -> (Vec<String>, Vec<String>, usize) {
    let max_prefix = a.len().min(b.len());
    let mut p = 0;
    while p < max_prefix && a[p] == b[p] {
        p += 1;
    }
    let mut q = 0;
    while q < a.len() - p && q < b.len() - p && a[a.len() - 1 - q] == b[b.len() - 1 - q] {
        q += 1;
    }
    (a[p..a.len() - q].to_vec(), b[p..b.len() - q].to_vec(), p + q)
}

fn strip_cue(words: &[String]) -> Option<Vec<String>> {
    for cue in CUES {
        if words.len() >= cue.len() && cue.iter().zip(words).all(|(c, w)| *c == w.as_str()) {
            return Some(words[cue.len()..].to_vec());
        }
    }
    None
}

/// The window of `prev` (about as long as `rest`) that looks most like `rest`.
fn best_window(prev: &[String], rest: &[String]) -> Option<(Vec<String>, Vec<String>)> {
    let target = rest.join(" ");
    let mut best: Option<(f64, Vec<String>)> = None;
    let lo = rest.len().saturating_sub(1).max(1);
    for len in lo..=rest.len() + 1 {
        if len > prev.len() {
            break;
        }
        for start in 0..=prev.len() - len {
            let window = prev[start..start + len].to_vec();
            let score = similarity(&window.join(" "), &target);
            if window != rest && best.as_ref().map_or(true, |(s, _)| score > *s) {
                best = Some((score, window));
            }
        }
    }
    best.map(|(_, w)| (w, rest.to_vec()))
}

/// Accept a (heard, meant) pair only if it could really be one word misheard as another.
fn plausible(heard: &[String], meant: &[String]) -> Option<(String, String)> {
    if heard.is_empty() || meant.is_empty() || heard.len() > 4 || meant.len() > 4 {
        return None;
    }
    let (h, m) = (heard.join(" "), meant.join(" "));
    if h == m || h.chars().filter(|c| c.is_alphanumeric()).count() < 3 || m.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
        return None;
    }
    let only_noise = |w: &[String]| w.iter().all(|t| STOPWORDS.contains(&t.as_str()) || t.chars().all(|c| c.is_ascii_digit()));
    if only_noise(heard) || only_noise(meant) {
        return None;
    }
    if similarity(&h, &m) < MIN_SIMILARITY {
        return None;
    }
    Some((h, m))
}

/// Did the user just correct the previous turn? `gap_secs` is the time between the two turns and
/// `prev_completed` says whether the previous command actually ran.
pub fn detect(prev: &str, now: &str, gap_secs: i64, prev_completed: bool) -> Option<(String, String)> {
    let p = tokens(prev);
    let n = tokens(now);
    if p.is_empty() || n.is_empty() {
        return None;
    }

    // An explicit correction: "no, I said open brave".
    if let Some(rest) = strip_cue(&n) {
        if rest.is_empty() || rest.len() > 6 || gap_secs > CUE_WINDOW_SECS {
            return None;
        }
        let (hs, ms, shared) = strip_common(&p, &rest);
        let pair = if shared >= 1 && !hs.is_empty() && !ms.is_empty() { Some((hs, ms)) } else { best_window(&p, &rest) };
        return pair.and_then(|(h, m)| plausible(&h, &m));
    }

    // The same command again with one word changed, right after one that ran.
    if prev_completed && gap_secs <= REISSUE_WINDOW_SECS {
        let (hs, ms, shared) = strip_common(&p, &n);
        if shared >= 1 && !hs.is_empty() && !ms.is_empty() && hs.len() <= 3 && ms.len() <= 3 {
            return plausible(&hs, &ms);
        }
    }
    None
}

// ---- Applying learned corrections ----

static LEARNED: RwLock<Vec<(String, String)>> = RwLock::new(Vec::new());

fn starts_like_a_command(words: &[String]) -> bool {
    words.first().map_or(false, |w| COMMAND_VERBS.contains(&w.as_str()))
}

/// Replace learned mishearings in a command transcript. Whole words only; other talk is untouched.
pub fn apply_with(list: &[(String, String)], text: &str) -> String {
    if list.is_empty() || !starts_like_a_command(&tokens(text)) {
        return text.to_string();
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let clean = |w: &str| -> String {
        w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase().replace('\'', "")
    };
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    'outer: while i < words.len() {
        for (heard, meant) in list {
            let h: Vec<&str> = heard.split(' ').collect();
            if i + h.len() <= words.len() && h.iter().enumerate().all(|(k, hw)| clean(words[i + k]) == *hw) {
                // Keep trailing punctuation of the last replaced word ("open brief." -> "open brave.").
                let last = words[i + h.len() - 1];
                let trailing: String = last.chars().rev().take_while(|c| !c.is_alphanumeric()).collect::<Vec<_>>().into_iter().rev().collect();
                out.push(format!("{meant}{trailing}"));
                i += h.len();
                continue 'outer;
            }
        }
        out.push(words[i].to_string());
        i += 1;
    }
    out.join(" ")
}

/// Rewrite learned mishearings using the in-memory list (no I/O; called while parsing).
pub fn apply_learned(text: &str) -> String {
    let list = LEARNED.read().map(|g| g.clone()).unwrap_or_default();
    let fixed = apply_with(&list, text);
    if fixed != text {
        println!("[CORRECTION] applied a learned correction: '{}' -> '{}'", text.trim(), fixed.trim());
    }
    fixed
}

/// Load the promoted corrections from the store into memory. Call at startup and after any change.
pub fn refresh(dir: &Path) {
    if let Some(list) = super::with_store(dir, |s| s.corrections_promoted()) {
        if let Ok(mut g) = LEARNED.write() {
            *g = list;
        }
    }
}

pub fn init(dir: &Path) {
    refresh(dir);
}

// ---- Observing turns ----

struct Prev {
    text: String,
    ts: i64,
    completed: bool,
}

static LAST: Mutex<Option<Prev>> = Mutex::new(None);

/// Compare a turn with the previous one; remember any correction found. Returns (heard, meant, count).
pub fn learn_from(
    dir: &Path,
    prev_text: &str,
    prev_ts: i64,
    prev_completed: bool,
    text: &str,
    ts: i64,
) -> Option<(String, String, u32)> {
    let (heard, meant) = detect(prev_text, text, ts - prev_ts, prev_completed)?;
    let count = super::with_store(dir, |s| s.corrections_observe(&heard, &meant, ts))?;
    Some((heard, meant, count))
}

/// Hook for every recorded turn (`conversation::record_conversation_turn`).
pub fn observe_turn(dir: &Path, transcript: &str, ts: i64, completed: bool) {
    let prev = {
        let mut g = LAST.lock().unwrap_or_else(|e| e.into_inner());
        g.replace(Prev { text: transcript.to_string(), ts, completed })
    };
    let Some(prev) = prev else { return };
    if let Some((heard, meant, count)) = learn_from(dir, &prev.text, prev.ts, prev.completed, transcript, ts) {
        println!("[CORRECTION] heard '{heard}' but you meant '{meant}' (seen {count}x)");
        // Only this production path updates the in-memory list the parser reads; `learn_from` stays
        // free of global state so tests cannot leak into each other.
        refresh(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_corr_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    fn pair(prev: &str, now: &str, gap: i64) -> Option<(String, String)> {
        detect(prev, now, gap, true)
    }

    #[test]
    fn an_explicit_correction_is_found() {
        assert_eq!(pair("open brief", "no I said open brave", 5), Some(("brief".into(), "brave".into())));
        assert_eq!(pair("open brief", "I meant open brave", 5), Some(("brief".into(), "brave".into())));
        assert_eq!(pair("Open brief.", "Nexus, no, I said brave", 5), Some(("brief".into(), "brave".into())));
        assert_eq!(pair("message priya", "no it's pria", 8), Some(("priya".into(), "pria".into())));
    }

    #[test]
    fn the_same_command_with_one_word_changed_is_found() {
        assert_eq!(pair("open brief", "open brave", 10), Some(("brief".into(), "brave".into())));
        assert_eq!(pair("call mum", "call mom", 6), Some(("mum".into(), "mom".into())));
    }

    #[test]
    fn two_different_commands_are_not_a_correction() {
        assert_eq!(pair("open chrome", "open notepad", 4), None, "unrelated words");
        assert_eq!(pair("call mom", "call dad", 4), None, "unrelated names");
        assert_eq!(pair("play jazz", "play something else", 4), None);
        assert_eq!(pair("open chrome", "what is the weather", 4), None);
    }

    #[test]
    fn timing_and_outcome_gate_the_guess() {
        assert_eq!(pair("open brief", "open brave", 60), None, "a re-issue after 25 s is a new command");
        assert_eq!(detect("open brief", "open brave", 5, false), None, "the first command did not run");
        assert_eq!(pair("open brief", "no I said open brave", 200), None, "a cue only reaches back 2 minutes");
        assert!(pair("open brief", "no I said open brave", 100).is_some());
    }

    #[test]
    fn a_polite_no_is_not_a_correction() {
        assert_eq!(pair("open brief", "no thanks", 3), None);
        assert_eq!(pair("open brief", "no", 3), None);
        assert_eq!(pair("open brief", "actually never mind that", 3), None);
        assert_eq!(pair("what time is it", "no I said what day is it", 3), None, "too different to be one misheard word");
    }

    #[test]
    fn similarity_works_on_letters_or_consonants() {
        assert!(similarity("brief", "brave") >= MIN_SIMILARITY);
        assert!(similarity("priya", "pria") >= MIN_SIMILARITY);
        assert!(similarity("chrome", "notepad") < MIN_SIMILARITY);
        assert!(similarity("mom", "dad") < MIN_SIMILARITY);
    }

    #[test]
    fn learned_words_are_applied_to_commands_only_and_whole_words_only() {
        let list = vec![("brief".to_string(), "brave".to_string())];
        assert_eq!(apply_with(&list, "open brief"), "open brave");
        assert_eq!(apply_with(&list, "Open Brief."), "Open brave.", "case and punctuation of the rest are kept");
        assert_eq!(apply_with(&list, "nexus open brief now"), "nexus open brave now");
        // ordinary talk, and longer words that merely contain the phrase, are never touched
        assert_eq!(apply_with(&list, "give me a brief summary"), "give me a brief summary");
        assert_eq!(apply_with(&list, "open briefcase"), "open briefcase");
        assert_eq!(apply_with(&[], "open brief"), "open brief");
    }

    #[test]
    fn nothing_is_promoted_after_one_sighting_and_it_is_after_two_separate_ones() {
        let dir = d("promote");
        let t0 = 1_800_000_000;
        // First sighting: remembered, not applied.
        let first = learn_from(&dir, "open brief", t0 - 4, true, "open brave", t0).unwrap();
        assert_eq!((first.0.as_str(), first.1.as_str(), first.2), ("brief", "brave", 1));
        let promoted = super::super::with_store(&dir, |s| s.corrections_promoted()).unwrap();
        assert!(promoted.is_empty(), "one sighting must not change behaviour: {promoted:?}");

        // The same event seen again within a minute is not a second confirmation.
        let again = learn_from(&dir, "open brief", t0 + 20, true, "open brave", t0 + 24).unwrap();
        assert_eq!(again.2, 1);

        // A separate occasion is.
        let second = learn_from(&dir, "open brief", t0 + 3_600, true, "no I said open brave", t0 + 3_604).unwrap();
        assert_eq!(second.2, 2);
        let promoted = super::super::with_store(&dir, |s| s.corrections_promoted()).unwrap();
        assert_eq!(promoted, vec![("brief".to_string(), "brave".to_string())]);
    }

    #[test]
    fn a_rival_answer_with_an_equal_count_blocks_promotion() {
        let dir = d("rival");
        let t0 = 1_800_000_000;
        for (i, meant) in ["open brave", "open bravo"].iter().enumerate() {
            for k in 0..2 {
                let at = t0 + (i as i64) * 10_000 + k * 3_600;
                learn_from(&dir, "open brief", at - 3, true, meant, at);
            }
        }
        let promoted = super::super::with_store(&dir, |s| s.corrections_promoted()).unwrap();
        assert!(promoted.is_empty(), "two answers tied 2-2: {promoted:?}");
        // One more sighting for brave breaks the tie.
        learn_from(&dir, "open brief", t0 + 100_000 - 3, true, "open brave", t0 + 100_000);
        let promoted = super::super::with_store(&dir, |s| s.corrections_promoted()).unwrap();
        assert_eq!(promoted, vec![("brief".to_string(), "brave".to_string())]);
    }

    /// Dev only: run the detector over this PC's real missed-intents log (consecutive utterances) to see how
    /// often it would fire. Read-only; prints counts and the detected pairs, nothing else.
    /// `cargo test --lib -- --ignored replay_real_missed_intents --nocapture`
    #[test]
    #[ignore = "dev: reads this PC's real missed_intents.jsonl"]
    fn replay_real_missed_intents() {
        let Some(dir) = crate::worker_auth::app_data_dir_envless() else { return };
        let Ok(text) = std::fs::read_to_string(dir.join("missed_intents.jsonl")) else {
            println!("no missed_intents.jsonl here");
            return;
        };
        let turns: Vec<(i64, String)> = text
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter_map(|v| Some((v["timestamp_ms"].as_i64()? / 1000, v["transcript"].as_str()?.to_string())))
            .collect();
        let (mut close_pairs, mut cue_hits, mut reissue_hits) = (0, 0, 0);
        let mut found: Vec<String> = vec![];
        for w in turns.windows(2) {
            let gap = w[1].0 - w[0].0;
            if gap < 0 || gap > CUE_WINDOW_SECS {
                continue;
            }
            close_pairs += 1;
            // Worst case for false positives: pretend the earlier command ran.
            if let Some((h, m)) = detect(&w[0].1, &w[1].1, gap, true) {
                if strip_cue(&tokens(&w[1].1)).is_some() {
                    cue_hits += 1;
                } else {
                    reissue_hits += 1;
                }
                found.push(format!("'{h}' -> '{m}' (gap {gap}s)"));
            }
        }
        println!("REPLAY of {} utterances: {close_pairs} pairs within 2 min; explicit-cue detections {cue_hits}, re-issue detections {reissue_hits}", turns.len());
        for f in found {
            println!("  {f}");
        }
    }

    #[test]
    fn forgetting_a_correction_removes_it_and_wipe_clears_all() {
        let dir = d("forget");
        let t0 = 1_800_000_000;
        learn_from(&dir, "open brief", t0 - 3, true, "open brave", t0);
        learn_from(&dir, "open brief", t0 + 3_600 - 3, true, "open brave", t0 + 3_600);
        let n = super::super::with_store(&dir, |s| s.correction_forget("brief")).unwrap();
        assert_eq!(n, 1);
        assert!(super::super::with_store(&dir, |s| s.corrections_all()).unwrap().is_empty());
        learn_from(&dir, "call mum", t0 - 3, true, "call mom", t0);
        super::super::with_store(&dir, |s| s.wipe("test", t0));
        assert!(super::super::with_store(&dir, |s| s.corrections_all()).unwrap().is_empty());
    }
}
