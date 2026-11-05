//! Smart pause hold decision module: determines whether a speech turn is complete, incomplete, or unsure.
//!
//! A finished command goes immediately (no added wait, no extra STT call).
//! When the transcript is clearly UNFINISHED (ends with an open connector word, trailing punctuation like ",", "-", "...",
//! or the deterministic parser recognises a command verb with a required slot missing), the turn is held to allow
//! the speaker to continue.

use crate::directed::Origin;
use crate::intent_parser::parse_deterministic;
use crate::center::{validate, Validity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Complete,
    Incomplete,
    Unsure,
}

const OPEN_WORDS: &[&str] = &[
    // Articles & determiners / possessives
    "a", "an", "the", "my", "your", "his", "her", "our", "their", "its",
    // Conjunctions & transitions
    "and", "then", "or", "but", "nor", "so", "because", "since", "while",
    "although", "though", "unless", "until", "that", "which", "if", "also", "plus",
    // Prepositions
    "to", "for", "with", "of", "in", "on", "at", "from", "by", "into", "onto",
    "about", "between", "through", "under", "over", "as", "than", "like",
    // Hesitation fillers
    "um", "uh", "er", "hmm", "hm", "ah",
];

const OPEN_PHRASES: &[&str] = &[
    "and then",
    "send it to",
    "open the",
    "what's the weather like in",
    "remind me to",
    "send a message to",
    "search for",
    "look up",
];

const SHORT_ACKS: &[&str] = &[
    "yes", "yeah", "yep", "yup", "ya", "aye", "sure", "ok", "okay", "alright", "all right",
    "no", "nope", "nah", "stop", "cancel", "halt", "quit", "exit", "abort",
    "never mind", "nevermind", "go ahead", "do it", "proceed",
    "sounds good", "looks good", "i think so", "think so",
    "that's right", "thats right", "correct", "affirmative", "negative",
    "please do", "don't", "dont", "no thanks", "no thank you", "yes please",
];

/// Determine whether a transcript represents a complete utterance, an incomplete utterance (should hold),
/// or an unsure utterance (free-form content, proceeds without hold).
pub(crate) fn verdict(text: &str) -> Verdict {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Verdict::Incomplete;
    }

    // Trailing punctuation indicating the speaker trailed off or was cut off mid-thought.
    if trimmed.ends_with(',')
        || trimmed.ends_with('-')
        || trimmed.ends_with("...")
        || trimmed.ends_with('…')
        || trimmed.ends_with("--")
    {
        return Verdict::Incomplete;
    }

    // Clean text for word / phrase analysis.
    let stripped_clean = trimmed
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace() || c == '…')
        .to_lowercase();
    let words: Vec<&str> = stripped_clean.split_whitespace().collect();
    if words.is_empty() {
        return Verdict::Incomplete;
    }

    // Short affirmative, negative, or stop acknowledgments (<= 3 words).
    if words.len() <= 3 {
        for &ack in SHORT_ACKS {
            if stripped_clean == ack {
                return Verdict::Complete;
            }
        }
    }

    // Finished question check (>= 3 words and ends with '?').
    // If the last word is an obvious dangling connector (like "in", "to", "for", "with"),
    // it remains incomplete even with a '?'.
    if trimmed.ends_with('?') {
        if words.len() >= 3 {
            let last_q_raw = words[words.len() - 1];
            let is_dangling_preposition = matches!(
                last_q_raw,
                "in" | "to" | "for" | "with" | "at" | "from" | "of" | "on" | "by" | "and" | "or" | "the" | "a" | "an" | "my"
            );
            if !is_dangling_preposition {
                return Verdict::Complete;
            }
        }
    }

    // Check multi-word open phrases.
    for &phrase in OPEN_PHRASES {
        if stripped_clean == phrase || stripped_clean.ends_with(&format!(" {phrase}")) {
            return Verdict::Incomplete;
        }
    }

    // Check open connector words on the last word.
    let last_word = words[words.len() - 1];

    // Exception for "so": "I think so", "hope so", "suppose so", "guess so", "even so", "just so", "not so"
    // are completed thoughts, not open connectors.
    let is_so_exception = last_word == "so"
        && words.len() >= 2
        && matches!(
            words[words.len() - 2],
            "think" | "thought" | "hope" | "hoped" | "guess" | "guessed" | "suppose" | "supposed"
                | "believe" | "believed" | "say" | "said" | "just" | "even" | "not"
        );

    if !is_so_exception {
        for &open_word in OPEN_WORDS {
            if last_word == open_word {
                return Verdict::Incomplete;
            }
        }
    }

    // Check deterministic intent parsing and validation.
    // If the parser recognises a command verb and validator reports a missing required slot, it's Incomplete.
    // If the parser returns an intent and validator confirms Ok, it's Complete.
    if let Some(parsed) = parse_deterministic(trimmed) {
        match validate(&parsed.intent, trimmed, false) {
            Validity::NeedSlot { .. } => return Verdict::Incomplete,
            Validity::Ok => return Verdict::Complete,
            _ => {}
        }
    }

    // Free-form sentences ending on a normal content word behave like Complete (do not hold).
    Verdict::Unsure
}

