//! Turn continuation recognition and merging.
//!
//! Handles utterances right after a command where the owner says:
//! "Nexus, complete this <more words>", "continue ...", "add to that ...",
//! "and also ...", "I wasn't done ...", "let me finish ...", etc.
//!
//! Pure module with zero side effects, no network, and no settings access.

/// The result of detecting a continuation phrase in an utterance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Continuation {
    /// Not a continuation phrase (fresh command).
    None,
    /// Continuation phrase matched; contains the remaining text (may be empty).
    Append(String),
}

/// Leading phrases indicating continuation.
///
/// Longer phrases precede shorter prefixes ("continue that" before "continue",
/// "i was not done" before "i wasn't done").
const PHRASES: &[&str] = &[
    "i wasn't done",
    "i was not done",
    "one more thing",
    "let me finish",
    "complete this",
    "complete that",
    "continue that",
    "add to that",
    "add to this",
    "keep going",
    "also add",
    "and also",
    "continue",
];

/// Noun/object blacklist for "complete this/that" and "continue".
/// If the subsequent text immediately refers to one of these objects, the phrase
/// is an ordinary command (e.g. "complete this form", "continue reading"),
/// NOT a continuation.
const OBJECT_BLACKLIST: &[&str] = &[
    "form",
    "forms",
    "task",
    "tasks",
    "article",
    "articles",
    "book",
    "books",
    "reading",
    "playing",
    "watching",
    "listening",
    "download",
    "downloads",
    "downloading",
    "video",
    "videos",
    "music",
    "song",
    "songs",
    "track",
    "tracks",
    "game",
    "games",
    "movie",
    "movies",
    "audio",
    "survey",
    "surveys",
    "quiz",
    "quizzes",
    "application",
    "applications",
    "sentence",
    "sentences",
    "assignment",
    "assignments",
    "project",
    "projects",
    "document",
    "documents",
];

/// Direction blacklist for "keep going".
/// e.g. "keep going straight", "keep going left" are navigation, not command continuation.
const DIRECTION_BLACKLIST: &[&str] = &[
    "straight",
    "forward",
    "forwards",
    "ahead",
    "left",
    "right",
    "north",
    "south",
    "east",
    "west",
    "backward",
    "backwards",
];

/// Strip leading wake-name prefixes (e.g. "nexus", "hey nexus") if present.
pub(crate) fn strip_wake_prefix(text: &str) -> &str {
    let mut current = text.trim();
    loop {
        let mut found = false;
        let lowered = current.to_lowercase();
        for prefix in crate::wake_names::wake_prefixes() {
            if lowered.starts_with(prefix.as_str()) && current.is_char_boundary(prefix.len()) {
                let rest = &current[prefix.len()..];
                let after = rest.chars().next();
                if after.is_none()
                    || matches!(
                        after,
                        Some(' ' | '\t' | '.' | ',' | '!' | '?' | ':' | '-' | ';' | '—')
                    )
                {
                    let cleaned = rest
                        .trim_start_matches(|c: char| {
                            matches!(c, '.' | ',' | '!' | '?' | ':' | '-' | ';' | '—' | ' ' | '\t')
                        })
                        .trim();
                    current = cleaned;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            break;
        }
    }
    current
}

/// Detect whether `text` starts with a continuation phrase.
///
/// Input can be raw or with the wake prefix already stripped.
/// Returns `Continuation::Append(rest)` where `rest` is the trailing words after the phrase,
/// or `Continuation::None` if it does not qualify as a continuation.
pub(crate) fn detect(text: &str) -> Continuation {
    let unstripped = text.trim();
    let text = strip_wake_prefix(unstripped);
    if text.is_empty() {
        return Continuation::None;
    }

    let lowered = text.to_lowercase();
    for &phrase in PHRASES {
        if lowered.starts_with(phrase) && text.is_char_boundary(phrase.len()) {
            let remainder = &text[phrase.len()..];
            let next_char = remainder.chars().next();
            // Word boundary check: must be followed by whitespace, punctuation, or end-of-text
            if let Some(ch) = next_char {
                if !ch.is_whitespace() && !matches!(ch, '.' | ',' | '!' | '?' | ':' | ';' | '-' | '—') {
                    continue;
                }
            }

            let rest = remainder
                .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ':' | ';' | '-' | '—'))
                .trim();

            if rest.is_empty() {
                return Continuation::Append(String::new());
            }

            // Extract words for blacklist checks
            let words: Vec<String> = rest
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase())
                .collect();

            if words.is_empty() {
                return Continuation::Append(String::new());
            }

            // Blacklist checks based on matched phrase:
            if phrase == "complete this" || phrase == "complete that" || phrase == "continue" || phrase == "continue that" {
                let first = words[0].as_str();
                if OBJECT_BLACKLIST.contains(&first) {
                    return Continuation::None;
                }
                // Handle "continue the <object>" or "continue with the <object>"
                if first == "the" {
                    if let Some(second) = words.get(1) {
                        if OBJECT_BLACKLIST.contains(&second.as_str()) {
                            return Continuation::None;
                        }
                    }
                } else if first == "with" {
                    if let Some(second) = words.get(1) {
                        if OBJECT_BLACKLIST.contains(&second.as_str()) {
                            return Continuation::None;
                        }
                        if second == "the" {
                            if let Some(third) = words.get(2) {
                                if OBJECT_BLACKLIST.contains(&third.as_str()) {
                                    return Continuation::None;
                                }
                            }
                        }
                    }
                }
            } else if phrase == "keep going" {
                let first = words[0].as_str();
                if DIRECTION_BLACKLIST.contains(&first) {
                    return Continuation::None;
                }
            } else if phrase == "and also" || phrase == "also add" {
                let first = words[0].as_str();
                if first == "known" {
                    return Continuation::None;
                }
            }

            return Continuation::Append(rest.to_string());
        }
    }

    Continuation::None
}

