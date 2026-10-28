//! Who the owner is, taken from their connected Google account (name only is spoken or sent; the
//! email is stored for the user profile but never put in the always-on cloud block).
//!
//! Before this, nothing wrote the account's name into memory, so the cloud context never held it and
//! "what's my name" had no answer. The seed only FILLS a gap: a name the owner told NEXUS themselves
//! ("remember my name is ...") is never overwritten.

use std::path::Path;

const KEY_NAME: &str = "user_name";
const KEY_FIRST: &str = "user_first_name";

/// Pure: the name to store from an account list entry (`None` for an empty or email-like name).
pub fn usable_name(name: &str) -> Option<String> {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() || n.contains('@') || n.chars().count() > 60 || !n.chars().any(char::is_alphabetic) {
        return None;
    }
    Some(n)
}

fn first_name(full: &str) -> String {
    full.split_whitespace().next().unwrap_or(full).to_string()
}

/// Thin glue: writes `user_name` / `user_first_name` core facts when the owner has not set a name.
/// Returns true when something was stored. Gated by `memcoreAccountSeed` (default on).
pub fn seed(dir: &Path) -> bool {
    if !super::enabled(dir) || !super::flag(dir, "memcoreAccountSeed", true) {
        return false;
    }
    let accounts = crate::auth_vault::get_google_accounts();
    let primary = accounts.iter().find(|a| a.is_primary).or_else(|| accounts.first());
    let Some(name) = primary.and_then(|a| usable_name(&a.name)) else {
        return false;
    };
    seed_name(dir, &name)
}

/// Same, with the name given (testable without the OS key store).
pub fn seed_name(dir: &Path, name: &str) -> bool {
    if crate::memory::core_value(dir, KEY_NAME).is_some() || crate::memory::core_value(dir, "name").is_some() {
        return false;
    }
    let a = crate::memory::remember(dir, KEY_NAME, name);
    let b = crate::memory::remember(dir, KEY_FIRST, &first_name(name));
    a || b
}

/// The owner's name as NEXUS knows it (first name preferred when `first`).
pub fn known_name(dir: &Path, first: bool) -> Option<String> {
    let full = crate::memory::core_value(dir, "name").or_else(|| crate::memory::core_value(dir, KEY_NAME))?;
    Some(if first { crate::memory::core_value(dir, KEY_FIRST).unwrap_or_else(|| first_name(&full)) } else { full })
}

/// Pure: is this a "what is my name / who am I" question? Whole-sentence match so chat that merely
/// contains the words still goes to the normal path.
pub fn is_name_question(transcript: &str) -> bool {
    let t: String = transcript
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let t = t.trim_start_matches("nexus ").trim_start_matches("hey nexus ");
    matches!(
        t,
        "what's my name" | "whats my name" | "what is my name" | "do you know my name" | "who am i"
            | "do you know who i am" | "tell me my name" | "say my name"
    )
}

/// The spoken answer (no cloud call). `None` when no name is known.
pub fn name_answer(dir: &Path) -> Option<String> {
    known_name(dir, false).map(|n| format!("You're {n}, sir."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nexus_seed_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn names_are_cleaned_and_emails_refused() {
        assert_eq!(usable_name("  Asha   Rao ").as_deref(), Some("Asha Rao"));
        assert_eq!(usable_name("asha@gmail.com"), None);
        assert_eq!(usable_name(""), None);
        assert_eq!(usable_name("1234"), None);
    }

    #[test]
    fn the_seed_fills_a_gap_and_never_overwrites_what_the_owner_said() {
        let d = temp();
        assert!(seed_name(&d, "Asha Rao"));
        assert_eq!(known_name(&d, false).as_deref(), Some("Asha Rao"));
        assert_eq!(known_name(&d, true).as_deref(), Some("Asha"));
        assert!(!seed_name(&d, "Someone Else"), "an existing name is kept");
        assert_eq!(known_name(&d, false).as_deref(), Some("Asha Rao"));

        let d2 = temp();
        assert!(crate::memory::remember(&d2, "name", "Lakshya"));
        assert!(!seed_name(&d2, "Chitkul Lakshya"), "the owner's own words win");
        assert_eq!(known_name(&d2, false).as_deref(), Some("Lakshya"));
    }

    #[test]
    fn only_whole_name_questions_are_answered_locally() {
        for t in ["What's my name?", "nexus, who am I", "Do you know my name"] {
            assert!(is_name_question(t), "{t}");
        }
        for t in ["what is my name tag printer", "who am i to judge", "my name is Asha", "what's my balance"] {
            assert!(!is_name_question(t), "{t}");
        }
        let d = temp();
        assert_eq!(name_answer(&d), None);
        seed_name(&d, "Asha Rao");
        assert_eq!(name_answer(&d).as_deref(), Some("You're Asha Rao, sir."));
    }
}
