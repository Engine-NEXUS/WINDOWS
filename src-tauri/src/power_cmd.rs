//! "Nexus shut down my laptop" and friends.
//!
//! Before this existed `"shut down"` was only a *close-app* verb, so "shut down
//! my laptop" parsed as "close the app called 'my laptop'". Power commands are
//! parsed first, strictly (the target must be the computer itself), confirmed
//! by voice when they end the session, and always preceded by a session
//! snapshot so the next boot can offer to reopen what was open.
//!
//! The actual call happens after a short, cancellable countdown ("say cancel to
//! stop"), through Windows' own `shutdown` so apps with unsaved work still get
//! their "save?" prompts instead of being killed.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerKind {
    Shutdown,
    Restart,
    Sleep,
    Lock,
    SignOut,
}

impl PowerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PowerKind::Shutdown => "shutdown",
            PowerKind::Restart => "restart",
            PowerKind::Sleep => "sleep",
            PowerKind::Lock => "lock",
            PowerKind::SignOut => "signout",
        }
    }

    pub fn from_str(s: &str) -> Option<PowerKind> {
        Some(match s {
            "shutdown" => PowerKind::Shutdown,
            "restart" => PowerKind::Restart,
            "sleep" => PowerKind::Sleep,
            "lock" => PowerKind::Lock,
            "signout" => PowerKind::SignOut,
            _ => return None,
        })
    }

    /// Ends the user's session: asks first, and snapshots what is open.
    pub fn ends_session(self) -> bool {
        matches!(self, PowerKind::Shutdown | PowerKind::Restart | PowerKind::SignOut)
    }

    /// Whether the PC comes back to the same open windows (sleep/lock), so no snapshot is needed.
    pub fn needs_snapshot(self) -> bool {
        self.ends_session()
    }

    /// Seconds between the spoken announcement and the call.
    pub fn countdown_secs(self) -> u64 {
        match self {
            PowerKind::Shutdown | PowerKind::Restart | PowerKind::SignOut => 12,
            PowerKind::Sleep => 4,
            PowerKind::Lock => 1,
        }
    }

    pub fn ask(self) -> &'static str {
        match self {
            PowerKind::Shutdown => "Shut down your laptop, sir? I'll note what's open first so I can offer to reopen it.",
            PowerKind::Restart => "Restart your laptop, sir? I'll note what's open first so I can offer to reopen it.",
            PowerKind::SignOut => "Sign you out of Windows, sir? I'll note what's open first.",
            PowerKind::Sleep => "Putting the laptop to sleep.",
            PowerKind::Lock => "Locking the screen.",
        }
    }

    /// The "did you mean it?" question for a phrase that only might be a power command.
    pub fn confirm_question(self) -> &'static str {
        match self {
            PowerKind::Shutdown => "Did you want me to shut down your laptop, sir?",
            PowerKind::Restart => "Did you want me to restart your laptop, sir?",
            PowerKind::Sleep => "Did you want me to put the laptop to sleep, sir?",
            PowerKind::Lock => "Did you want me to lock the screen, sir?",
            PowerKind::SignOut => "Did you want me to sign you out of Windows, sir?",
        }
    }

    pub fn verb(self) -> &'static str {
        match self {
            PowerKind::Shutdown => "powering off",
            PowerKind::Restart => "restarting",
            PowerKind::SignOut => "signing out",
            PowerKind::Sleep => "going to sleep",
            PowerKind::Lock => "locking",
        }
    }
}

// ─── Parsing ─────────────────────────────────────────────────────────

const POLITE_PREFIXES: &[&str] = &[
    "nexus ",
    "hey nexus ",
    "hey ",
    "okay ",
    "ok ",
    "alright ",
    "so ",
    "now ",
    "just ",
    "please ",
    "can you ",
    "could you ",
    "would you ",
    "will you ",
    "go ahead and ",
    "i want you to ",
    "i need you to ",
    "i'd like you to ",
    "i would like you to ",
    "i'd like to ",
    "i want to ",
    "let's ",
    "lets ",
];
const POLITE_SUFFIXES: &[&str] = &[
    " right now",
    " immediately",
    " now",
    " please",
    " for me",
    " nexus",
    " thanks",
    " thank you",
    " tonight",
    " for the night",
    " for today",
    " okay",
    " ok",
    " will you",
    " sir",
];
/// The only things a power verb may be aimed at.
const COMPUTER_WORDS: &[&str] = &["laptop", "computer", "pc", "system", "machine", "windows", "device", "notebook"];

fn clean(text: &str) -> String {
    let mut t = text.trim().to_lowercase().replace('\u{2019}', "'").replace(',', " ");
    t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    t = t.trim_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | ' ' | ';' | ':')).to_string();
    loop {
        let before = t.clone();
        for p in POLITE_PREFIXES {
            if let Some(rest) = t.strip_prefix(p) {
                t = rest.trim().to_string();
            }
        }
        for s in POLITE_SUFFIXES {
            if let Some(rest) = t.strip_suffix(s) {
                t = rest.trim().to_string();
            }
        }
        if t == before {
            return t;
        }
    }
}

/// "my laptop", "the computer", "windows pc", "of windows" -> only computer words remain.
fn is_computer(rest: &str, allow_screen: bool) -> bool {
    let words: Vec<&str> = rest
        .split_whitespace()
        .filter(|w| !matches!(*w, "my" | "the" | "this" | "your" | "of" | "off" | "down" | "out" | "to" | "all"))
        .collect();
    if words.is_empty() {
        return false;
    }
    words
        .iter()
        .all(|w| COMPUTER_WORDS.contains(w) || (allow_screen && matches!(*w, "screen" | "workstation")))
}

