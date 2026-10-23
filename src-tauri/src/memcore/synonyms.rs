//! Query-side concept families for personal-fact recall (Memory v2, phase M1).
//!
//! Lexical search only finds a fact when the question shares a word with it: "where do I work" can
//! not reach `employer`. People use a handful of different words for the same part of their life, so
//! each family below lists words that mean (nearly) the same thing; a question word expands to the
//! rest of its family. Expansions are only searched when the exact words did not fill the result list,
//! and they rank below exact hits (see `Store::search`).
//!
//! Deliberately small and general (profile slots people actually store), not tuned to a test set. A
//! model-written alias per fact (the optional review, phase M4) is the next step if this leaves gaps.

use std::collections::HashMap;

use once_cell::sync::Lazy;

const FITNESS: &[&str] = &["gym", "workout", "exercise", "exercises", "fitness", "training", "running", "lifting"];
const WAKE: &[&str] = &["wake", "awake", "alarm", "sunrise", "rise", "morning"];
const NICKNAME: &[&str] = &["nickname", "call", "called", "alias", "handle"];
const RESTRICTIONS: &[&str] = &[
    "allergy", "allergic", "allergies", "intolerance", "intolerant", "avoid", "restriction", "diet", "vegetarian",
    "vegan", "eat", "eating", "food", "meat", "meal",
];

const FAMILIES: &[&[&str]] = &[
    &[
        "work", "works", "working", "worked", "job", "jobs", "employer", "employed", "employment", "company",
        "firm", "office", "workplace", "career", "occupation", "profession",
    ],
    &["manager", "boss", "supervisor", "superior"],
    &[
        "live", "lives", "living", "reside", "residence", "home", "address", "city", "town", "hometown", "village",
        "from", "native",
    ],
    &["birthday", "birth", "born", "birthdate", "dob", "age", "old"],
    &["sibling", "siblings", "sister", "brother"],
    &["parent", "parents", "mother", "mom", "mum", "father", "dad"],
    &["cousin", "aunt", "uncle", "relative", "relatives"],
    &["wife", "husband", "spouse", "partner", "girlfriend", "boyfriend"],
    &["son", "daughter", "child", "children", "kids", "kid"],
    &[
        "study", "studying", "student", "college", "university", "school", "education", "degree", "course", "major",
        "campus",
    ],
    &["music", "song", "songs", "listen", "listening", "genre", "playlist", "artist", "band", "singer", "tune"],
    WAKE,
    &["car", "cars", "drive", "driving", "vehicle", "bike", "scooter", "motorcycle", "ride", "riding"],
    &["commute", "travel", "transport", "metro", "bus", "train"],
    RESTRICTIONS,
    FITNESS,
    &["phone", "mobile", "handset", "smartphone", "cellphone", "iphone", "android", "pixel"],
    &["coffee", "tea", "drink", "drinks", "drinking", "beverage", "latte", "espresso", "milk"],
    &["code", "coding", "program", "programs", "programming", "programmer", "developer", "software", "language"],
    NICKNAME,
    &["movie", "movies", "film", "films", "cinema", "watch", "watching", "series"],
    &["pet", "pets", "dog", "dogs", "cat", "cats", "puppy", "animal"],
    &["salary", "income", "pay", "paid", "wage", "earn", "earning"],
    &["bank", "account", "card", "loan"],
    &["hobby", "hobbies", "interest", "interests", "pastime"],
    &["book", "books", "read", "reading", "novel", "author"],
    &["sport", "sports", "cricket", "football", "game", "games", "gaming", "console"],
    &["laptop", "computer", "keyboard", "mouse", "device"],
    &["wifi", "internet", "network", "router"],
    &["doctor", "medicine", "medication", "health", "blood"],
    &["news", "newspaper", "magazine"],
    &["trip", "vacation", "holiday", "flight", "visit"],
    &["season", "weather", "monsoon", "winter", "summer"],
];

/// Multi-word phrases whose meaning differs from their parts ("work out" is fitness, not a job).
/// The phrase's own words are not expanded individually.
const PHRASES: &[(&str, &[&str])] = &[
    ("work out", FITNESS),
    ("working out", FITNESS),
    ("worked out", FITNESS),
    ("wake up", WAKE),
    ("get up", WAKE),
    ("woke up", WAKE),
    ("call me", NICKNAME),
    ("eat meat", RESTRICTIONS),
];

/// Longest list of extra tokens one question may add, so the search query stays small.
const MAX_EXTRA: usize = 40;

