//! Personality tone (F0): butler (default, formal "sir") vs friend opt-in.
//!
//! Pure helpers — no I/O. Settings live in `commands.rs` (`persona_mode`).

/// True when the stored mode string selects friend tone. Unknown → butler.
pub fn is_friend(mode: &str) -> bool {
    mode.trim().eq_ignore_ascii_case("friend")
}

/// How to address the user: butler → always "sir"; friend → first name
/// when known, otherwise no address (callers skip the comma phrase).
/// Returns None only for friend-without-name.
pub fn address(name: Option<&str>, friend: bool) -> Option<String> {
    if !friend {
        return Some("sir".to_string());
    }
    match name.map(|n| n.trim()).filter(|n| !n.is_empty()) {
        Some(n) => Some(n.to_string()),
        None => None,
    }
}

/// "Hello, sir." vs "Hey, Lakshya." vs "Hey." — comma phrase included.
pub fn greet_address(name: Option<&str>, friend: bool) -> String {
    match address(name, friend) {
        Some(a) => format!(", {a}"),
        None => String::new(),
    }
}

/// Restyle a canned butler greeting for friend mode: ", sir" becomes
/// ", {name}" (or vanishes when the name is unknown). Pure + tested —
/// applied once at the orchestrator hook so every pick-list is covered.
pub fn restyle_greeting(reply: &str, name: Option<&str>) -> String {
    let addr = greet_address(name, true);
    reply.replace(", sir", &addr).replace("  ", " ")
}

/// Words that may precede a vocative name ("Hi Lakshya.", "Okay Lakshya,").
const LEAD_IN: &[&str] = &["hi", "hello", "hey", "ok", "okay", "thanks", "thank", "sure", "yes", "no", "right", "well", "good"];

/// Butler mode guard, independent of what the model was told: when a reply addresses the owner by name
/// ("Sure, Lakshya." / "Lakshya, here you go") the name becomes "sir". Only the vocative position is touched
/// (name right after a comma, the start of the text or a lead-in word, and followed by punctuation or the end),
/// so a repository, file or person that merely shares the name is left alone. Pure.
pub fn enforce_address(reply: &str, names: &[String]) -> String {
    let wanted: Vec<String> = names
        .iter()
        .flat_map(|n| n.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).collect::<Vec<_>>())
        .filter(|w| w.chars().count() >= 3 && w != "sir")
        .collect();
    if wanted.is_empty() {
        return reply.to_string();
    }
    let chars: Vec<char> = reply.chars().collect();
    let mut out = String::with_capacity(reply.len());
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_alphanumeric() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '\'') {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let lower = word.to_lowercase();
        let hit = wanted.contains(&lower) && {
            // previous non-space char / previous word
            let mut p = start;
            while p > 0 && chars[p - 1] == ' ' {
                p -= 1;
            }
            let prev_ok = p == 0 || matches!(chars[p - 1], ',' | '.' | '!' | '?' | '\n') || {
                let mut q = p;
                while q > 0 && chars[q - 1].is_alphanumeric() {
                    q -= 1;
                }
                let prev_word: String = chars[q..p].iter().collect::<String>().to_lowercase();
                LEAD_IN.contains(&prev_word.as_str())
            };
            let mut n = i;
            while n < chars.len() && chars[n] == ' ' {
                n += 1;
            }
            let next_ok = n >= chars.len() || matches!(chars[n], ',' | '.' | '!' | '?' | ';' | ':' | '\n');
            prev_ok && next_ok
        };
        if hit {
            // keep sentence-initial capitalisation: "sir" mid-sentence, "Sir" at the start
            let at_start = out.trim_end().is_empty() || out.trim_end().ends_with(['.', '!', '?']);
            out.push_str(if at_start { "Sir" } else { "sir" });
        } else {
            out.push_str(&word);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> Vec<String> {
        vec!["chitkul lakshya.".into(), "Lakshya".into(), "Lakshya Official".into()]
    }

    #[test]
    fn name_used_as_address_becomes_sir() {
        assert_eq!(enforce_address("Sure, Lakshya.", &me()), "Sure, sir.");
        assert_eq!(enforce_address("Lakshya, here you go.", &me()), "Sir, here you go.");
        assert_eq!(enforce_address("Hi Lakshya! It is sunny.", &me()), "Hi sir! It is sunny.");
        assert_eq!(enforce_address("Done, Chitkul.", &me()), "Done, sir.");
    }

    #[test]
    fn names_that_are_content_are_left_alone() {
        assert_eq!(enforce_address("Your repo Lakshya Official has 3 PRs.", &me()), "Your repo Lakshya Official has 3 PRs.");
        assert_eq!(enforce_address("Opening the lakshya folder.", &me()), "Opening the lakshya folder.");
        assert_eq!(enforce_address("Sure, sir.", &me()), "Sure, sir.");
        assert_eq!(enforce_address("Sure, Lakshya.", &[]), "Sure, Lakshya.");
    }

    #[test]
    fn test_butler_always_sir() {
        assert_eq!(address(None, false).as_deref(), Some("sir"));
        assert_eq!(address(Some("Lakshya"), false).as_deref(), Some("sir"));
        assert_eq!(address(Some("Lakshya"), true).as_deref(), Some("Lakshya"));
        assert_eq!(address(None, true), None);
        assert!(!is_friend("butler"));
        assert!(!is_friend("FRIENDLY"));
        assert!(!is_friend(""));
        assert!(is_friend("friend"));
        assert!(is_friend("Friend"));
    }

    #[test]
    fn test_greet_address_shapes() {
        assert_eq!(greet_address(None, false), ", sir");
        assert_eq!(greet_address(Some("Lakshya"), true), ", Lakshya");
        assert_eq!(greet_address(None, true), "");
    }

    #[test]
    fn test_restyle_greeting_canned_lists() {
        // With a name.
        assert_eq!(
            restyle_greeting("Hello, sir.", Some("Lakshya")),
            "Hello, Lakshya."
        );
        assert_eq!(
            restyle_greeting("Hi, sir. How can I help?", Some("Lakshya")),
            "Hi, Lakshya. How can I help?"
        );
        assert_eq!(
            restyle_greeting("At your service, sir.", Some("Lakshya")),
            "At your service, Lakshya."
        );
        // Without a name the address vanishes cleanly (no double space).
        assert_eq!(restyle_greeting("Hello, sir.", None), "Hello.");
        assert_eq!(
            restyle_greeting("Hey, sir. What can I do for you?", None),
            "Hey. What can I do for you?"
        );
        // Replies without ", sir" pass through untouched.
        assert_eq!(restyle_greeting("All systems green. Ready?", Some("Lakshya")), "All systems green. Ready?");
    }
}