/// Strict parse of a power command. `None` for everything else - in
/// particular "shut down notepad" (a close-app command) and "shut down nexus".
/// Whisper hears "shut down" as these (live log 2026-10-15: "send on my laptop", "set down my laptop",
/// "to set down my laptop" - all three fell through to the cloud, which explained how to shut down).
/// Only honoured when what follows is exactly the computer itself, so "set down my coffee" never matches.
const SHUTDOWN_MISHEARINGS: &[&str] = &["set down", "sat down", "shot down", "shut dawn", "shutdown", "send on", "shut it down", "shot dawn", "shutten", "check down", "chuck down", "shuck down", "tick on", "send down"];

fn fix_shutdown_mishearing(t: &str) -> Option<String> {
    let t = t.strip_prefix("to ").unwrap_or(t);
    for bad in SHUTDOWN_MISHEARINGS {
        if let Some(rest) = t.strip_prefix(bad) {
            if rest.starts_with(' ') && is_computer(rest.trim(), false) && rest.split_whitespace().count() <= 3 {
                return Some(format!("shut down{rest}"));
            }
        }
    }
    None
}

pub fn parse(text: &str) -> Option<PowerKind> {
    let t = clean(text);
    if t.is_empty() {
        return None;
    }
    let t = fix_shutdown_mishearing(&t).unwrap_or(t);

    // "put my laptop to sleep", "send the computer to sleep", "put the pc on standby"
    for lead in ["put ", "send "] {
        if let Some(rest) = t.strip_prefix(lead) {
            if let Some(mid) = rest
                .strip_suffix(" to sleep")
                .or_else(|| rest.strip_suffix(" in sleep mode"))
                .or_else(|| rest.strip_suffix(" on standby"))
                .or_else(|| rest.strip_suffix(" in standby"))
            {
                return is_computer(mid, false).then_some(PowerKind::Sleep);
            }
        }
    }
    // "sign me out of windows" / "log me out of my computer"
    let t = if t.starts_with("sign me ") || t.starts_with("log me ") { t.replacen(" me", "", 1) } else { t };
    // "turn the laptop off", "power my pc down", "give the laptop a restart"
    if let Some(kind) = parse_reordered(&t) {
        return Some(kind);
    }

    let table: &[(&str, PowerKind)] = &[
        ("shut down", PowerKind::Shutdown),
        ("shutdown", PowerKind::Shutdown),
        ("power off", PowerKind::Shutdown),
        ("power down", PowerKind::Shutdown),
        ("shut off", PowerKind::Shutdown),
        ("close down", PowerKind::Shutdown),
        ("turn off", PowerKind::Shutdown),
        ("switch off", PowerKind::Shutdown),
        ("restart", PowerKind::Restart),
        ("reboot", PowerKind::Restart),
        ("sleep", PowerKind::Sleep),
        ("hibernate", PowerKind::Sleep),
        ("suspend", PowerKind::Sleep),
        ("standby", PowerKind::Sleep),
        ("lock", PowerKind::Lock),
        ("sign out", PowerKind::SignOut),
        ("log out", PowerKind::SignOut),
        ("log off", PowerKind::SignOut),
        ("sign off", PowerKind::SignOut),
    ];
    for (verb, kind) in table {
        if let Some(rest) = t.strip_prefix(verb) {
            // The verb must end at a word boundary ("locked" is not "lock").
            if !rest.is_empty() && !rest.starts_with(' ') {
                continue;
            }
            if is_computer(rest.trim(), *kind == PowerKind::Lock) {
                return Some(*kind);
            }
        }
    }
    None
}

/// The object sits between the verb and its particle: "turn the laptop off", "power my pc down",
/// "shut my computer down", "close down the system", "give the laptop a restart".
fn parse_reordered(t: &str) -> Option<PowerKind> {
    let tokens: Vec<&str> = t.split_whitespace().collect();
    if tokens.len() < 3 {
        return None;
    }
    let (verb, last) = (tokens[0], *tokens.last()?);
    let mid = tokens[1..tokens.len() - 1].join(" ");
    let paired = match verb {
        "turn" | "switch" => matches!(last, "off" | "offline"),
        "power" | "shut" => matches!(last, "off" | "down"),
        "close" | "take" => last == "down",
        _ => false,
    };
    if paired {
        return is_computer(&mid, false).then_some(PowerKind::Shutdown);
    }
    if verb == "give" && matches!(last, "restart" | "reboot") {
        let mid = mid.strip_suffix(" a").or_else(|| mid.strip_suffix(" a quick")).unwrap_or(&mid);
        return is_computer(mid, false).then_some(PowerKind::Restart);
    }
    None
}

/// Bare power verbs with no object: "shut down", "power off", "turn it off", "restart", "log me out".
/// "shut down" alone could be meant for any app, so these are never obeyed silently: the caller asks
/// first (and a yes teaches the phrase for the shutdown kind). The WHOLE utterance must be the verb.
const WEAK_PHRASES: &[(&str, PowerKind)] = &[
    ("shut down", PowerKind::Shutdown),
    ("shutdown", PowerKind::Shutdown),
    ("shut it down", PowerKind::Shutdown),
    ("shut everything down", PowerKind::Shutdown),
    ("shut off", PowerKind::Shutdown),
    ("shut it off", PowerKind::Shutdown),
    ("power off", PowerKind::Shutdown),
    ("power it off", PowerKind::Shutdown),
    ("power down", PowerKind::Shutdown),
    ("power it down", PowerKind::Shutdown),
    ("turn off", PowerKind::Shutdown),
    ("turn it off", PowerKind::Shutdown),
    ("switch off", PowerKind::Shutdown),
    ("switch it off", PowerKind::Shutdown),
    ("restart", PowerKind::Restart),
    ("restart it", PowerKind::Restart),
    ("reboot", PowerKind::Restart),
    ("reboot it", PowerKind::Restart),
    ("suspend", PowerKind::Sleep),
    ("put it to sleep", PowerKind::Sleep),
    ("lock", PowerKind::Lock),
    ("lock it", PowerKind::Lock),
    ("sign out", PowerKind::SignOut),
    ("sign me out", PowerKind::SignOut),
    ("log out", PowerKind::SignOut),
    ("log me out", PowerKind::SignOut),
    ("log off", PowerKind::SignOut),
];