/// Light stemmer: enough to match endings ("studying" -> "study", "allergic" -> "allerg").
pub fn stem(word: &str) -> String {
    let w = word.to_lowercase();
    for (suffix, replacement) in [
        ("ies", "y"),
        ("ing", ""),
        ("ical", ""),
        ("ally", ""),
        ("ic", ""),
        ("ed", ""),
        ("ly", ""),
        ("es", ""),
        ("s", ""),
    ] {
        if let Some(base) = w.strip_suffix(suffix) {
            if base.chars().count() >= 3 {
                return format!("{base}{replacement}");
            }
        }
    }
    w
}

static INDEX: Lazy<HashMap<String, Vec<usize>>> = Lazy::new(|| {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, family) in FAMILIES.iter().enumerate() {
        for word in family.iter() {
            for key in [word.to_string(), stem(word)] {
                let entry = map.entry(key).or_default();
                if !entry.contains(&i) {
                    entry.push(i);
                }
            }
        }
    }
    map
});

fn candidates(token: &str) -> Vec<String> {
    let t = token.to_lowercase();
    let s = stem(&t);
    let mut out = vec![t.clone()];
    if s != t {
        out.push(format!("{s}e")); // "driving" -> "driv" -> "drive"
        out.push(s);
    }
    out
}

/// Extra tokens for a question: the other members of every concept family one of its words belongs
/// to. Never returns a token the question already has.
pub fn expand_query(query: &str, tokens: &[String]) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut extra: Vec<String> = Vec::new();
    let mut consumed: Vec<&str> = Vec::new();

    let push = |word: &str, extra: &mut Vec<String>| {
        if word.chars().count() > 2 && !tokens.iter().any(|t| t == word) && !extra.iter().any(|e| e == word) {
            extra.push(word.to_string());
        }
    };

    for (phrase, words) in PHRASES {
        if lower.contains(phrase) {
            consumed.extend(phrase.split(' '));
            for w in words.iter() {
                push(w, &mut extra);
            }
        }
    }

    let mut families: Vec<usize> = Vec::new();
    for token in tokens {
        if consumed.contains(&token.as_str()) {
            continue;
        }
        for c in candidates(token) {
            if let Some(found) = INDEX.get(&c) {
                for f in found {
                    if !families.contains(f) {
                        families.push(*f);
                    }
                }
            }
        }
    }
    for f in families {
        for w in FAMILIES[f].iter() {
            push(w, &mut extra);
        }
    }
    extra.truncate(MAX_EXTRA);
    extra
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(q: &str) -> Vec<String> {
        super::super::store::query_tokens(q)
    }

    #[test]
    fn stems_common_endings() {
        assert_eq!(stem("studying"), "study");
        assert_eq!(stem("allergic"), "allerg");
        assert_eq!(stem("works"), "work");
        assert_eq!(stem("cars"), "car");
        assert_eq!(stem("is"), "is"); // too short to strip
    }

    #[test]
    fn a_word_expands_to_its_family_and_not_to_itself() {
        let q = "where do I work";
        let extra = expand_query(q, &toks(q));
        assert!(extra.iter().any(|w| w == "employer"), "{extra:?}");
        assert!(extra.iter().any(|w| w == "job"), "{extra:?}");
        assert!(!extra.iter().any(|w| w == "work"), "must not repeat the question's own word");
    }

    #[test]
    fn inflected_question_words_still_find_their_family() {
        let q = "what was I studying";
        let extra = expand_query(q, &toks(q));
        assert!(extra.iter().any(|w| w == "college"), "{extra:?}");
        let q = "what do I drive";
        assert!(expand_query(q, &toks(q)).iter().any(|w| w == "car"));
        let q = "do I like driving";
        assert!(expand_query(q, &toks(q)).iter().any(|w| w == "car"));
    }

    #[test]
    fn a_phrase_beats_its_parts() {
        let q = "when do I work out";
        let extra = expand_query(q, &toks(q));
        assert!(extra.iter().any(|w| w == "gym"), "{extra:?}");
        assert!(!extra.iter().any(|w| w == "employer"), "'work out' is not a job question: {extra:?}");
    }

    #[test]
    fn unrelated_questions_add_nothing_and_the_list_is_bounded() {
        let q = "tell me something interesting about volcanoes";
        assert!(expand_query(q, &toks(q)).iter().all(|w| w != "employer"));
        let q = "work job home live school music wake car phone coffee code movie pet bank hobby book sport";
        assert!(expand_query(q, &toks(q)).len() <= MAX_EXTRA);
    }

    #[test]
    fn families_have_no_duplicate_members_and_are_lowercase() {
        for family in FAMILIES {
            let mut seen = std::collections::HashSet::new();
            for w in family.iter() {
                assert_eq!(*w, w.to_lowercase(), "{w}");
                assert!(seen.insert(*w), "duplicate {w} in {family:?}");
            }
        }
    }
}