/// Pure predicate deciding if a turn qualifies for smart pause hold.
pub(crate) fn may_hold(
    turn_hold_ms: u32,
    ghost_active: bool,
    origin: Origin,
    extended: bool,
    verdict: Verdict,
    ms_since_speech_end: u64,
) -> bool {
    turn_hold_ms > 0
        && !ghost_active
        && origin == Origin::Direct
        && !extended
        && verdict == Verdict::Incomplete
        && (ms_since_speech_end as u32) < turn_hold_ms
}

/// Calculate the number of 80ms chunks remaining for the smart pause hold window, clamped to a minimum of 5.
pub(crate) fn remaining_hold_chunks(turn_hold_ms: u32, ms_since_speech_end: u64) -> u32 {
    let rem_ms = turn_hold_ms.saturating_sub(ms_since_speech_end as u32);
    (rem_ms / 80).max(5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verdict_table_brief_cases() {
        // Required cases from the brief:
        assert_eq!(verdict("open notepad"), Verdict::Complete);
        assert_eq!(verdict("send a message to"), Verdict::Incomplete);
        assert_eq!(verdict("what's the weather like in"), Verdict::Incomplete);
        assert_eq!(verdict("remind me to"), Verdict::Incomplete);
        assert!(matches!(verdict("play some music"), Verdict::Complete | Verdict::Unsure));
        assert_eq!(verdict("um"), Verdict::Incomplete);
        assert_eq!(verdict("and"), Verdict::Incomplete);
        assert_ne!(verdict("I think so"), Verdict::Incomplete);
        assert_ne!(verdict("open the door"), Verdict::Incomplete);
    }

    #[test]
    fn test_verdict_table_incomplete_connectors() {
        let cases = [
            "I want to go to the store and",
            "open chrome and then",
            "send it to",
            "open the",
            "play music with",
            "turn on",
            "search for",
            "tell me about",
            "remind me about",
            "look at",
            "give me a",
            "show me an",
            "where is my",
            "because",
            "or",
            "but",
            "also",
            "plus",
            "like",
            "uh",
            "er",
            "hmm",
            "hm",
            "ah",
            "into",
            "onto",
            "between",
            "through",
            "until",
            "unless",
            "which",
            "that",
            "if",
        ];
        for case in cases {
            assert_eq!(
                verdict(case),
                Verdict::Incomplete,
                "Expected Incomplete for '{case}'"
            );
        }
    }

    #[test]
    fn test_verdict_trailing_punctuation_incomplete() {
        let cases = [
            "open notepad,",
            "send an email-",
            "I was thinking...",
            "maybe we could…",
            "wait for--",
        ];
        for case in cases {
            assert_eq!(
                verdict(case),
                Verdict::Incomplete,
                "Expected Incomplete for trailing punctuation in '{case}'"
            );
        }
    }

    #[test]
    fn test_verdict_questions_complete_and_dangling() {
        // Complete questions >= 3 words ending with '?'
        assert_eq!(verdict("what time is it?"), Verdict::Complete);
        assert_eq!(verdict("how are you today?"), Verdict::Complete);
        assert_eq!(verdict("is it raining outside?"), Verdict::Complete);
        assert_eq!(verdict("what is the weather like?"), Verdict::Complete);

        // Dangling prepositions inside questions remain Incomplete
        assert_eq!(verdict("what is the weather like in?"), Verdict::Incomplete);
        assert_eq!(verdict("who should I send this to?"), Verdict::Incomplete);
    }

    #[test]
    fn test_verdict_short_acks_complete() {
        let acks = [
            "yes",
            "yes please",
            "yeah",
            "yep",
            "sure",
            "ok",
            "okay",
            "alright",
            "all right",
            "no",
            "no thanks",
            "no thank you",
            "nope",
            "stop",
            "cancel",
            "never mind",
            "nevermind",
            "go ahead",
            "do it",
            "sounds good",
            "looks good",
            "i think so",
            "think so",
            "that's right",
            "correct",
        ];
        for ack in acks {
            assert_eq!(
                verdict(ack),
                Verdict::Complete,
                "Expected Complete for ack '{ack}'"
            );
        }
    }

    #[test]
    fn test_verdict_so_exception() {
        assert_ne!(verdict("I think so"), Verdict::Incomplete);
        assert_ne!(verdict("I hope so"), Verdict::Incomplete);
        assert_ne!(verdict("I guess so"), Verdict::Incomplete);
        assert_ne!(verdict("we believe so"), Verdict::Incomplete);
        assert_ne!(verdict("even so"), Verdict::Incomplete);
        assert_ne!(verdict("just so"), Verdict::Incomplete);
        // Dangling "so" as a conjunction remains Incomplete:
        assert_eq!(verdict("it was raining so"), Verdict::Incomplete);
        assert_eq!(verdict("I was tired so"), Verdict::Incomplete);
    }

    #[test]
    fn test_verdict_missing_slots_incomplete() {
        // Deterministic commands missing required contact/message slot produce Incomplete
        assert_eq!(verdict("send a message on whatsapp"), Verdict::Incomplete);
        assert_eq!(verdict("send mom a whatsapp message"), Verdict::Incomplete);
    }

    #[test]
    fn test_verdict_unsure_or_complete_normal_sentences() {
        let cases = [
            "the quick brown fox jumps over the lazy dog",
            "tomorrow is going to be sunny",
            "I finished writing the report today",
            "we are ready to start the presentation",
            "it is time to sleep", // ends in "sleep", not a connector
        ];
        for case in cases {
            assert_ne!(
                verdict(case),
                Verdict::Incomplete,
                "Expected non-Incomplete for '{case}'"
            );
        }
    }

    #[test]
    fn test_may_hold_predicate_table() {
        // Standard valid hold: hold_ms=3000, not ghost, Direct, not extended, Incomplete, elapsed=800ms
        assert!(may_hold(3000, false, Origin::Direct, false, Verdict::Incomplete, 800));

        // Disabled setting: turn_hold_ms = 0
        assert!(!may_hold(0, false, Origin::Direct, false, Verdict::Incomplete, 800));

        // Ghost mode active: must not hold
        assert!(!may_hold(3000, true, Origin::Direct, false, Verdict::Incomplete, 800));

        // HotMic origin (not explicit wake/hotkey Direct): must not hold
        assert!(!may_hold(3000, false, Origin::HotMic, false, Verdict::Incomplete, 800));

        // Already extended: max one extension per turn
        assert!(!may_hold(3000, false, Origin::Direct, true, Verdict::Incomplete, 800));

        // Complete verdict: must not hold
        assert!(!may_hold(3000, false, Origin::Direct, false, Verdict::Complete, 800));

        // Unsure verdict: must not hold
        assert!(!may_hold(3000, false, Origin::Direct, false, Verdict::Unsure, 800));

        // Elapsed time >= turn_hold_ms: silence window already expired
        assert!(!may_hold(3000, false, Origin::Direct, false, Verdict::Incomplete, 3000));
        assert!(!may_hold(3000, false, Origin::Direct, false, Verdict::Incomplete, 3500));
    }

    #[test]
    fn test_remaining_hold_chunks_table() {
        // 3000 ms hold, 800 ms elapsed -> 2200 ms remaining / 80 = 27 chunks
        assert_eq!(remaining_hold_chunks(3000, 800), 27);

        // 3000 ms hold, 0 ms elapsed -> 3000 / 80 = 37 chunks
        assert_eq!(remaining_hold_chunks(3000, 0), 37);

        // 3000 ms hold, 2800 ms elapsed -> 200 ms remaining / 80 = 2 chunks -> clamped to min 5
        assert_eq!(remaining_hold_chunks(3000, 2800), 5);

        // 3000 ms hold, 3200 ms elapsed -> saturates to 0 -> clamped to min 5
        assert_eq!(remaining_hold_chunks(3000, 3200), 5);
    }
}
