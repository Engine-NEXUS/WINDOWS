//! Spoken wording that changes every time, for the lines said around the reopen question at logon.
//!
//! Every line here is short (at most 9 words), has no digits and no addresses, so it is cacheable (`tts_cache_disk`): the
//! stock lines are rendered once at start-up and then played from disk with nothing synthesised, like the greeting. The line
//! with a day in it ("... from yesterday") is rendered the first time it is chosen, while the greeting is still playing.
//!
//! Choosing is pure (`choose`): the same line never comes twice in a row, otherwise it follows a seed (the clock).
//! The last choice per bank is kept in the memory store (`meta`) so "twice in a row" holds across restarts too.

/// The banks. Each has its own "last said" memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bank {
    /// "Shall I bring back your last session?" (no day)
    Question,
    /// The same question naming the day ("... from yesterday"): `{day}` is replaced.
    QuestionDay,
    /// Said at once after a yes.
    AckYes,
    /// After a no.
    AckNo,
    /// After "later" / "not now".
    AckLater,
    /// After everything opened (the apps open while the acknowledgement plays; this is only for "reopen my session").
    Done,
}

impl Bank {
    fn key(self) -> &'static str {
        match self {
            Bank::Question => "phrase_last_question",
            Bank::QuestionDay => "phrase_last_question_day",
            Bank::AckYes => "phrase_last_ack_yes",
            Bank::AckNo => "phrase_last_ack_no",
            Bank::AckLater => "phrase_last_ack_later",
            Bank::Done => "phrase_last_done",
        }
    }

    pub fn lines(self) -> &'static [&'static str] {
        match self {
            Bank::Question => QUESTION,
            Bank::QuestionDay => QUESTION_DAY,
            Bank::AckYes => ACK_YES,
            Bank::AckNo => ACK_NO,
            Bank::AckLater => ACK_LATER,
            Bank::Done => DONE,
        }
    }
}

const QUESTION: &[&str] = &[
    "Want me to pick up where you left off?",
    "Shall I bring back your last session?",
    "Should I reopen what you had open?",
    "Shall I start where you left off, sir?",
    "Want your last session back, sir?",
    "Shall I reopen everything, sir?",
    "Should I open your last apps again?",
    "Want me to bring everything back?",
    "Shall I get your apps going again?",
    "Ready to reopen your last session, sir?",
    "Shall I set you up where you stopped?",
    "Do you want your apps back, sir?",
];

const QUESTION_DAY: &[&str] = &[
    "Shall I open everything from {day}?",
    "Want the apps from {day} back, sir?",
    "Bring back what you had open {day}, sir?",
    "Shall I reopen your session from {day}?",
    "Do you want {day}'s apps back?",
    "Pick up where you left off {day}, sir?",
];

const ACK_YES: &[&str] = &[
    "On it, sir.",
    "Right away.",
    "Opening them now.",
    "Bringing them back.",
    "Consider it done, sir.",
    "Getting them up now.",
    "One moment, sir.",
    "Sure thing, sir.",
    "Coming right up.",
    "Reopening them now, sir.",
];

const ACK_NO: &[&str] = &[
    "Understood, sir.",
    "No problem, sir.",
    "Alright, I'll leave them closed.",
    "Very well, sir.",
    "Okay, nothing will open.",
    "As you wish, sir.",
];

const ACK_LATER: &[&str] = &[
    "Very well, sir. I'll keep the list.",
    "Alright, I'll hold on to it.",
    "No rush, sir. Say reopen my session anytime.",
    "Okay, I'll keep it for later.",
    "Sure, sir. The list stays.",
];

const DONE: &[&str] = &[
    "All back, sir.",
    "They're all open, sir.",
    "Everything's open again, sir.",
    "Done, sir. Your session is back.",
    "That's everything reopened, sir.",
    "All set, sir.",
];

/// Pick a line index: never `last` (when there is a choice), otherwise by `seed`. Pure.
pub fn choose(len: usize, last: Option<usize>, seed: u64) -> usize {
    if len <= 1 {
        return 0;
    }
    // a small mix so consecutive seconds do not walk the bank in order
    let mixed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 33;
    let mut i = (mixed % len as u64) as usize;
    if Some(i) == last {
        i = (i + 1 + (mixed as usize / len) % (len - 1)) % len;
    }
    i
}

/// The lines every start renders once into the voice cache (the one with a day is rendered when first chosen).
pub fn stock_lines() -> impl Iterator<Item = &'static str> {
    // (AckLater is not rendered up front: for the reopen question a "later" is read as a no, see `offer::classify_for`)
    [Bank::Question, Bank::AckYes, Bank::AckNo, Bank::Done].into_iter().flat_map(|b| b.lines().iter().copied())
}

