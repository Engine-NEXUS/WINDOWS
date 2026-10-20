//! One source of truth for how the assistant's name can be written by STT.
//!
//! Two consumers used to keep their own lists and disagreed:
//! - the wake-word stage-2 verifier (`wakeword_oww::verify_transcript_gated`) —
//!   "did the speech engine hear the wake word?";
//! - `intent_parser::strip_wake_prefix` — "drop the wake word the hot mic
//!   captured, then parse the command".
//!
//! A real "nexus" is transcribed as lexus, nixus, nexis, "next us", "next sis"
//! (measured on owner recordings; see the wake-data poisoning rescue in
//! AGENTS.md). A name form the verifier accepted but the stripper did not
//! produced "lexus open chrome" → no command.
//!
//! Policy: [`EXACT`] is trusted on its own. Every other form is a *near* form —
//! the verifier additionally requires a strong neural score for those (two-key),
//! so a lone soundalike on TV cannot wake the assistant. Deliberately excluded
//! because they are everyday words: texas, next, access, "next is", "next test".

/// Spelled exactly right: trusted at any neural score.
pub const EXACT: &[&str] = &["nexus"];

/// Single-word STT confusions (phoneme edit distance ~1; lexus 37%, nixus/nixis
/// dominant spellings, nexat/nexo truncations, nexas/nekus artifacts, plus
/// doubled-letter spellings).
pub const NEAR_WORDS: &[&str] = &[
    "lexus", "nexis", "nixus", "nixis", "nexas", "nexuss", "nekus", "lexis",
    "nexat", "nexo", "nexos", "nex", "knex", "mexus", "mexis", "nexxus",
    "nexes", "nixes",
];

/// Two-word confusions ("nexus" heard as a split pair).
pub const NEAR_PAIRS: &[(&str, &str)] = &[
    ("next", "us"),
    ("next", "sis"),
    ("next", "sense"),
    ("necks", "us"),
];

/// Words that may precede the name ("hey nexus", "ok nexus").
pub const GREETINGS: &[&str] = &["hello", "okay", "hey", "ok", "hi"];

/// How a transcript matched the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameMatch {
    /// The exact name is present.
    Exact,
    /// Only a near form is present (needs a high neural score to count).
    Near,
    None,
}

/// Classify a transcript already split into lowercase alphanumeric words.
pub fn classify(words: &[String]) -> NameMatch {
    if words.iter().any(|w| EXACT.contains(&w.as_str())) {
        return NameMatch::Exact;
    }
    let near_word = words.iter().any(|w| NEAR_WORDS.contains(&w.as_str()));
    let near_pair = words
        .windows(2)
        .any(|p| NEAR_PAIRS.iter().any(|(a, b)| p[0] == *a && p[1] == *b));
    if near_word || near_pair {
        NameMatch::Near
    } else {
        NameMatch::None
    }
}

/// Every spoken-name form, space separated, longest (most words) first.
fn name_forms() -> Vec<String> {
    let mut v: Vec<String> = EXACT.iter().chain(NEAR_WORDS.iter()).map(|s| s.to_string()).collect();
    v.extend(NEAR_PAIRS.iter().map(|(a, b)| format!("{a} {b}")));
    v
}

/// Prefixes `strip_wake_prefix` removes: "<greeting> <name>" and bare "<name>",
/// longest first so "hey nexus" is tried before "nexus". Built once.
pub fn wake_prefixes() -> &'static [String] {
    static CELL: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        let forms = name_forms();
        let mut out: Vec<String> = Vec::new();
        for g in GREETINGS {
            for f in &forms {
                out.push(format!("{g} {f}"));
            }
        }
        out.extend(forms);
        out.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect()
    }

    #[test]
    fn test_classify_exact_near_pair_and_none() {
        assert_eq!(classify(&w("hey nexus")), NameMatch::Exact);
        assert_eq!(classify(&w("lexus nexus")), NameMatch::Exact);
        for near in ["lexus", "hey nixus", "Nexxus!", "next us", "next, sis", "necks us", "nixes"] {
            assert_eq!(classify(&w(near)), NameMatch::Near, "{near}");
        }
        for none in ["", "next", "texas", "next is the budget", "next test", "access", "us next", "alexa play music"] {
            assert_eq!(classify(&w(none)), NameMatch::None, "{none}");
        }
    }

    #[test]
    fn test_every_listed_form_is_a_prefix_and_longest_first() {
        let p = wake_prefixes();
        assert!(p.contains(&"hey nexus".to_string()));
        assert!(p.contains(&"nexus".to_string()));
        assert!(p.contains(&"okay next us".to_string()));
        for f in EXACT.iter().chain(NEAR_WORDS.iter()) {
            assert!(p.contains(&f.to_string()), "{f}");
        }
        let pos = |s: &str| p.iter().position(|x| x == s).unwrap();
        assert!(pos("hey nexus") < pos("nexus"));
        assert!(pos("hey next us") < pos("next us"));
        // No form is both exact and near.
        for e in EXACT {
            assert!(!NEAR_WORDS.contains(e));
        }
    }
}