pub fn parse_weak(text: &str) -> Option<PowerKind> {
    // "restart nexus" / "shut down nexus" aim the verb AT the assistant (a vocative needs a comma: "shut down, nexus").
    let raw = text.trim().to_lowercase().replace('\u{2019}', "'");
    let raw = raw.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ' '));
    if raw.ends_with(" nexus") && !raw.ends_with(", nexus") {
        return None;
    }
    let t = clean(text);
    WEAK_PHRASES.iter().find(|(p, _)| t == *p).map(|(_, k)| *k)
}

// ─── End-of-day phrases ("that's it for today") ──────────────────────

/// Whole-utterance normaliser for end-of-day phrases: lowercase, straight apostrophes, no punctuation,
/// no leading "nexus / hey nexus / okay / so / well / alright", no trailing "nexus / sir / now".
/// (The strict `clean` also cuts "for today" and "tonight", which would erase the meaning here.)
fn casual_norm(text: &str) -> String {
    let mut t = text
        .to_lowercase()
        .replace('\u{2019}', "'")
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    loop {
        let before = t.clone();
        for p in ["hey nexus ", "nexus ", "okay ", "ok ", "so ", "well ", "alright ", "right ", "yeah ", "um ", "uh "] {
            if let Some(r) = t.strip_prefix(p) {
                t = r.to_string();
            }
        }
        for sfx in [" nexus", " sir", " now"] {
            if let Some(r) = t.strip_suffix(sfx) {
                t = r.to_string();
            }
        }
        if t == before {
            return t;
        }
    }
}

const DAY_ENDS: &[&str] = &["for today", "for the day", "for tonight", "for the night", "for this evening"];

/// "that's it for today", "i'm done for the night", "calling it a day", "i'm going to bed" ...
/// Said in passing, so NEXUS only ASKS (and a yes teaches the exact phrase). Whole utterance only: a
/// sentence that merely contains one of these words ("that's it for today's agenda") never matches.
fn is_end_of_day(norm: &str) -> bool {
    // "that's it / that's all / that will be all / nothing more / nothing else" + a day end (never bare)
    for lead in [
        "that's it", "that is it", "thats it", "that's all", "that is all", "thats all", "that'll be all",
        "that will be all", "nothing more", "nothing else", "no more",
    ] {
        if let Some(rest) = norm.strip_prefix(lead) {
            if DAY_ENDS.contains(&rest.trim()) {
                return true;
            }
        }
    }
    // "i'm done / finished / through / off / out / good / up / wrapping up / signing off" + a day end
    for subj in ["i'm", "i am", "we're", "we are", "im", "i'll be", "i will be"] {
        if let Some(rest) = norm.strip_prefix(subj) {
            let rest = rest.trim();
            for state in [
                "done", "finished", "through", "off", "out", "good", "up", "all done", "all set", "wrapping up",
                "signing off", "logging off", "clocking out", "shutting down", "heading out", "heading off",
            ] {
                if let Some(tail) = rest.strip_prefix(state) {
                    if DAY_ENDS.contains(&tail.trim()) {
                        return true;
                    }
                }
            }
        }
    }
    // idioms and bedtime
    const IDIOMS: &[&str] = &[
        "i'm calling it a night", "i'm calling it a day", "i am calling it a night", "i am calling it a day",
        "calling it a night", "calling it a day", "let's call it a night", "let's call it a day", "lets call it a night",
        "lets call it a day", "i'll call it a night", "i'll call it a day", "call it a night", "call it a day",
        "i'm going to bed", "i am going to bed", "i'm going to sleep", "i am going to sleep", "i'm off to bed",
        "i'm off to sleep", "i'm heading to bed", "i'm heading off to bed", "i'm hitting the sack", "i'm turning in",
        "i'm turning in for the night", "time for bed", "it's time for bed", "it's bedtime", "its bedtime", "bedtime",
        "i'm done working", "i'm done with work", "i'm done with work for today", "i'm done with work for the day",
        "i'm all done", "that's a wrap", "that's a wrap for today", "let's wrap up", "let's wrap it up",
        "let's wrap up for today", "let's wrap it up for today", "time to wrap up", "wrap it up",
        "that's enough for today", "that's enough for tonight", "enough for today", "enough for tonight",
    ];
    IDIOMS.contains(&norm)
}

/// End-of-day phrase -> the shutdown question (never obeyed silently).
pub fn parse_casual(text: &str) -> Option<PowerKind> {
    let n = casual_norm(text);
    is_end_of_day(&n).then_some(PowerKind::Shutdown)
}

/// The normalised phrase to store when a casual phrase is taught.
pub fn casual_key(text: &str) -> Option<String> {
    let n = casual_norm(text);
    is_end_of_day(&n).then_some(n)
}

/// What NEXUS asks after an end-of-day phrase.
pub const CASUAL_QUESTION: &str = "Calling it a day, sir? Shall I shut down your laptop?";

// ─── Loose match (STT mishearings) + learned phrases ─────────────────

/// Words that make "<verb> down/on/off my laptop" something other than a shutdown.
const BENIGN_VERBS: &[&str] = &[
    "turn", "switch", "power", "boot", "start", "wake", "plug", "charge", "clean", "slow", "calm", "break", "bring",
    "back", "update", "write", "track", "wipe", "dust", "open", "close", "lock", "sleep", "restart", "reboot", "log",
    "sign", "hang", "hold", "keep", "leave", "look", "carry", "pick", "take", "get", "put", "tidy", "shut",
    "show", "cut", "kick", "clear", "knock", "hand", "tear",
];
/// A sentence that starts with one of these is a question, not a command.
const QUESTION_WORDS: &[&str] = &[
    "how", "what", "why", "can", "do", "does", "did", "is", "are", "where", "who", "when", "which", "will", "would",
    "could", "should", "am", "tell", "explain",
];

