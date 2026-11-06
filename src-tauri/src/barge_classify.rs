//! Pure classification of spoken barge-in utterances.
//!
//! Decides whether to:
//! - Resume: speech was echo of NEXUS's own voice, empty/mumble/filler, or unrelated background talk (e.g. TV).
//! - StopOnly: speech was a pure stop command ("stop", "be quiet", "hold on", "wait", "shut up", "that's enough").
//! - StopAndRoute: speech was a short reply ("ok", "yes", "no", "cancel", ...), a parsed command, wake word, or continuation.

use crate::directed;
use crate::ghost;
use crate::intent_parser;
use crate::turn_continue;
use crate::wake_names;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BargeAction {
    Resume,
    StopAndRoute,
    StopOnly,
}

const HESITATION_FILLERS: &[&str] = &[
    "uh", "um", "umm", "uhh", "hmm", "mm", "mmm", "huh", "er", "ah", "ahh", "eh",
];

const SHORT_REPLIES: &[&str] = &[
    "ok",
    "okay",
    "yes",
    "yeah",
    "do it",
    "go ahead",
    "sure",
    "no",
    "cancel",
    "proceed",
    "continue",
    "never mind",
    "thanks",
    "thank you",
];

const EXTRA_STOP_PHRASES: &[&str] = &[
    "stop talking",
    "be quiet",
    "that's enough",
    "thats enough",
    "wait",
    "hold on",
    "shut up",
    "enough",
];

fn normalize_for_match(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_ascii_punctuation() && c != '\'' { ' ' } else { c })
        .collect();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn strip_greeting_and_name(text: &str) -> &str {
    let mut s = text.trim();
    for greeting in &["hey", "ok", "okay", "hello", "hi"] {
        if let Some(rest) = s.strip_prefix(greeting) {
            let rest = rest.trim_start();
            if !rest.is_empty() {
                s = rest;
            }
        }
    }
    for name in &["nexus", "lexus", "nixus", "nexis"] {
        if let Some(rest) = s.strip_prefix(name) {
            let rest = rest.trim_start();
            if !rest.is_empty() {
                s = rest;
            }
        }
    }
    s.trim()
}

fn strip_politeness(text: &str) -> &str {
    let mut s = text.trim();
    for suffix in &["please", "sir"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            let rest = rest.trim_end();
            if !rest.is_empty() {
                s = rest;
            }
        }
    }
    s.trim()
}