/// Merge previous command text and continuation text.
///
/// Prev trimmed + " " + rest trimmed.
/// Strips a trailing "." from `prev` when `rest` is non-empty to prevent double punctuation.
pub(crate) fn merge(prev: &str, rest: &str) -> String {
    let p = prev.trim();
    let r = rest.trim();
    if r.is_empty() {
        return p.to_string();
    }
    if p.is_empty() {
        return r.to_string();
    }
    let p_clean = p.strip_suffix('.').unwrap_or(p).trim_end();
    format!("{p_clean} {r}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_exact_phrases_empty_rest() {
        assert_eq!(detect("complete this"), Continuation::Append("".into()));
        assert_eq!(detect("complete that"), Continuation::Append("".into()));
        assert_eq!(detect("continue"), Continuation::Append("".into()));
        assert_eq!(detect("continue that"), Continuation::Append("".into()));
        assert_eq!(detect("keep going"), Continuation::Append("".into()));
        assert_eq!(detect("add to that"), Continuation::Append("".into()));
        assert_eq!(detect("add to this"), Continuation::Append("".into()));
        assert_eq!(detect("and also"), Continuation::Append("".into()));
        assert_eq!(detect("also add"), Continuation::Append("".into()));
        assert_eq!(detect("i wasn't done"), Continuation::Append("".into()));
        assert_eq!(detect("i was not done"), Continuation::Append("".into()));
        assert_eq!(detect("let me finish"), Continuation::Append("".into()));
        assert_eq!(detect("one more thing"), Continuation::Append("".into()));
    }

    #[test]
    fn test_detect_with_punctuation() {
        assert_eq!(detect("complete this."), Continuation::Append("".into()));
        assert_eq!(detect("continue,"), Continuation::Append("".into()));
        assert_eq!(detect("keep going!"), Continuation::Append("".into()));
        assert_eq!(detect("let me finish..."), Continuation::Append("".into()));
        assert_eq!(detect("i wasn't done:"), Continuation::Append("".into()));
        assert_eq!(detect("and also;"), Continuation::Append("".into()));
        assert_eq!(detect("one more thing -"), Continuation::Append("".into()));
    }

    #[test]
    fn test_detect_with_rest() {
        assert_eq!(
            detect("complete this: open chrome"),
            Continuation::Append("open chrome".into())
        );
        assert_eq!(
            detect("continue, and check pull requests"),
            Continuation::Append("and check pull requests".into())
        );
        assert_eq!(
            detect("continue that search on duckduckgo"),
            Continuation::Append("search on duckduckgo".into())
        );
        assert_eq!(
            detect("keep going and write tests"),
            Continuation::Append("and write tests".into())
        );
        assert_eq!(
            detect("add to that and make it quick"),
            Continuation::Append("and make it quick".into())
        );
        assert_eq!(
            detect("add to this with extra logging"),
            Continuation::Append("with extra logging".into())
        );
        assert_eq!(
            detect("and also open the console"),
            Continuation::Append("open the console".into())
        );
        assert_eq!(
            detect("also add git status"),
            Continuation::Append("git status".into())
        );
        assert_eq!(
            detect("i wasn't done, run cargo check"),
            Continuation::Append("run cargo check".into())
        );
        assert_eq!(
            detect("i was not done, show me the error"),
            Continuation::Append("show me the error".into())
        );
        assert_eq!(
            detect("let me finish please"),
            Continuation::Append("please".into())
        );
        assert_eq!(
            detect("one more thing: delete the tmp file"),
            Continuation::Append("delete the tmp file".into())
        );
    }

    #[test]
    fn test_detect_with_wake_prefix() {
        assert_eq!(detect("nexus, complete this"), Continuation::Append("".into()));
        assert_eq!(detect("hey nexus continue"), Continuation::Append("".into()));
        assert_eq!(
            detect("ok nexus, and also open terminal"),
            Continuation::Append("open terminal".into())
        );
        assert_eq!(
            detect("hello nexus let me finish"),
            Continuation::Append("".into())
        );
    }

    #[test]
    fn test_detect_negative_blacklist() {
        assert_eq!(detect("complete this form"), Continuation::None);
        assert_eq!(detect("complete this task"), Continuation::None);
        assert_eq!(detect("complete this article"), Continuation::None);
        assert_eq!(detect("complete this book"), Continuation::None);
        assert_eq!(detect("complete this survey"), Continuation::None);
        assert_eq!(detect("complete this download"), Continuation::None);
        assert_eq!(detect("complete that form"), Continuation::None);
        assert_eq!(detect("complete that task"), Continuation::None);
        assert_eq!(detect("complete that project"), Continuation::None);
        assert_eq!(detect("continue reading the article"), Continuation::None);
        assert_eq!(detect("continue playing the song"), Continuation::None);
        assert_eq!(detect("continue watching youtube"), Continuation::None);
        assert_eq!(detect("continue listening to podcast"), Continuation::None);
        assert_eq!(detect("continue download"), Continuation::None);
        assert_eq!(detect("continue downloading"), Continuation::None);
        assert_eq!(detect("continue the article"), Continuation::None);
        assert_eq!(detect("continue with the video"), Continuation::None);
        assert_eq!(detect("keep going straight"), Continuation::None);
        assert_eq!(detect("keep going forward"), Continuation::None);
        assert_eq!(detect("keep going left"), Continuation::None);
        assert_eq!(detect("also known as batman"), Continuation::None);
        assert_eq!(detect("and also known as dark knight"), Continuation::None);
    }

    #[test]
    fn test_detect_negative_general() {
        assert_eq!(detect("open chrome"), Continuation::None);
        assert_eq!(detect("hello nexus"), Continuation::None);
        assert_eq!(detect("continuation of the story"), Continuation::None);
        assert_eq!(detect("keep goingxyz"), Continuation::None);
        assert_eq!(detect(""), Continuation::None);
        assert_eq!(detect("   "), Continuation::None);
        assert_eq!(detect("nexus, complete this form"), Continuation::None);
    }

    #[test]
    fn test_merge() {
        assert_eq!(
            merge("open chrome.", "and go to google.com"),
            "open chrome and go to google.com"
        );
        assert_eq!(
            merge("open chrome", "and go to google.com"),
            "open chrome and go to google.com"
        );
        assert_eq!(merge("open chrome.", ""), "open chrome.");
        assert_eq!(merge("open chrome", ""), "open chrome");
        assert_eq!(merge("", "and go to google.com"), "and go to google.com");
        assert_eq!(
            merge("  run test.  ", "  --nocapture  "),
            "run test --nocapture"
        );
        assert_eq!(
            merge("what is rust?", "and how does borrow checker work"),
            "what is rust? and how does borrow checker work"
        );
    }
}