/// Whisper keeps mishearing "shut down my laptop" ("send on", "set down", "check down", "tick on", ...).
/// A fixed list always misses the next variant, so this is structural: a short sentence of the form
/// `<one or two words> <down|off> <my laptop>` whose first words are not an ordinary verb. ("on" is NOT
/// used: "music on my laptop" is an ordinary phrase; the two logged "on" mishearings are in the strict list.)
/// Weak evidence: the caller must ASK before powering off, never obey silently and never ask the cloud.
pub fn parse_loose(text: &str) -> Option<PowerKind> {
    let t = clean(&text.replace(',', " "));
    let t = t.strip_prefix("to ").unwrap_or(&t);
    let tokens: Vec<&str> = t.split_whitespace().collect();
    if tokens.len() < 3 || tokens.len() > 5 {
        return None;
    }
    if QUESTION_WORDS.contains(&tokens[0]) {
        return None;
    }
    let particle = tokens.iter().position(|w| matches!(*w, "down" | "off"))?;
    if particle == 0 || particle > 2 {
        return None;
    }
    // A misheard verb is a plain word; "what's"/"it's" style contractions are never one.
    if !tokens[..particle].iter().all(|w| w.chars().all(|c| c.is_ascii_alphabetic())) {
        return None;
    }
    if tokens[..particle].iter().any(|w| BENIGN_VERBS.contains(w)) {
        return None;
    }
    let rest = tokens[particle + 1..].join(" ");
    is_computer(&rest, false).then_some(PowerKind::Shutdown)
}

/// The mishearing the user was just asked about (so a "yes" can teach it). Expires with the question.
static PENDING_LOOSE: std::sync::Mutex<Option<(String, std::time::Instant)>> = std::sync::Mutex::new(None);
const PENDING_LOOSE_TTL: std::time::Duration = std::time::Duration::from_secs(45);
const MAX_ALIASES: usize = 50;

fn alias_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("power_aliases.json")
}

/// Normalised form used to store and compare a heard phrase.
pub fn alias_key(text: &str) -> String {
    clean(&text.replace(',', " ")).split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn load_aliases(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(alias_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

/// Has the user confirmed this exact (normalised) phrase as "shut down" before?
pub fn is_learned_shutdown(dir: &std::path::Path, text: &str) -> bool {
    let key = learn_key(text);
    !key.is_empty() && load_aliases(dir).iter().any(|a| *a == key)
}

/// Key under which a phrase is taught: casual end-of-day phrases get their own prefixed key
/// (`alias_key` would cut "for today" off them and teach a far too broad "that's it").
fn learn_key(text: &str) -> String {
    match casual_key(text) {
        Some(k) => format!("casual: {k}"),
        None => alias_key(text),
    }
}

/// Remember a confirmed mishearing (local only, capped, newest kept). Returns whether it was stored.
pub fn add_alias(dir: &std::path::Path, text: &str) -> bool {
    let key = learn_key(text);
    // Only things the loose, bare-verb or end-of-day rule would have caught, and only for a SHUTDOWN, are worth
    // storing (never an arbitrary sentence, never "restart" taught as a shutdown).
    let known = parse_casual(text).is_some() || parse_loose(&key).is_some() || parse_weak(&key) == Some(PowerKind::Shutdown);
    if key.is_empty() || !known {
        return false;
    }
    let mut all = load_aliases(dir);
    if all.contains(&key) {
        return true;
    }
    all.push(key);
    if all.len() > MAX_ALIASES {
        let drop = all.len() - MAX_ALIASES;
        all.drain(..drop);
    }
    let Ok(json) = serde_json::to_string_pretty(&all) else { return false };
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join("power_aliases.json.tmp");
    std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, alias_path(dir)).is_ok()
}

/// Remember which heard phrase the "Shut down your laptop?" question is about.
pub fn note_loose_question(text: &str) {
    if let Ok(mut g) = PENDING_LOOSE.lock() {
        *g = Some((text.to_string(), std::time::Instant::now() + PENDING_LOOSE_TTL));
    }
}

/// The user did not say yes (or a different question began): the remembered phrase must not be taught later.
pub fn forget_pending_alias() {
    if let Ok(mut g) = PENDING_LOOSE.lock() {
        *g = None;
    }
}

/// The user said yes: teach the phrase that triggered the question (if it is still fresh).
pub fn learn_pending_alias(dir: &std::path::Path) -> bool {
    let taken = PENDING_LOOSE.lock().ok().and_then(|mut g| g.take());
    match taken {
        Some((phrase, until)) if until > std::time::Instant::now() => add_alias(dir, &phrase),
        _ => false,
    }
}

// ─── Countdown / execution ───────────────────────────────────────────

/// Bumped on every new countdown and on cancel; a countdown only fires if its
/// generation is still current.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACTIVE: AtomicU64 = AtomicU64::new(0);

/// True while a power command is counting down and can still be cancelled.
pub fn countdown_active() -> bool {
    ACTIVE.load(Ordering::SeqCst) != 0
}

/// Abort a pending power command. Returns whether one was pending.
pub fn cancel() -> bool {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    ACTIVE.swap(0, Ordering::SeqCst) != 0
}

/// Whether a reply means "stop the power command".
pub fn is_cancel_phrase(text: &str) -> bool {
    let t = text.trim().to_lowercase().replace(',', "");
    let t = t.trim_matches(|c: char| matches!(c, '.' | '!' | '?' | ' '));
    let t = t.strip_prefix("nexus ").unwrap_or(t);
    matches!(
        t,
        "cancel"
            | "stop"
            | "abort"
            | "wait"
            | "no"
            | "don't"
            | "do not"
            | "never mind"
            | "cancel that"
            | "stop that"
            | "cancel shutdown"
            | "cancel the shutdown"
            | "don't shut down"
            | "do not shut down"
            | "stop shutdown"
            | "cancel restart"
            | "abort shutdown"
    )
}

/// Start a power sequence that runs its own steps (say goodbye, close apps, ...) and needs the same
/// cancel flag as the countdown. Returns its generation; `cancel()` (or a newer command) invalidates it.
pub fn begin() -> u64 {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    ACTIVE.store(generation, Ordering::SeqCst);
    generation
}

/// Is this still the live power command (not cancelled, not superseded)?
pub fn is_current(generation: u64) -> bool {
    GENERATION.load(Ordering::SeqCst) == generation
}

/// The sequence reached its end (or gave up): it is no longer cancellable.
pub fn finish(generation: u64) {
    if is_current(generation) {
        ACTIVE.store(0, Ordering::SeqCst);
    }
}

/// The last window in which "cancel" still stops a shutdown after the apps have been closed.
pub const FINAL_CANCEL_WINDOW_SECS: u64 = 3;

/// Call Windows now (no countdown). Used at the end of a sequence that already waited and checked.
pub fn execute_now(kind: PowerKind) -> Result<(), String> {
    println!("[POWER] executing {}", kind.as_str());
    execute(kind)
}

/// What NEXUS says as the shutdown starts: "Good night" late, a plain goodbye otherwise
/// (both are pre-rendered fixed lines). `noted` = a session snapshot was saved for tomorrow's offer.
pub fn goodbye_line(hour: u32, name: Option<&str>, friend: bool, noted: bool) -> String {
    let addr = crate::persona::greet_address(name, friend);
    let base = match hour {
        20..=23 | 0..=4 => format!("Good night{addr}."),
        _ => format!("Shutting down{addr}. See you soon."),
    };
    if noted {
        format!("I've noted what's open. {base}")
    } else {
        base
    }
}

/// Start the countdown, then call Windows. Returns immediately.
pub fn schedule(kind: PowerKind) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    ACTIVE.store(generation, Ordering::SeqCst);
    let secs = kind.countdown_secs();
    std::thread::Builder::new()
        .name("power-countdown".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            if GENERATION.load(Ordering::SeqCst) != generation {
                println!("[POWER] {} cancelled", kind.as_str());
                return;
            }
            ACTIVE.store(0, Ordering::SeqCst);
            println!("[POWER] executing {}", kind.as_str());
            if let Err(e) = execute(kind) {
                tracing::error!("power command {} failed: {e}", kind.as_str());
            }
        })
        .ok();
}