/// Every line of every bank, with `{day}` left in (for tests and the cache-limit check).
pub fn all_templates() -> impl Iterator<Item = &'static str> {
    [Bank::Question, Bank::QuestionDay, Bank::AckYes, Bank::AckNo, Bank::AckLater, Bank::Done]
        .into_iter()
        .flat_map(|b| b.lines().iter().copied())
}

/// The next line of `bank`, remembering it so it is not repeated next time. With a day phrase the template's `{day}` is replaced.
/// Blocking (the memory store); callers run it on a worker thread or accept the few milliseconds.
pub fn next(dir: &std::path::Path, bank: Bank, day: Option<&str>) -> String {
    let lines = bank.lines();
    let last = super::with_store(dir, |s| s.meta_get(bank.key()).and_then(|v| v.parse::<usize>().ok())).flatten();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let i = choose(lines.len(), last, seed);
    super::with_store(dir, |s| s.meta_set(bank.key(), &i.to_string()));
    fill(lines[i], day)
}

fn fill(template: &str, day: Option<&str>) -> String {
    match day {
        Some(d) => template.replace("{day}", d),
        None => template.replace("{day}", "last time"),
    }
}

/// The reopen question: a day-free line, or (about 1 in 3) one that names the day.
pub fn reopen_question(dir: &std::path::Path, day: &str) -> String {
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let with_day = !day.trim().is_empty() && (seed >> 7) % 3 == 0;
    if with_day {
        next(dir, Bank::QuestionDay, Some(day))
    } else {
        next(dir, Bank::Question, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_line_is_short_digit_free_and_therefore_cacheable() {
        for t in all_templates() {
            // the longest day phrase is "on Wednesday" (2 words)
            let filled = fill(t, Some("on Wednesday"));
            assert!(crate::tts_cache_disk::cacheable(&filled), "not cacheable: {filled}");
            assert!(filled.split_whitespace().count() <= crate::tts_cache_disk::MAX_WORDS, "too long: {filled}");
        }
    }

    #[test]
    fn banks_are_big_enough_and_have_no_duplicates() {
        assert!(Bank::Question.lines().len() >= 10);
        assert!(Bank::AckYes.lines().len() >= 8);
        let mut seen = HashSet::new();
        for t in all_templates() {
            assert!(seen.insert(crate::tts_cache_disk::norm(t)), "duplicate line: {t}");
        }
    }

    #[test]
    fn the_same_line_never_comes_twice_in_a_row() {
        for len in [2usize, 3, 6, 12] {
            let mut last = None;
            for seed in 0..5_000u64 {
                let i = choose(len, last, seed.wrapping_mul(0x9E37_79B9));
                assert!(i < len);
                assert_ne!(Some(i), last, "len {len} seed {seed}");
                last = Some(i);
            }
        }
        assert_eq!(choose(1, Some(0), 7), 0, "a bank of one cannot avoid repeating");
    }

    #[test]
    fn over_many_starts_every_line_gets_used() {
        let len = Bank::Question.lines().len();
        let mut used = HashSet::new();
        let mut last = None;
        for seed in 0..400u64 {
            let i = choose(len, last, seed * 7_919);
            used.insert(i);
            last = Some(i);
        }
        assert_eq!(used.len(), len, "all {len} variants appear");
    }

    #[test]
    fn the_day_is_filled_in_and_a_missing_day_never_leaves_a_brace() {
        assert_eq!(fill("Shall I open everything from {day}?", Some("yesterday")), "Shall I open everything from yesterday?");
        assert!(!fill("Shall I open everything from {day}?", None).contains('{'));
    }

    #[test]
    fn the_stock_lines_exclude_the_day_lines_and_fit_the_cache() {
        let stock: Vec<&str> = stock_lines().collect();
        assert!(stock.iter().all(|l| !l.contains("{day}")));
        // 35 older stock lines + these + at most 6 templates x 10 day phrases must stay well under the cache bound
        assert!(35 + stock.len() + Bank::QuestionDay.lines().len() * 10 < crate::tts_cache_disk::MAX_ENTRIES);
    }

    #[test]
    fn the_store_remembers_the_last_line_so_a_restart_does_not_repeat_it() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("nexus_phrase_bank_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let mut previous = String::new();
        for round in 0..40 {
            let line = next(&dir, Bank::AckYes, None);
            assert!(Bank::AckYes.lines().contains(&line.as_str()), "a line of the bank: {line}");
            assert_ne!(line, previous, "round {round}: the same acknowledgement twice in a row");
            previous = line;
        }
        let q = reopen_question(&dir, "yesterday");
        assert!(q.ends_with('?') && !q.contains('{'), "{q}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
