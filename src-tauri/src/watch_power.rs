//! "Watch this session until it is complete, and if it is perfect shut down my laptop."
//!
//! The utterance has three parts that people say in any order and with many words:
//!   * WATCH  - watch / monitor / keep an eye on / wait for ... (optional when an END clause is present)
//!   * END    - until it is complete / when claude finishes / once it is done ...
//!   * POWER  - shut down my laptop / power off the pc / turn off the computer ...
//! plus an optional condition ("if it is perfect", "only if there are no errors") and its opposite
//! ("even if it fails"), and an optional time limit ("within two hours").
//!
//! This module is a PURE parser (no clock, no I/O). Deciding that a session really ended and ended well,
//! and the shutdown itself, belong to later phases (doc 142). The parser is deliberately strict: a
//! sentence that does not name a session (or an agent, or a clear pronoun) and an end is never claimed,
//! so "watch this video until it ends then shut down" and "shut down notepad when it is done" are left
//! alone. A weak power phrase ("shut down", "turn it off") is only accepted inside a clear watch context,
//! and never for restart / sleep / lock / sign-out (those need an explicit "my laptop").

use crate::power_cmd::{self, PowerKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchPlan {
    /// Lowercase agent name ("claude", "codex", ...) or None = the session in front.
    pub agent: Option<String>,
    pub kind: PowerKind,
    /// Only power off when the session ended cleanly (the default, and the only safe one).
    pub require_clean: bool,
    /// Longest time to wait, when the user gave one.
    pub deadline_mins: Option<u32>,
}

const WATCH_VERBS: &[&str] = &[
    "watch over", "keep watch over", "keep watch on", "keep watching", "watch", "monitor", "keep an eye on",
    "keep eye on", "keep tabs on", "keep track of", "babysit", "supervise", "oversee", "observe", "track",
    "stay on", "stay with", "wait for", "wait until", "wait till", "look after", "see this through",
    "see it through", "follow",
];

/// Words that mean "this thing has ended" (single tokens).
const END_WORDS: &[&str] = &[
    "done", "finish", "finishes", "finished", "finishing", "complete", "completes", "completed", "completing",
    "stops", "stopped", "ends", "ended", "concludes", "concluded",
];

/// Things that can be watched to completion. An agent name counts too (see `agent_in`).
const SESSION_NOUNS: &[&str] = &[
    "session", "sessions", "task", "tasks", "run", "build", "job", "agent", "agents", "chat", "process", "work",
    "response", "conversation", "download", "downloads", "install", "installation", "update", "upload", "deployment",
    "deploy", "training", "render", "export", "compile", "compilation", "test", "tests", "script", "code", "project",
    "migration", "scan", "backup", "sync", "prompt", "request", "generation", "coding",
];

/// Entertainment and screen things: "watch this video ..." is not a session watch.
const MEDIA_NOUNS: &[&str] = &[
    "video", "videos", "movie", "film", "episode", "series", "song", "songs", "stream", "show", "match", "game",
    "page", "tab", "window", "screen", "ad", "ads", "clip", "reel", "reels", "cricket", "football", "news", "lecture",
    "tutorial", "podcast", "music", "playlist",
];

const AGENTS: &[(&str, &str)] = &[
    ("claude", "claude"),
    ("codex", "codex"),
    ("antigravity", "antigravity"),
    ("agy", "antigravity"),
    ("cursor", "cursor"),
    ("trae", "trae"),
    ("windsurf", "windsurf"),
    ("aider", "aider"),
    ("opencode", "opencode"),
    ("hermes", "hermes"),
    ("gemini", "gemini"),
    ("copilot", "copilot"),
    ("vscode", "vscode"),
];

const QUESTION_STARTS: &[&str] = &[
    "how", "what", "why", "which", "who", "whose", "where", "is", "are", "was", "were", "did", "does", "do", "should", "would",
];

/// Words that may directly follow a bare power phrase ("shut down when ...", "shut it down if ...").
const CLAUSE_STARTERS: &[&str] = &[
    "when", "once", "after", "if", "as", "but", "and", "then", "even", "only", "whenever", "in", "within", "by", "at",
    "because", "so", "until", "before", "should", "provided", "assuming", "unless", "regardless",
];

const FILLER_PREFIXES: &[&str] = &[
    "hey nexus", "nexus", "hey", "okay", "ok", "so", "alright", "well", "please", "can you", "can u", "could you", "would you",
    "will you", "i want you to", "i need you to", "i'd like you to", "i would like you to", "go ahead and", "just", "also",
    "and", "right", "yeah", "um", "uh",
];

/// Lowercase, straight apostrophes, punctuation -> spaces, single spaces.
fn norm(text: &str) -> String {
    text.to_lowercase()
        .replace('\u{2019}', "'")
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_fillers(mut t: String) -> String {
    loop {
        let before = t.clone();
        for p in FILLER_PREFIXES {
            if let Some(rest) = t.strip_prefix(p) {
                if rest.is_empty() || rest.starts_with(' ') {
                    t = rest.trim().to_string();
                }
            }
        }
        if t == before {
            return t;
        }
    }
}

fn find_phrase(tokens: &[&str], phrase: &str) -> Option<(usize, usize)> {
    let p: Vec<&str> = phrase.split(' ').collect();
    if p.is_empty() || tokens.len() < p.len() {
        return None;
    }
    (0..=tokens.len() - p.len()).find(|&i| tokens[i..i + p.len()] == p[..]).map(|i| (i, i + p.len()))
}

fn is_first_person_before(tokens: &[&str], i: usize) -> bool {
    let prev = if i >= 1 { tokens[i - 1] } else { "" };
    let prev2 = if i >= 2 { tokens[i - 2] } else { "" };
    matches!(prev, "i'm" | "im" | "we're" | "you're" | "youre" | "i've" | "we've")
        || (matches!(prev, "am" | "are" | "have" | "had") && matches!(prev2, "i" | "we" | "you"))
}

/// Index of the first END word that is about a THING finishing (not "I'm done").
fn find_end(tokens: &[&str]) -> Option<usize> {
    for (i, t) in tokens.iter().enumerate() {
        if END_WORDS.contains(t) && !is_first_person_before(tokens, i) {
            return Some(i);
        }
        // "wraps up", "wrapped up"
        if matches!(*t, "wraps" | "wrapped") && tokens.get(i + 1) == Some(&"up") && !is_first_person_before(tokens, i) {
            return Some(i);
        }
    }
    None
}

fn is_pronoun(t: &str) -> bool {
    matches!(t, "this" | "it" | "that" | "it's" | "its")
}

/// The power phrase: (start, end, kind). Strict phrases are tried first (any length 1..=7); a weak
/// shutdown phrase counts only when followed by a clause word or the end of the sentence.
fn find_power(tokens: &[&str], weak_allowed: bool) -> Option<(usize, usize, PowerKind)> {
    let n = tokens.len();
    for start in 0..n {
        for len in (1..=7.min(n - start)).rev() {
            let slice = tokens[start..start + len].join(" ");
            if let Some(kind) = power_cmd::parse(&slice) {
                return Some((start, start + len, kind));
            }
        }
    }
    if weak_allowed {
        for start in 0..n {
            for len in (1..=4.min(n - start)).rev() {
                let slice = tokens[start..start + len].join(" ");
                if power_cmd::parse_weak(&slice) == Some(PowerKind::Shutdown) {
                    let end = start + len;
                    let ok_next = end == n || CLAUSE_STARTERS.contains(&tokens[end]);
                    if ok_next {
                        return Some((start, end, PowerKind::Shutdown));
                    }
                }
            }
        }
    }
    None
}

fn negated_before(tokens: &[&str], start: usize) -> bool {
    let prev = if start >= 1 { tokens[start - 1] } else { "" };
    let prev2 = if start >= 2 { tokens[start - 2] } else { "" };
    matches!(prev, "don't" | "dont" | "not" | "never" | "without" | "no" | "didn't" | "won't")
        || (prev == "not" && prev2 == "do")
        || (matches!(prev, "to" | "you") && matches!(prev2, "not" | "never" | "don't"))
}

fn agent_in(tokens: &[&str]) -> Option<String> {
    for (i, t) in tokens.iter().enumerate() {
        if let Some((_, canon)) = AGENTS.iter().find(|(alias, _)| alias == t) {
            return Some((*canon).to_string());
        }
        if *t == "vs" && tokens.get(i + 1) == Some(&"code") {
            return Some("vscode".to_string());
        }
        if *t == "open" && tokens.get(i + 1) == Some(&"code") {
            return Some("opencode".to_string());
        }
    }
    None
}

fn number_word(w: &str) -> Option<u32> {
    Some(match w {
        "a" | "an" | "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "twelve" => 12,
        "fifteen" => 15,
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "sixty" => 60,
        _ => return w.parse::<u32>().ok(),
    })
}

/// "within two hours", "no more than 30 minutes", "in 3 hours at the latest", "half an hour".
fn deadline(tokens: &[&str]) -> Option<u32> {
    for (i, t) in tokens.iter().enumerate() {
        let lead_ok = matches!(*t, "within" | "than")
            || (*t == "in" && tokens[i..].windows(3).any(|w| w == ["at", "the", "latest"] || w[..2] == ["at", "most"]))
            || (*t == "up" && tokens.get(i + 1) == Some(&"to"));
        if !lead_ok {
            continue;
        }
        let mut j = i + 1;
        if *t == "up" {
            j += 1;
        }
        if tokens.get(j) == Some(&"half") {
            return Some(30);
        }
        let Some(n) = tokens.get(j).and_then(|w| number_word(w)) else { continue };
        let Some(unit) = tokens.get(j + 1) else { continue };
        if unit.starts_with("hour") || *unit == "hr" || *unit == "hrs" {
            return Some(n * 60);
        }
        if unit.starts_with("min") {
            return Some(n);
        }
    }
    None
}

fn contains_seq(tokens: &[&str], seq: &[&str]) -> bool {
    tokens.windows(seq.len()).any(|w| w == seq)
}

/// "even if it fails" / "no matter what" -> do not require a clean ending, unless it is negated
/// ("but not if it fails").
fn require_clean(tokens: &[&str]) -> bool {
    let fail_words = ["fail", "fails", "failed", "error", "errors", "break", "breaks", "broken", "wrong", "problem", "problems", "crash", "crashes"];
    // "don't / not / never ... if (it) <fail>"
    let mut negated_if_fail = false;
    for (i, t) in tokens.iter().enumerate() {
        if matches!(*t, "don't" | "dont" | "not" | "never" | "unless") {
            let after = &tokens[i + 1..tokens.len().min(i + 6)];
            if after.iter().any(|w| fail_words.contains(w)) {
                negated_if_fail = true;
            }
        }
    }
    if negated_if_fail {
        return true;
    }
    let even_if = contains_seq(tokens, &["even", "if"])
        || contains_seq(tokens, &["even", "when"])
        || contains_seq(tokens, &["no", "matter", "what"])
        || contains_seq(tokens, &["whatever", "happens"])
        || contains_seq(tokens, &["either", "way"])
        || tokens.contains(&"regardless")
        || tokens.contains(&"anyway");
    !even_if
}

/// Parse the whole utterance. `None` = not a watch-then-power request.
pub fn parse(text: &str) -> Option<WatchPlan> {
    let t = strip_fillers(norm(text));
    if t.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = t.split(' ').collect();
    if QUESTION_STARTS.contains(&tokens[0]) {
        return None;
    }

    // WATCH verb (longest match, earliest position)
    let mut watch: Option<(usize, usize)> = None;
    for v in WATCH_VERBS {
        if let Some((s, e)) = find_phrase(&tokens, v) {
            // "watch" inside a longer verb ("watch over") is found first by list order
            if watch.map_or(true, |(ws, we)| s < ws || (s == ws && e > we)) {
                watch = Some((s, e));
            }
        }
    }
    let end = find_end(&tokens);
    if watch.is_none() && end.is_none() {
        return None;
    }

    let agent = agent_in(&tokens);
    let session_noun = tokens.iter().any(|w| SESSION_NOUNS.contains(w));

    // The thing being watched: media windows reject, otherwise an agent / session noun / clear pronoun accepts.
    if let Some((_, we)) = watch {
        let window = &tokens[we..tokens.len().min(we + 3)];
        if window.iter().any(|w| MEDIA_NOUNS.contains(w)) {
            return None;
        }
    }
    if let Some(ei) = end {
        let window = &tokens[ei.saturating_sub(3)..ei];
        if window.iter().any(|w| MEDIA_NOUNS.contains(w)) {
            return None;
        }
    }
    let pronoun_subject = watch.map_or(false, |(_, we)| tokens[we..tokens.len().min(we + 2)].iter().any(|w| is_pronoun(w)))
        || end.map_or(false, |ei| tokens[ei.saturating_sub(3)..ei].iter().any(|w| is_pronoun(w)));
    let subject_ok = agent.is_some() || session_noun || pronoun_subject;
    if !subject_ok {
        return None;
    }

    // Weak power phrases ("shut down", "turn it off") only with an explicit watch verb, an agent or a session word.
    let weak_allowed = watch.is_some() || agent.is_some() || session_noun;
    let (ps, _pe, kind) = find_power(&tokens, weak_allowed)?;
    if negated_before(&tokens, ps) {
        return None;
    }
    // A watch verb that IS the power phrase's own word ("follow", "track") cannot also be the power phrase.
    if let Some((ws, we)) = watch {
        if ps >= ws && ps < we {
            return None;
        }
    }

    Some(WatchPlan {
        agent,
        kind,
        require_clean: require_clean(&tokens),
        deadline_mins: deadline(&tokens),
    })
}

/// What NEXUS says back (also the text a later phase asks the user to confirm).
pub fn read_back(plan: &WatchPlan) -> String {
    let who = plan.agent.as_deref().map(|a| a.to_string()).unwrap_or_else(|| "this session".to_string());
    let action = match plan.kind {
        PowerKind::Shutdown => "shut down your laptop",
        PowerKind::Restart => "restart your laptop",
        PowerKind::Sleep => "put your laptop to sleep",
        PowerKind::Lock => "lock your screen",
        PowerKind::SignOut => "sign you out",
    };
    let cond = if plan.require_clean { "only if it looks perfect" } else { "even if it does not end cleanly" };
    let limit = plan.deadline_mins.map(|m| format!(", waiting at most {m} minutes")).unwrap_or_default();
    format!("Watch {who} until it finishes, then {action}, {cond}{limit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shut(p: &str) -> WatchPlan {
        parse(p).unwrap_or_else(|| panic!("not claimed: {p:?}"))
    }

    #[test]
    fn the_users_own_sentence_and_its_many_wordings() {
        let phrases = [
            "watch over this session until it's complete and if it's perfect shut down my laptop",
            "nexus can you watch over this session until complete if it is perfect shut down my laptop",
            "nexus, can you watch this session and when it's done, if everything is perfect, power off the pc",
            "when claude finishes, if there are no errors, shut down my laptop",
            "keep an eye on this and turn off the computer once it's finished successfully",
            "if the build passes and the agent is done, shut my laptop down",
            "shut down my laptop after this finishes, but only if it went well",
            "monitor codex, and when it's done and all tests pass, switch off the laptop",
            "wait for this to finish and then shut down, but don't if there are errors",
            "once antigravity is done and it looks perfect, shut down the system",
            "stay on this session; if it completes without problems, power the laptop down",
            "let this session finish, check it's perfect, then shut down my pc",
            "I'm going to bed, watch this and shut down when it's perfect",
            "watch this task until it ends and shut down the laptop if all is good",
            "keep watching my coding agent and turn off my computer when it's finished",
            "babysit this build and power down the pc when it is complete",
            "supervise this run, and if nothing fails, shut down my laptop",
            "track this download until it's done then shut down my laptop",
            "watch it until it's finished and then shut it down if it was successful",
            "please monitor cursor and shut down my machine when it stops",
            "watch over claude code and once it has finished running shut down the laptop",
            "wait until this session is complete, then if everything went well turn the laptop off",
            "see this through and shut down my laptop when it's done and perfect",
            "after the agent is done and there are no issues shut down my laptop",
            "can u watch this and shut down my laptop if it completes perfectly",
            "watch over this and power off the computer once it has completed",
        ];
        let mut n = 0;
        for p in phrases {
            let plan = shut(p);
            assert_eq!(plan.kind, PowerKind::Shutdown, "{p}");
            assert!(plan.require_clean, "{p}");
            n += 1;
        }
        assert!(n >= 25);
    }

    #[test]
    fn order_swaps_and_wrappers_do_not_matter() {
        let parts_watch = ["watch this session", "monitor this session", "keep an eye on this session", "wait for this session"];
        let parts_end = ["until it is complete", "until it's done", "until it finishes", "until it has finished"];
        let parts_power = ["shut down my laptop", "power off my pc", "turn off the computer", "shut my laptop down"];
        let parts_gate = ["if it's perfect", "only if everything went well", "if there are no errors", "if it succeeds"];
        let mut n = 0;
        for w in parts_watch {
            for e in parts_end {
                for p in parts_power {
                    for g in parts_gate {
                        for p2 in [
                            format!("{w} {e} and {g} {p}"),
                            format!("{w} {e}, {p} {g}"),
                            format!("{p} {g} {e} but {w} first"),
                            format!("nexus please {w} {e}. {g}, {p}."),
                        ] {
                            let plan = parse(&p2).unwrap_or_else(|| panic!("not claimed: {p2:?}"));
                            assert_eq!(plan.kind, PowerKind::Shutdown, "{p2}");
                            n += 1;
                        }
                    }
                }
            }
        }
        assert!(n >= 1000, "{n}");
    }

    #[test]
    fn the_agent_and_the_options_are_picked_up() {
        assert_eq!(shut("when claude finishes shut down my laptop").agent.as_deref(), Some("claude"));
        assert_eq!(shut("when codex is done, power off the pc").agent.as_deref(), Some("codex"));
        assert_eq!(shut("when antigravity is done shut down my laptop").agent.as_deref(), Some("antigravity"));
        assert_eq!(shut("watch agy until it's done and shut down my laptop").agent.as_deref(), Some("antigravity"));
        assert_eq!(shut("when vs code finishes shut down my laptop").agent.as_deref(), Some("vscode"));
        assert_eq!(shut("when open code is done shut down my laptop").agent.as_deref(), Some("opencode"));
        assert_eq!(shut("watch this until it's done and shut down my laptop").agent, None);

        assert!(shut("watch this until it's done and shut down my laptop").require_clean);
        assert!(!shut("watch this until it's done and shut down my laptop even if it fails").require_clean);
        assert!(!shut("shut down my laptop when this finishes no matter what").require_clean);
        assert!(!shut("when it's done shut down my laptop either way, watch this session").require_clean);
        assert!(shut("watch this until it's done and shut down my laptop but not if it fails").require_clean);
        assert!(shut("watch this until it's done and shut down my laptop, don't if something fails").require_clean);

        assert_eq!(shut("watch this until it's done and shut down my laptop within two hours").deadline_mins, Some(120));
        assert_eq!(shut("watch this until it's done, shut down my laptop, no more than 30 minutes").deadline_mins, Some(30));
        assert_eq!(shut("watch this session and shut down my laptop when done within half an hour").deadline_mins, Some(30));
        assert_eq!(shut("watch this until it's done and shut down my laptop").deadline_mins, None);

        assert_eq!(shut("watch this until it's done and restart my laptop").kind, PowerKind::Restart);
        assert_eq!(shut("when claude is done put my laptop to sleep").kind, PowerKind::Sleep);
        assert_eq!(shut("when this finishes lock my laptop and watch it").kind, PowerKind::Lock);
    }

    #[test]
    fn weak_power_words_need_a_clear_watch_context_and_only_mean_shutdown() {
        assert_eq!(shut("watch this session until it's done and then shut down").kind, PowerKind::Shutdown);
        assert_eq!(shut("monitor claude, when it finishes turn it off").kind, PowerKind::Shutdown);
        assert_eq!(shut("when the agent finishes, power off").kind, PowerKind::Shutdown);
        // pronoun-only subject without a watch verb: too ambiguous
        assert_eq!(parse("shut it down when it's done"), None);
        assert_eq!(parse("turn it off once it's finished"), None);
        // bare restart / sleep / lock are never taken from a watch sentence
        for p in [
            "watch this session until it's done then restart",
            "when claude finishes put it to sleep",
            "watch this until it's done and lock it",
            "when the build finishes sign me out",
        ] {
            assert_eq!(parse(p), None, "{p}");
        }
    }

    #[test]
    fn other_sentences_are_left_alone() {
        for p in [
            // not a watch plan at all
            "shut down my laptop", "watch over this session", "watch this", "tell me when claude finishes", "when claude finishes open vs code",
            "open claude", "shut down notepad when it's done", "shut down nexus when it's done", "turn off the lights when it's done",
            "restart the download when it fails", "lock the door when the session ends", "put it to sleep when it's done",
            // media and screen things
            "watch this video until it's done then shut down my laptop", "watch the game until it's over and shut down my laptop",
            "watch this movie and shut down my laptop when it finishes", "watch this episode till it ends then shut down my laptop",
            // first person
            "when i'm done shut down my laptop", "once we're finished shut down the laptop", "when i am done turn off my pc",
            "when i'm done with this session shut down my laptop", "once i've finished this task shut down the laptop",
            "when i am done with the build power off my pc",
            // a bare "shut down" aimed at something else inside a watch sentence
            "watch this session until it's done then shut down notepad", "when claude finishes shut down the music",
            "monitor this build and when it's done shut down the server",
            // questions and explanations
            "how do i shut down my laptop when the build finishes", "what happens if i shut down my laptop when claude is done",
            "can you explain how to shut down my laptop after the session", "did you shut down my laptop when it finished",
            // negated
            "don't shut down my laptop when it's done", "do not shut down the laptop when claude finishes", "never turn off my computer when the session ends",
            "watch this session but without shutting down my laptop", "",
        ] {
            assert_eq!(parse(p), None, "{p:?}");
        }
    }

    #[test]
    fn read_back_names_every_part() {
        let p = shut("watch claude until it's done and shut down my laptop within two hours");
        let s = read_back(&p);
        assert!(s.contains("claude") && s.contains("shut down your laptop") && s.contains("only if it looks perfect") && s.contains("120 minutes"), "{s}");
        let q = shut("watch this until it's done and shut down my laptop even if it fails");
        let s = read_back(&q);
        assert!(s.contains("this session") && s.contains("even if it does not end cleanly"), "{s}");
    }
}
