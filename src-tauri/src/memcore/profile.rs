//! The always-on profile (Memory v2, phase M1; Hermes Agent's `USER.md`).
//!
//! A small, bounded block of what the user themselves told NEXUS ("remember that ..."), attached to
//! every cloud request. It fixes two things at once: facts the question's words cannot reach
//! (retrieval is lexical) and the prompt prefix staying identical between turns, which lets providers
//! that cache by prefix (Groq, Gemini) reuse it.
//!
//! Only pinned, `user_said` facts qualify: never mined guesses, never untrusted text. Sensitive
//! categories (health, money, identity numbers, credentials, beliefs) are excluded - they remain
//! searchable when a question asks for them, but are not sent on every turn. The block is capped; what
//! does not fit is skipped, never silently truncated mid-line. The order is by creation time, so the
//! text only changes when the user's facts do.

/// Character cap of the whole block (about 350 tokens). Hermes uses 1,375 for its user profile.
pub const PROFILE_CAP: usize = 1400;
/// One entry's value is clipped to this many characters.
const MAX_VALUE: usize = 120;
const HEADER: &str = "About the user (they told you this):";

/// Key fragments that mark a fact as too sensitive to send on every turn.
const SENSITIVE: &[&str] = &[
    "allerg", "medic", "health", "diagnos", "disease", "illness", "condition", "blood", "doctor", "therap",
    "salary", "income", "wage", "bank", "account", "iban", "card", "loan", "debt", "tax", "religio", "caste",
    "politic", "sexual", "orientation", "password", "passcode", "secret", "otp", "aadhaar", "aadhar", "passport",
    "ssn", "licen", "address",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: i64,
    pub key: String,
    pub value: String,
}

/// Matches whole key words by prefix ("taxes" yes, "syntax_theme" no).
pub fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_lowercase();
    if k.contains("phone_number") || k.contains("mobile_number") {
        return true;
    }
    k.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .any(|w| w == "pin" || SENSITIVE.iter().any(|s| w.starts_with(s)))
}

/// Render the profile block and return the ids it contains (so recall does not repeat them).
/// Empty text = nothing qualifies.
pub fn render(entries: &[Entry], cap: usize) -> (String, Vec<i64>) {
    let mut lines: Vec<String> = Vec::new();
    let mut ids: Vec<i64> = Vec::new();
    let mut used = HEADER.chars().count();
    for e in entries {
        if is_sensitive_key(&e.key) || e.value.trim().is_empty() {
            continue;
        }
        let value: String = e.value.trim().chars().take(MAX_VALUE).collect();
        let line = format!("- {}: {}", e.key, value);
        let cost = line.chars().count() + 1; // + newline
        if used + cost > cap {
            continue; // a shorter entry later may still fit
        }
        used += cost;
        lines.push(line);
        ids.push(e.id);
    }
    if lines.is_empty() {
        return (String::new(), Vec::new());
    }
    (format!("{HEADER}\n{}", lines.join("\n")), ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: i64, key: &str, value: &str) -> Entry {
        Entry { id, key: key.into(), value: value.into() }
    }

    #[test]
    fn renders_in_given_order_with_a_header() {
        let (text, ids) = render(&[e(1, "dog_name", "Bruno"), e(2, "employer", "Acme")], PROFILE_CAP);
        assert_eq!(text, "About the user (they told you this):\n- dog_name: Bruno\n- employer: Acme");
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn sensitive_and_empty_entries_are_left_out() {
        let (text, ids) = render(
            &[
                e(1, "allergy", "peanuts"),
                e(2, "bank_account", "1234"),
                e(3, "home_address", "12 Road"),
                e(4, "religion", "x"),
                e(5, "dog_name", "Bruno"),
                e(6, "empty_one", "   "),
            ],
            PROFILE_CAP,
        );
        assert!(text.contains("dog_name: Bruno"));
        for banned in ["peanuts", "1234", "12 Road", "religion", "empty_one"] {
            assert!(!text.contains(banned), "{banned} must not be always-on: {text}");
        }
        assert_eq!(ids, vec![5]);
    }

    #[test]
    fn sensitivity_is_decided_per_word_not_per_substring() {
        assert!(is_sensitive_key("phone_number"));
        assert!(is_sensitive_key("bank_pin"));
        assert!(is_sensitive_key("taxes_paid"));
        assert!(!is_sensitive_key("phone_model"));
        assert!(!is_sensitive_key("syntax_theme"));
        assert!(!is_sensitive_key("favorite_pizza_topping"));
    }

    #[test]
    fn the_cap_is_respected_and_a_shorter_later_entry_can_still_fit() {
        let long = "x".repeat(120);
        let entries = vec![e(1, "a_long_one", &long), e(2, "b_long_one", &long), e(3, "tiny", "ok")];
        let cap = HEADER.chars().count() + ("- a_long_one: ".chars().count() + 120 + 1) + 15;
        let (text, ids) = render(&entries, cap);
        assert!(text.chars().count() <= cap, "{} > {cap}", text.chars().count());
        assert_eq!(ids, vec![1, 3], "second long entry skipped, tiny one fits");
    }

    #[test]
    fn long_values_are_clipped_and_empty_input_gives_nothing() {
        let (text, _) = render(&[e(1, "bio", &"y".repeat(500))], PROFILE_CAP);
        assert!(text.lines().nth(1).unwrap().chars().count() <= "- bio: ".chars().count() + MAX_VALUE);
        assert_eq!(render(&[], PROFILE_CAP), (String::new(), vec![]));
    }

    #[test]
    fn output_is_stable_for_the_same_input() {
        let entries = vec![e(1, "dog_name", "Bruno"), e(2, "employer", "Acme")];
        assert_eq!(render(&entries, PROFILE_CAP), render(&entries, PROFILE_CAP));
    }
}