#[cfg(target_os = "windows")]
fn execute(kind: PowerKind) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let run = |prog: &str, args: &[&str]| -> Result<(), String> {
        std::process::Command::new(prog)
            .args(args)
            .creation_flags(NO_WINDOW)
            .status()
            .map_err(|e| e.to_string())
            .and_then(|s| if s.success() { Ok(()) } else { Err(format!("{prog} exited with {s}")) })
    };
    match kind {
        // No /f: apps with unsaved work get to ask instead of being killed.
        PowerKind::Shutdown => run("shutdown", &["/s", "/t", "0"]),
        PowerKind::Restart => run("shutdown", &["/r", "/t", "0"]),
        PowerKind::SignOut => run("shutdown", &["/l"]),
        PowerKind::Lock => run("rundll32.exe", &["user32.dll,LockWorkStation"]),
        PowerKind::Sleep => {
            use windows::Win32::Foundation::BOOLEAN;
            use windows::Win32::System::Power::SetSuspendState;
            // SAFETY: plain Win32 call with no pointers.
            let ok = unsafe { SetSuspendState(BOOLEAN(0), BOOLEAN(0), BOOLEAN(0)) };
            if ok.0 != 0 { Ok(()) } else { Err("SetSuspendState failed".into()) }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn execute(_kind: PowerKind) -> Result<(), String> {
    Err("power commands are only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_phrases() {
        for t in [
            "shut down my laptop",
            "Shut down the laptop.",
            "nexus shut down my laptop",
            "please shut down my computer",
            "shutdown my pc",
            "power off my laptop",
            "power down the system",
            "turn off my laptop",
            "switch off the computer",
            "can you shut down my laptop now",
            "shut down windows",
            "shut down the laptop please",
        ] {
            assert_eq!(parse(t), Some(PowerKind::Shutdown), "{t}");
        }
    }

    #[test]
    fn other_power_kinds() {
        assert_eq!(parse("restart my laptop"), Some(PowerKind::Restart));
        assert_eq!(parse("reboot the computer"), Some(PowerKind::Restart));
        assert_eq!(parse("put my laptop to sleep"), Some(PowerKind::Sleep));
        assert_eq!(parse("sleep my laptop"), Some(PowerKind::Sleep));
        assert_eq!(parse("lock my laptop"), Some(PowerKind::Lock));
        assert_eq!(parse("lock the screen"), Some(PowerKind::Lock));
        assert_eq!(parse("sign out of windows"), Some(PowerKind::SignOut));
        assert_eq!(parse("log off my computer"), Some(PowerKind::SignOut));
    }

    #[test]
    fn app_and_unrelated_commands_are_not_power_commands() {
        for t in [
            "shut down notepad",
            "shut down nexus",
            "shut down the music",
            "close notepad",
            "turn off the lights",
            "turn off bluetooth",
            "restart nexus",
            "restart the download",
            "lock the door",
            "sleep",
            "shut down",
            "reboot",
            "log out",
            "open my laptop settings",
            "locked my laptop yesterday",
            "",
        ] {
            assert_eq!(parse(t), None, "{t:?}");
        }
        // The screen is only a lock target.
        assert_eq!(parse("shut down the screen"), None);
    }

    #[test]
    fn only_session_ending_commands_ask_and_snapshot() {
        assert!(PowerKind::Shutdown.ends_session() && PowerKind::Restart.ends_session() && PowerKind::SignOut.ends_session());
        assert!(!PowerKind::Sleep.ends_session() && !PowerKind::Lock.ends_session());
        assert!(PowerKind::Shutdown.needs_snapshot() && !PowerKind::Lock.needs_snapshot());
    }

    #[test]
    fn cancel_phrases_are_strict() {
        for t in ["cancel", "stop", "Nexus, stop.", "don't shut down", "never mind", "abort shutdown"] {
            assert!(is_cancel_phrase(t), "{t}");
        }
        for t in ["open chrome", "yes", "cancel my meeting tomorrow", "stop the music please"] {
            assert!(!is_cancel_phrase(t), "{t}");
        }
    }

    #[test]
    fn a_countdown_can_be_cancelled_and_a_new_one_supersedes_it() {
        // Uses a kind whose execution is never reached: cancel before the delay elapses.
        let _ = cancel();
        assert!(!countdown_active());
        // Simulate the bookkeeping without spawning a real power call.
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        ACTIVE.store(generation, Ordering::SeqCst);
        assert!(countdown_active());
        assert!(cancel());
        assert!(!countdown_active());
        assert!(GENERATION.load(Ordering::SeqCst) != generation, "a cancelled countdown must not fire");
        assert!(!cancel(), "nothing left to cancel");
    }

    #[test]
    fn whisper_mishearings_of_shut_down_still_shut_down_but_only_for_the_computer() {
        for t in ["send on my laptop.", "set down my laptop.", "to set down my laptop.", "shot down the computer", "Nexus set down my pc"] {
            assert_eq!(parse(t), Some(PowerKind::Shutdown), "{t}");
        }
        for t in ["set down my coffee", "send on my email", "set down the phone", "send on the file to my laptop", "set down"] {
            assert_eq!(parse(t), None, "{t}");
        }
    }

    #[test]
    fn goodbye_is_good_night_late_and_a_plain_goodbye_by_day() {
        assert_eq!(goodbye_line(23, Some("Lakshya"), false, false), "Good night, sir.");
        assert_eq!(goodbye_line(2, None, false, false), "Good night, sir.");
        assert_eq!(goodbye_line(14, None, false, false), "Shutting down, sir. See you soon.");
        assert_eq!(goodbye_line(21, Some("Lakshya"), true, false), "Good night, Lakshya.");
        assert_eq!(goodbye_line(21, None, false, true), "I've noted what's open. Good night, sir.");
    }

    #[test]
    fn kind_round_trips() {
        for k in [PowerKind::Shutdown, PowerKind::Restart, PowerKind::Sleep, PowerKind::Lock, PowerKind::SignOut] {
            assert_eq!(PowerKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(PowerKind::from_str("nope"), None);
    }
}


#[cfg(test)]
mod loose_tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nexus_power_alias_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Every real mishearing of "shut down my laptop" seen in missed_intents.jsonl.
    const REAL_MISHEARINGS: &[&str] = &[
        "send on my laptop.",
        "set down my laptop.",
        "to set down my laptop.",
        "check down my laptop.",
        "tick on my laptop.",
        "send down my laptop.",
    ];

    #[test]
    fn the_real_mishearings_are_shutdowns() {
        for t in REAL_MISHEARINGS {
            assert_eq!(parse(t).or_else(|| parse_loose(t)), Some(PowerKind::Shutdown), "{t}");
        }
        // the ones with "down"/"off" are also caught structurally (no list needed)
        for t in ["set down my laptop.", "to set down my laptop.", "check down my laptop.", "send down my laptop."] {
            assert_eq!(parse_loose(t), Some(PowerKind::Shutdown), "{t}");
        }
        // a variant nobody has logged yet still matches (the point of a structural rule)
        assert_eq!(parse_loose("Nexus, shock down my computer"), Some(PowerKind::Shutdown));
    }

    #[test]
    fn ordinary_sentences_never_match() {
        for t in [
            "turn on my laptop",
            "slow down my laptop",
            "calm down my computer",
            "bring down my laptop",
            "charge my laptop",
            "how do i shut down my laptop",
            "what is a laptop",
            "can you shut down my laptop",
            "open my laptop",
            "close my laptop",
            "restart my laptop",
            "lock my laptop",
            "put my laptop down",
            "write down my laptop",
            "wake up my computer",
            "down my laptop",
            "shut down notepad",
            "set down the phone",
            "send on the file to my laptop",
            "my laptop is slow",
            "set down my coffee",
            "send on my email",
            // ordinary "<noun> on/off my laptop" phrases
            "music on my laptop",
            "files on my laptop",
            "play music on my laptop",
            "what's on my laptop",
            "show off my laptop",
            "get off my laptop",
            // only the question-word / contraction guards stop these
            "what down my laptop",
            "is down my computer",
            "what's down my laptop",
            // particle too far from the start: a longer sentence, not a misheard two-word verb
            "pour hot water down laptop",
            "",
        ] {
            assert_eq!(parse_loose(t), None, "{t:?} must not be a loose shutdown");
        }
    }

    #[test]
    fn logged_mishearings_are_obeyed_but_unseen_variants_are_only_loose() {
        assert_eq!(parse("shut down my laptop"), Some(PowerKind::Shutdown));
        // all six the owner actually produced are strict (obeyed at once)
        for t in REAL_MISHEARINGS {
            assert_eq!(parse(t), Some(PowerKind::Shutdown), "{t}");
        }
        // a variant nobody has logged yet is NOT a strict match: it is asked about first
        assert_eq!(parse("shock down my laptop."), None);
        assert_eq!(parse_loose("shock down my laptop."), Some(PowerKind::Shutdown));
    }

    #[test]
    fn a_yes_teaches_the_phrase_and_nothing_else() {
        let d = tmp("learn");
        assert!(!is_learned_shutdown(&d, "shock down my laptop."));
        note_loose_question("shock down my laptop.");
        assert!(learn_pending_alias(&d));
        assert!(is_learned_shutdown(&d, "Shock down my laptop"), "case/punctuation-insensitive");
        // consumed: a second yes (e.g. the strict 'shut down anyway?' flow) teaches nothing
        assert!(!learn_pending_alias(&d));
        // an arbitrary sentence can never be stored
        assert!(!add_alias(&d, "open the pod bay doors"));
        assert!(!is_learned_shutdown(&d, "open the pod bay doors"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn aliases_are_capped_newest_kept() {
        let d = tmp("cap");
        // distinct all-letter "verbs" (digits are not a plain word): zza, zzb, ...
        let word = |i: usize| format!("zz{}{}", (b'a' + (i / 26) as u8) as char, (b'a' + (i % 26) as u8) as char);
        for i in 0..(MAX_ALIASES + 10) {
            assert!(add_alias(&d, &format!("{} down my laptop", word(i))));
        }
        let all = load_aliases(&d);
        assert_eq!(all.len(), MAX_ALIASES);
        assert!(all.last().unwrap().starts_with(&word(MAX_ALIASES + 9)));
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod phrase_coverage_tests {
    use super::*;

    #[test]
    fn every_explicit_way_to_say_shut_down_the_laptop() {
        let verbs = ["shut down", "shutdown", "power off", "power down", "turn off", "switch off", "shut off", "close down"];
        let targets = ["my laptop", "the laptop", "my computer", "the computer", "my pc", "the pc", "the system", "windows", "my machine", "this device"];
        let mut n = 0;
        for v in verbs {
            for t in targets {
                let phrase = format!("{v} {t}");
                assert_eq!(parse(&phrase), Some(PowerKind::Shutdown), "{phrase}");
                n += 1;
            }
        }
        // polite wrappers
        for wrap in [
            "nexus {}", "hey nexus {}", "please {}", "can you {}", "could you please {}", "would you {}", "will you {}",
            "go ahead and {}", "i want you to {}", "i need you to {}", "i'd like you to {}", "i would like you to {}",
            "i want to {}", "let's {}", "just {}", "okay nexus {}", "ok {}", "alright {}", "{} now", "{} right now",
            "{} please", "{} for me", "{} thanks", "{} thank you", "{} tonight", "{} for the night", "{} okay",
            "{} nexus", "{} sir", "nexus, {}.", "Nexus, please {} now.",
        ] {
            let phrase = wrap.replace("{}", "shut down my laptop");
            assert_eq!(parse(&phrase), Some(PowerKind::Shutdown), "{phrase}");
            n += 1;
        }
        // object between verb and particle
        for p in [
            "turn the laptop off", "turn my laptop off", "turn my computer off", "switch my pc off", "switch the laptop off",
            "switch the computer offline", "power my laptop down", "power the pc off", "shut my laptop down", "shut the computer down",
            "shut my laptop off", "close the system down", "take the system down", "turn off the laptop", "please turn my laptop off now",
        ] {
            assert_eq!(parse(p), Some(PowerKind::Shutdown), "{p}");
            n += 1;
        }
        assert!(n >= 120, "corpus too small: {n}");
    }

    #[test]
    fn restart_sleep_lock_and_sign_out_variants() {
        for p in [
            "restart my laptop", "reboot my computer", "restart windows", "reboot the pc", "give the laptop a restart",
            "give my computer a reboot", "could you restart the system", "please reboot my laptop now",
        ] {
            assert_eq!(parse(p), Some(PowerKind::Restart), "{p}");
        }
        for p in [
            "put my laptop to sleep", "put the computer to sleep", "send the pc to sleep", "suspend my laptop", "put the laptop on standby",
            "put my pc in sleep mode", "hibernate the laptop", "sleep my laptop",
        ] {
            assert_eq!(parse(p), Some(PowerKind::Sleep), "{p}");
        }
        for p in ["lock my laptop", "lock the screen", "lock windows", "lock my pc", "please lock the computer"] {
            assert_eq!(parse(p), Some(PowerKind::Lock), "{p}");
        }
        for p in ["sign out of windows", "sign me out of windows", "log me out of my computer", "log off my computer", "sign out of my pc"] {
            assert_eq!(parse(p), Some(PowerKind::SignOut), "{p}");
        }
    }

    #[test]
    fn bare_verbs_are_only_asked_about_never_obeyed() {
        for p in [
            "shut down", "Shut down.", "Nexus, shut down", "please shut down", "shutdown", "shut it down", "shut it off",
            "power off", "power down", "power it off", "turn off", "turn it off", "switch off", "switch it off", "shut everything down",
            "can you shut down", "okay shut it down",
        ] {
            assert_eq!(parse(p), None, "{p}: must not be obeyed silently");
            assert_eq!(parse_weak(p), Some(PowerKind::Shutdown), "{p}");
        }
        assert_eq!(parse_weak("restart"), Some(PowerKind::Restart));
        assert_eq!(parse_weak("reboot it"), Some(PowerKind::Restart));
        assert_eq!(parse_weak("suspend"), Some(PowerKind::Sleep));
        assert_eq!(parse_weak("lock it"), Some(PowerKind::Lock));
        assert_eq!(parse_weak("sign me out"), Some(PowerKind::SignOut));
        assert_eq!(parse_weak("log me out"), Some(PowerKind::SignOut));
        // only the WHOLE utterance counts
        for p in ["shut down notepad", "shut down the music", "turn off the lights", "restart the download", "lock the door", "how do i shut down", "shut down and then open chrome", "restart nexus", ""] {
            assert_eq!(parse_weak(p), None, "{p:?}");
        }
    }

    #[test]
    fn things_that_are_not_about_the_computer_stay_out() {
        for p in [
            "shut down notepad", "shut down chrome", "shut down the music", "shut down the server", "shut down nexus",
            "turn off the lights", "turn off the tv", "turn off the music", "turn off my phone", "turn off bluetooth",
            "turn off the screen", "turn off notifications", "switch off the fan", "power off the printer", "power off my phone",
            "turn the volume off", "turn the music down", "turn my phone off", "shut the door down", "shut the window",
            "close down notepad", "close the laptop", "take the laptop to work", "take the trash down", "give me a restart",
            "give the dog a restart", "restart the song", "restart the game", "restart nexus", "reboot the router",
            "lock the door", "lock my phone", "lock the car", "sign out of gmail", "sign out of spotify", "log out of github",
            "how do i shut down my laptop", "what does shut down my laptop do", "why did my laptop shut down", "my laptop shut down by itself",
            "i shut down my laptop yesterday", "did you shut down my laptop", "shut down my laptop and open chrome",
            "open my laptop settings", "put the laptop on the table", "put it to sleep", "good night", "that's it for today",
        ] {
            assert_eq!(parse(p), None, "{p:?}");
        }
    }

    #[test]
    fn learning_a_bare_verb_applies_to_shutdown_only() {
        let d = std::env::temp_dir().join(format!("nexus_power_weak_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert!(add_alias(&d, "Shut down."));
        assert!(is_learned_shutdown(&d, "shut down"));
        assert!(!add_alias(&d, "restart"), "restart must never be stored as a shutdown phrase");
        assert!(!is_learned_shutdown(&d, "restart"));
        forget_pending_alias();
        note_loose_question("turn it off");
        forget_pending_alias();
        assert!(!learn_pending_alias(&d), "a forgotten question teaches nothing");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn each_kind_has_its_own_question() {
        for k in [PowerKind::Shutdown, PowerKind::Restart, PowerKind::Sleep, PowerKind::Lock, PowerKind::SignOut] {
            assert!(k.confirm_question().starts_with("Did you want me to "), "{k:?}");
        }
        assert!(PowerKind::Restart.confirm_question().contains("restart"));
        assert!(PowerKind::Lock.confirm_question().contains("lock"));
    }
}

#[cfg(test)]
mod end_of_day_tests {
    use super::*;

    #[test]
    fn end_of_day_phrases_are_asked_about() {
        let mut n = 0;
        for day in DAY_ENDS {
            for lead in [
                "that's it", "that is it", "thats it", "that's all", "that'll be all", "that will be all",
                "nothing else", "nothing more",
            ] {
                let p = format!("{lead} {day}");
                assert_eq!(parse_casual(&p), Some(PowerKind::Shutdown), "{p}");
                assert_eq!(parse(&p), None, "{p}: never an explicit command");
                n += 1;
            }
            for subj in ["i'm", "i am", "we're"] {
                for state in ["done", "finished", "through", "off", "out", "good", "up", "all set", "wrapping up", "signing off"] {
                    let p = format!("{subj} {state} {day}");
                    assert_eq!(parse_casual(&p), Some(PowerKind::Shutdown), "{p}");
                    n += 1;
                }
            }
        }
        for p in [
            "calling it a night", "i'm calling it a day", "let's call it a day", "i'm going to bed", "time for bed",
            "it's bedtime", "i'm turning in", "i'm hitting the sack", "that's a wrap", "let's wrap up", "enough for today",
            "i'm done working", "i'm up for tonight", "I\u{2019}m done for the night",
        ] {
            assert_eq!(parse_casual(p), Some(PowerKind::Shutdown), "{p}");
            n += 1;
        }
        for wrap in ["nexus {}", "Nexus, {}.", "hey nexus, {}", "okay {}", "so {}", "{} nexus", "{}, sir.", "alright nexus {}"] {
            let p = wrap.replace("{}", "that's it for today");
            assert_eq!(parse_casual(&p), Some(PowerKind::Shutdown), "{p}");
            n += 1;
        }
        assert!(n >= 150, "corpus too small: {n}");
    }

    #[test]
    fn sentences_that_merely_contain_those_words_are_left_alone() {
        for p in [
            "that's it", "that's all", "thats all", "i'm done", "i'm done with this file", "i'm done for now",
            "that's it for today's agenda", "that's it for today but open chrome", "open notepad that's it for today",
            "i'm going to bed soon remind me at ten", "is that it for today", "what's on for today", "that's it for the video",
            "good night", "goodnight nexus", "time for bed reminder", "i'm done for today open spotify",
            "call it a day and play music", "remind me to wrap up", "wrap up the report", "that's it for the pull request",
            "i'm off to the gym", "i'm out of coffee", "we're done with the call", "",
        ] {
            assert_eq!(parse_casual(p), None, "{p:?}");
        }
    }

    #[test]
    fn a_taught_end_of_day_phrase_uses_its_own_key_and_stays_exact() {
        let d = std::env::temp_dir().join(format!("nexus_power_casual_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        assert!(add_alias(&d, "Nexus, that's it for today."));
        assert!(is_learned_shutdown(&d, "that's it for today"));
        assert!(is_learned_shutdown(&d, "okay nexus that's it for today"));
        // the broad stem was NOT taught, and neither was a different day end
        assert!(!is_learned_shutdown(&d, "that's it"));
        assert!(!is_learned_shutdown(&d, "that's it for the video"));
        assert!(!is_learned_shutdown(&d, "that's it for tonight"));
        assert!(!add_alias(&d, "that's it for the video"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