pub fn classify(text: &str, spoken_lines: &[String]) -> BargeAction {
    let t = directed::tokens(text);

    // 1. Echo: word-overlap with NEXUS's own recent speech >= 0.6. The capture runs over NEXUS's own
    //    (ducked) voice, so a real "ok" often arrives mixed with echo words ("ok I have scheduled your ...").
    //    Judge the words NEXUS did not say itself: if those few words are a reply/stop/command, honour them.
    if t.len() >= 2 && directed::echo_overlap(&t, spoken_lines) >= 0.6 {
        let spoken_words: std::collections::HashSet<String> =
            spoken_lines.iter().flat_map(|l| directed::tokens(l)).collect();
        let novel: Vec<String> = t.iter().filter(|w| !spoken_words.contains(*w)).cloned().collect();
        if !novel.is_empty() && novel.len() <= 4 {
            let novel_text = novel.join(" ");
            let action = classify(&novel_text, &[]);
            if action != BargeAction::Resume {
                return action;
            }
        }
        return BargeAction::Resume;
    }

    // 2. Empty / mumble / only fillers like "uh um hmm"
    if t.is_empty() || t.iter().all(|w| HESITATION_FILLERS.contains(&w.as_str())) {
        return BargeAction::Resume;
    }

    let norm = normalize_for_match(text);
    let stripped = strip_greeting_and_name(&norm);
    let de_polite_norm = strip_politeness(&norm);
    let de_polite_stripped = strip_politeness(stripped);

    // 3. Short replies ("ok", "okay", "yes", "yeah", "do it", "go ahead", "sure", "no", "cancel", "proceed", "continue", "never mind", "thanks", "thank you") => StopAndRoute
    if SHORT_REPLIES.iter().any(|&r| {
        norm == r || stripped == r || de_polite_norm == r || de_polite_stripped == r
    }) {
        return BargeAction::StopAndRoute;
    }

    // 4. Stop phrases (ghost::is_stop_phrase, plus "stop talking", "be quiet", "that's enough", "wait", "hold on", "shut up", "enough") => StopOnly
    if EXTRA_STOP_PHRASES.iter().any(|&p| {
        norm == p || stripped == p || de_polite_norm == p || de_polite_stripped == p
    }) || ghost::is_stop_phrase(text) || ghost::is_stop_phrase(stripped) {
        return BargeAction::StopOnly;
    }

    // 5. Anything that parses as a command, or contains the wake word, or a continuation phrase => StopAndRoute
    let stripped_wake = turn_continue::strip_wake_prefix(text);
    if intent_parser::parse_deterministic(stripped_wake).is_some() {
        return BargeAction::StopAndRoute;
    }
    if wake_names::classify(&t) != wake_names::NameMatch::None {
        return BargeAction::StopAndRoute;
    }
    if matches!(turn_continue::detect(text), turn_continue::Continuation::Append(_)) {
        return BargeAction::StopAndRoute;
    }

    // 6. Otherwise (free talk that is not echo, e.g. TV) => Resume
    BargeAction::Resume
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_barge_classify_table() {
        let echo_context = vec![
            "I have scheduled your appointment for tomorrow at three PM in the conference room".to_string(),
            "Opening the repository in visual studio code now".to_string(),
            "Here is the summary of latest changes in your repository".to_string(),
        ];

        let cases = [
            // Short replies (StopAndRoute)
            ("ok", BargeAction::StopAndRoute),
            ("okay", BargeAction::StopAndRoute),
            ("yes", BargeAction::StopAndRoute),
            ("yeah", BargeAction::StopAndRoute),
            ("do it", BargeAction::StopAndRoute),
            ("go ahead", BargeAction::StopAndRoute),
            ("sure", BargeAction::StopAndRoute),
            ("no", BargeAction::StopAndRoute),
            ("cancel", BargeAction::StopAndRoute),
            ("proceed", BargeAction::StopAndRoute),
            ("continue", BargeAction::StopAndRoute),
            ("never mind", BargeAction::StopAndRoute),
            ("thanks", BargeAction::StopAndRoute),
            ("thank you", BargeAction::StopAndRoute),
            ("Ok.", BargeAction::StopAndRoute),
            ("OKAY!", BargeAction::StopAndRoute),
            ("Yes, please.", BargeAction::StopAndRoute),
            ("Nexus, do it.", BargeAction::StopAndRoute),
            ("Hey nexus, go ahead", BargeAction::StopAndRoute),
            ("ok cancel", BargeAction::StopAndRoute),

            // Stop phrases (StopOnly)
            ("stop", BargeAction::StopOnly),
            ("stop talking", BargeAction::StopOnly),
            ("be quiet", BargeAction::StopOnly),
            ("that's enough", BargeAction::StopOnly),
            ("thats enough", BargeAction::StopOnly),
            ("wait", BargeAction::StopOnly),
            ("hold on", BargeAction::StopOnly),
            ("shut up", BargeAction::StopOnly),
            ("enough", BargeAction::StopOnly),
            ("Stop!", BargeAction::StopOnly),
            ("Nexus, stop.", BargeAction::StopOnly),
            ("hey nexus, be quiet", BargeAction::StopOnly),
            ("stand down", BargeAction::StopOnly),
            ("halt", BargeAction::StopOnly),
            ("abort", BargeAction::StopOnly),
            ("exit ghost", BargeAction::StopOnly),

            // Commands (StopAndRoute)
            ("open chrome", BargeAction::StopAndRoute),
            ("open notepad", BargeAction::StopAndRoute),
            ("open settings", BargeAction::StopAndRoute),
            ("shut down my laptop", BargeAction::StopAndRoute),
            ("nexus open brave", BargeAction::StopAndRoute),
            ("hey nexus, search for rust programming", BargeAction::StopAndRoute),
            ("order pizza from dominos", BargeAction::StopAndRoute),
            ("give me coderabbit review", BargeAction::StopAndRoute),

            // Wake word mentions (StopAndRoute)
            ("nexus", BargeAction::StopAndRoute),
            ("hey nexus", BargeAction::StopAndRoute),
            ("ok nexus", BargeAction::StopAndRoute),
            ("lexus", BargeAction::StopAndRoute),
            ("nixus", BargeAction::StopAndRoute),

            // Continuation phrases (StopAndRoute)
            ("one more thing", BargeAction::StopAndRoute),
            ("and also check my calendar", BargeAction::StopAndRoute),
            ("let me finish", BargeAction::StopAndRoute),
            ("add to that also send an email", BargeAction::StopAndRoute),

            // Empty / mumble / hesitation fillers (Resume)
            ("", BargeAction::Resume),
            ("   ", BargeAction::Resume),
            ("...", BargeAction::Resume),
            ("uh", BargeAction::Resume),
            ("um", BargeAction::Resume),
            ("hmm", BargeAction::Resume),
            ("uh um", BargeAction::Resume),
            ("er ah", BargeAction::Resume),
            ("mm hmm", BargeAction::Resume),
            ("umm...", BargeAction::Resume),

            // Echo of NEXUS's own recent speech (Resume)
            ("appointment for tomorrow at three PM", BargeAction::Resume),
            ("in visual studio code now", BargeAction::Resume),
            ("summary of latest changes in your repository", BargeAction::Resume),

            // Free talk / TV chatter / unrelated conversation (Resume)
            ("in today's news the president visited the united nations", BargeAction::Resume),
            ("we should probably get some lunch after this meeting", BargeAction::Resume),
            ("the stock market went up three hundred points today", BargeAction::Resume),
            ("the cat is sitting on top of the television screen", BargeAction::Resume),
            ("we will be right back after these commercial messages", BargeAction::Resume),
            ("she said she would call back in twenty minutes", BargeAction::Resume),
            ("I was talking to my brother about the football game", BargeAction::Resume),
        ];

        for (input, expected) in cases {
            let actual = classify(input, &echo_context);
            assert_eq!(
                actual, expected,
                "classify({:?}) should be {:?}, got {:?}",
                input, expected, actual
            );
        }
    }

    #[test]
    fn test_reply_mixed_with_echo_is_honoured() {
        let spoken = vec!["I have scheduled your appointment for tomorrow at three PM in the conference room".to_string()];
        assert_eq!(classify("ok I have scheduled your appointment for tomorrow at three", &spoken), BargeAction::StopAndRoute);
        assert_eq!(classify("stop I have scheduled your appointment for tomorrow", &spoken), BargeAction::StopOnly);
        assert_eq!(classify("I have scheduled your appointment for tomorrow at three", &spoken), BargeAction::Resume);
    }
}
