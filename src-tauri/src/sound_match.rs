//! Sound-based word matching: find the name a mis-heard phrase most likely meant, by how it SOUNDS.
//!
//! Speech recognisers confuse sound-alikes ("note pad" / "notepad", "open brief" / "open brave", "spot if I" / "spotify"). Spelling-based fuzzy
//! matching misses these because the letters differ a lot while the sounds barely do. This module turns both the heard words and each known
//! name into phoneme sequences and compares them with a phoneme edit distance in which sound-alike swaps (p/b, t/d, vowel/vowel…) are cheap.
//!
//! It only SUGGESTS: callers treat a hit as a guess and ask ("did you mean Notepad?"), exactly like the existing fuzzy tier.
//!
//! Memory: the pronunciation table is an `include_str!` blob inside the executable (paged in on use, never copied to the heap); the only
//! heap structure is a line-offset index of about 0.5 MB, built on first use. It deliberately does NOT use the offline voice's G2P, whose
//! dictionary measured about +125 MB when loaded.
//!
//! Data: the CMU Pronouncing Dictionary, Copyright (C) 1993-2015 Carnegie Mellon University, BSD-style licence (see THIRD_PARTY_NOTICES.md),
//! compacted by `scripts/gen_pron_lexicon.py`.

use std::sync::OnceLock;

const DATA: &str = include_str!("sound_match_cmudict.txt");

/// Phoneme order shared with `scripts/gen_pron_lexicon.py`; a phoneme's code character is `'0' + index`.
pub const PHONEMES: [&str; 39] = [
    "AA", "AE", "AH", "AO", "AW", "AY", "B", "CH", "D", "DH", "EH", "ER", "EY", "F", "G", "HH", "IH", "IY", "JH", "K", "L", "M", "N", "NG", "OW", "OY",
    "P", "R", "S", "SH", "T", "TH", "UH", "UW", "V", "W", "Y", "Z", "ZH",
];

/// Names the dictionary lacks or spells oddly (brands and NEXUS vocabulary), as ARPAbet.
const HAND: &[(&str, &str)] = &[
    ("nexus", "N EH K S AH S"), ("spotify", "S P AA T AH F AY"), ("whatsapp", "W AA T S AE P"), ("antigravity", "AE N T IY G R AE V AH T IY"),
    ("github", "G IH T HH AH B"), ("vscode", "V IY EH S K OW D"), ("vs", "V IY EH S"), ("notepad", "N OW T P AE D"), ("chrome", "K R OW M"),
    ("discord", "D IH S K AO R D"), ("youtube", "Y UW T UW B"), ("opencode", "OW P AH N K OW D"), ("copilot", "K OW P AY L AH T"),
    ("cursor", "K ER S ER"), ("codex", "K OW D EH K S"), ("gemini", "JH EH M AH N AY"), ("windsurf", "W IH N D S ER F"), ("kilo", "K IY L OW"), ("kiro", "K IY R OW"),
    ("aider", "EY D ER"), ("telegram", "T EH L AH G R AE M"), ("brave", "B R EY V"), ("laptop", "L AE P T AA P"), ("shutdown", "SH AH T D AW N"),
    ("onenote", "W AH N N OW T"), ("powerpoint", "P AW ER P OY N T"), ("outlook", "AW T L UH K"), ("zoom", "Z UW M"), ("steam", "S T IY M"),
    ("figma", "F IH G M AH"), ("postman", "P OW S T M AH N"), ("vlc", "V IY EH L S IY"),
];

fn phoneme_id(name: &str) -> Option<u8> {
    PHONEMES.iter().position(|p| *p == name).map(|i| i as u8)
}

fn is_vowel(p: u8) -> bool {
    matches!(p, 0 | 1 | 2 | 3 | 4 | 5 | 10 | 11 | 12 | 16 | 17 | 24 | 25 | 32 | 33)
}

/// Groups of consonants that are easily confused with each other (indices into `PHONEMES`).
const GROUPS: &[&[u8]] = &[&[26, 6], &[30, 8], &[19, 14], &[13, 34], &[28, 37, 29, 38], &[7, 18], &[21, 22, 23], &[20, 27], &[31, 9], &[35, 34]];

const INDEL: f32 = 0.8;

fn sub_cost(a: u8, b: u8) -> f32 {
    if a == b {
        return 0.0;
    }
    match (is_vowel(a), is_vowel(b)) {
        (true, true) => 0.35,
        (false, false) if GROUPS.iter().any(|g| g.contains(&a) && g.contains(&b)) => 0.4,
        _ => 1.0,
    }
}

/// Phoneme edit distance (sound-alike substitutions are cheap).
pub fn phoneme_distance(x: &[u8], y: &[u8]) -> f32 {
    let (n, m) = (x.len(), y.len());
    let mut prev: Vec<f32> = (0..=m).map(|j| j as f32 * INDEL).collect();
    for i in 1..=n {
        let mut cur = vec![0.0f32; m + 1];
        cur[0] = i as f32 * INDEL;
        for j in 1..=m {
            cur[j] = (prev[j] + INDEL).min(cur[j - 1] + INDEL).min(prev[j - 1] + sub_cost(x[i - 1], y[j - 1]));
        }
        prev = cur;
    }
    prev[m]
}

// ---------- pronunciation lookup ----------

fn offsets() -> &'static Vec<u32> {
    static IDX: OnceLock<Vec<u32>> = OnceLock::new();
    IDX.get_or_init(|| {
        let mut v = Vec::with_capacity(130_000);
        let mut pos = 0usize;
        for line in DATA.split_inclusive('\n') {
            v.push(pos as u32);
            pos += line.len();
        }
        v.shrink_to_fit();
        v
    })
}

/// Heap bytes held by the pronunciation index (the table itself lives in the executable image).
#[cfg(test)]
pub fn index_heap_bytes() -> usize {
    offsets().len() * std::mem::size_of::<u32>()
}

fn line_at(i: usize) -> &'static str {
    let off = offsets();
    let start = off[i] as usize;
    let end = off.get(i + 1).map(|e| *e as usize).unwrap_or(DATA.len());
    DATA[start..end].trim_end_matches('\n')
}

fn dict_lookup(word: &str) -> Option<Vec<u8>> {
    let n = offsets().len();
    let (mut lo, mut hi) = (0usize, n);
    while lo < hi {
        let mid = (lo + hi) / 2;
        let line = line_at(mid);
        let (w, codes) = line.split_once('\t')?;
        match w.cmp(word) {
            std::cmp::Ordering::Equal => return Some(codes.bytes().map(|b| b - b'0').collect()),
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
        }
    }
    None
}

/// Crude letter-to-sound fallback for words in neither table (rare names, typos).
fn spell_out(word: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for ch in word.chars() {
        let names: &[&str] = match ch {
            'a' => &["AE"], 'b' => &["B"], 'c' => &["K"], 'd' => &["D"], 'e' => &["EH"], 'f' => &["F"], 'g' => &["G"], 'h' => &["HH"],
            'i' => &["IH"], 'j' => &["JH"], 'k' => &["K"], 'l' => &["L"], 'm' => &["M"], 'n' => &["N"], 'o' => &["AA"], 'p' => &["P"],
            'q' => &["K"], 'r' => &["R"], 's' => &["S"], 't' => &["T"], 'u' => &["AH"], 'v' => &["V"], 'w' => &["W"], 'x' => &["K", "S"],
            'y' => &["Y"], 'z' => &["Z"],
            _ => &[],
        };
        out.extend(names.iter().filter_map(|n| phoneme_id(n)));
    }
    out
}

/// Phonemes of one lowercase word: hand table, then dictionary, then letter fallback.
pub fn pronounce_word(word: &str) -> Vec<u8> {
    let w: String = word.chars().filter(|c| c.is_ascii_lowercase() || *c == '\'').collect();
    if w.is_empty() {
        return Vec::new();
    }
    if let Some((_, ph)) = HAND.iter().find(|(h, _)| *h == w) {
        return ph.split_whitespace().filter_map(phoneme_id).collect();
    }
    dict_lookup(&w).unwrap_or_else(|| spell_out(&w))
}

/// Phonemes of a phrase, ignoring word boundaries ("note pad" and "notepad" sound the same).
pub fn phrase_phones(text: &str) -> Vec<u8> {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !(c.is_ascii_lowercase() || c == '\''))
        .filter(|w| !w.is_empty())
        .flat_map(pronounce_word)
        .collect()
}

// ---------- nearest-name search ----------

/// A name to match against. `group` identifies the thing the name belongs to: several names for one app must not count as rivals.
pub struct Entry {
    pub label: String,
    pub group: String,
    phones: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub label: String,
    pub group: String,
    /// 0 = identical sound, about 1 = unrelated; normalised by the longer sequence.
    pub dist: f32,
    /// How much clearer the best match is than the best DIFFERENT thing.
    pub margin: f32,
}

pub const MIN_PHONES: usize = 3;
/// Loosest accepted normalised distance; tuned on the confusion set in the tests.
pub const MAX_DIST: f32 = 0.34;
pub const MIN_MARGIN: f32 = 0.16;

pub struct PhoneticIndex {
    entries: Vec<Entry>,
}

impl PhoneticIndex {
    pub fn new<I: IntoIterator<Item = (String, String)>>(names: I) -> Self {
        let entries = names
            .into_iter()
            .filter_map(|(label, group)| {
                let phones = phrase_phones(&label);
                (phones.len() >= MIN_PHONES).then_some(Entry { label, group, phones })
            })
            .collect();
        Self { entries }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The single clearly-best name for `query`, or None when nothing is close enough or two different things are about equally close.
    pub fn nearest(&self, query: &str) -> Option<Hit> {
        self.nearest_with(query, MAX_DIST, MIN_MARGIN)
    }

    /// [`nearest`](Self::nearest) with explicit thresholds (used to tune them).
    pub fn nearest_with(&self, query: &str, max_dist: f32, min_margin: f32) -> Option<Hit> {
        let qp = phrase_phones(query);
        if qp.len() < MIN_PHONES {
            return None;
        }
        let mut best: Option<(f32, usize)> = None;
        let mut second_other = f32::INFINITY;
        let mut scored: Vec<(f32, usize)> = Vec::with_capacity(self.entries.len());
        for (i, e) in self.entries.iter().enumerate() {
            let (a, b) = (qp.len() as f32, e.phones.len() as f32);
            if a / b > 2.0 || b / a > 2.0 {
                continue;
            }
            let d = phoneme_distance(&qp, &e.phones) / qp.len().max(e.phones.len()) as f32;
            scored.push((d, i));
            if best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
        let (bd, bi) = best?;
        for (d, i) in scored {
            if self.entries[i].group != self.entries[bi].group && d < second_other {
                second_other = d;
            }
        }
        let margin = if second_other.is_finite() { second_other - bd } else { 1.0 };
        (bd <= max_dist && margin >= min_margin).then(|| Hit {
            label: self.entries[bi].label.clone(),
            group: self.entries[bi].group.clone(),
            dist: bd,
            margin,
        })
    }
}

/// What the sound search found for a phrase.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// One clearly-best name.
    One(Hit),
    /// Two different things sound about equally close: the right move is to ask which one.
    Close(Hit, Hit),
}

/// How much further than [`MAX_DIST`] the runner-up may be and still be offered as the second choice. Zero: on the owner's
/// 320-app list a runner-up at 0.39 ("trey" -> Google Drive, "jim" -> Gmail) was never a sensible second choice.
pub const PAIR_SLACK: f32 = 0.0;

impl PhoneticIndex {
    /// Like [`nearest`](Self::nearest), but when two DIFFERENT things are about equally close (the case `nearest` refuses),
    /// returns both so the caller can ask "A or B?". `None` when the best is too far away, or the runner-up is.
    pub fn nearest_or_close(&self, query: &str) -> Option<Found> {
        if let Some(hit) = self.nearest(query) {
            return Some(Found::One(hit));
        }
        let qp = phrase_phones(query);
        if qp.len() < MIN_PHONES {
            return None;
        }
        let mut scored: Vec<(f32, usize)> = Vec::with_capacity(self.entries.len());
        for (i, e) in self.entries.iter().enumerate() {
            let (a, b) = (qp.len() as f32, e.phones.len() as f32);
            if a / b > 2.0 || b / a > 2.0 {
                continue;
            }
            let d = phoneme_distance(&qp, &e.phones) / qp.len().max(e.phones.len()) as f32;
            scored.push((d, i));
        }
        scored.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        let (bd, bi) = *scored.first()?;
        if bd > MAX_DIST {
            return None;
        }
        let (sd, si) = *scored.iter().find(|(_, i)| self.entries[*i].group != self.entries[bi].group)?;
        if sd > MAX_DIST + PAIR_SLACK {
            return None;
        }
        let hit = |d: f32, i: usize, other: f32| Hit {
            label: self.entries[i].label.clone(),
            group: self.entries[i].group.clone(),
            dist: d,
            margin: (other - d).abs(),
        };
        Some(Found::Close(hit(bd, bi, sd), hit(sd, si, bd)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic Windows app list (names a typical user has installed).
    const APPS: &[&str] = &[
        "Notepad", "Calculator", "Paint", "Brave", "Google Chrome", "Microsoft Edge", "Firefox", "Spotify", "WhatsApp", "Discord", "Slack",
        "Microsoft Teams", "Zoom", "Visual Studio Code", "VS Code", "Word", "Excel", "PowerPoint", "Outlook", "OneNote", "Photos", "Camera",
        "Settings", "Windows Terminal", "File Explorer", "Task Manager", "Snipping Tool", "Clock", "Maps", "Mail", "Calendar", "Weather",
        "Microsoft Store", "Xbox", "Steam", "YouTube", "GitHub Desktop", "Antigravity", "OpenCode", "Cursor", "Postman", "Figma", "VLC",
        "Notion", "Telegram", "Skype", "OBS Studio", "Audacity", "GIMP", "7-Zip", "WinRAR", "Adobe Acrobat", "Sticky Notes", "Voice Recorder",
        "Movies and TV", "Media Player", "Magnifier", "Narrator", "On-Screen Keyboard", "Remote Desktop", "Registry Editor", "Command Prompt",
        "PowerShell", "Windows Security", "Device Manager", "Disk Cleanup", "Character Map", "Quick Assist", "Get Help", "Tips", "Feedback Hub",
        "Phone Link", "Clipchamp", "Copilot", "Cortana", "Dropbox", "OneDrive", "Google Drive", "Java", "Python", "Node", "Docker Desktop",
        "Android Studio", "IntelliJ IDEA", "PyCharm", "Sublime Text", "Notepad Plus Plus", "Everything", "TeamViewer", "AnyDesk", "Epic Games",
        "Battle net", "League of Legends", "Minecraft", "Roblox", "Netflix", "Prime Video", "Hotstar", "Kindle", "Canva", "Grammarly",
    ];

    fn index() -> PhoneticIndex {
        PhoneticIndex::new(APPS.iter().map(|a| (a.to_lowercase(), a.to_lowercase())))
    }

    #[test]
    fn phoneme_table_matches_the_generator() {
        let gen = include_str!("../../scripts/gen_pron_lexicon.py");
        let start = gen.find("PHONEMES = [").unwrap();
        let end = start + gen[start..].find(']').unwrap();
        let listed: Vec<&str> = gen[start..end].split('"').skip(1).step_by(2).collect();
        assert_eq!(listed, PHONEMES.to_vec());
    }

    #[test]
    fn table_is_sorted_and_words_resolve() {
        let words: Vec<&str> = DATA.lines().map(|l| l.split('\t').next().unwrap()).collect();
        assert!(words.windows(2).all(|w| w[0] < w[1]), "binary search needs a strictly sorted table");
        assert!(words.len() > 120_000);
        for w in ["open", "pad", "brave", "laptop", "tomorrow"] {
            assert!(!pronounce_word(w).is_empty(), "{w}");
        }
    }

    #[test]
    fn sound_alikes_are_closer_than_strangers() {
        let d = |a: &str, b: &str| phoneme_distance(&phrase_phones(a), &phrase_phones(b)) / phrase_phones(a).len().max(phrase_phones(b).len()) as f32;
        assert!(d("note pad", "notepad") < 0.05);
        assert!(d("load pad", "notepad") < d("load pad", "calculator"));
        assert!(d("cat", "cap") > 0.0);
        assert_eq!(d("brave", "brave"), 0.0);
    }

    /// (what the recogniser heard, the app it meant). Real slips from the owner's logs first.
    const SLIPS: &[(&str, &str)] = &[
        ("load pad", "notepad"), ("note pad", "notepad"), ("brief", "brave"), ("breath", "brave"), ("spot if I", "spotify"), ("what's up", "whatsapp"),
        ("vs coat", "vs code"), ("this cord", "discord"), ("you tube", "youtube"), ("anti gravity", "antigravity"), ("open code", "opencode"),
        ("figure", "figma"), ("post men", "postman"), ("tell a gram", "telegram"), ("slap", "slack"), ("zoom in", "zoom"), ("calculater", "calculator"),
        ("pain", "paint"), ("fire fox", "firefox"), ("power point", "powerpoint"), ("out look", "outlook"), ("excell", "excel"), ("word pad", "notepad"),
    ];

    #[test]
    fn real_and_typical_slips_find_the_right_app() {
        let idx = index();
        let mut right = 0;
        let mut wrong = Vec::new();
        let mut none = Vec::new();
        for (heard, meant) in SLIPS {
            match idx.nearest(heard) {
                Some(h) if h.group == *meant => right += 1,
                Some(h) => wrong.push((*heard, h.group)),
                None => none.push(*heard),
            }
        }
        eprintln!("sound match: right {right}/{} wrong {wrong:?} no-suggestion {none:?}", SLIPS.len());
        // A wrong suggestion is only a "did you mean?" question, but it is still a cost: none of the real owner slips may be wrong.
        assert!(wrong.len() <= 2, "wrong suggestions: {wrong:?}");
        assert!(right * 100 >= SLIPS.len() * 75, "found only {right}/{}; missed {none:?}", SLIPS.len());
        for (heard, meant) in &SLIPS[..8] {
            let h = idx.nearest(heard).unwrap_or_else(|| panic!("real slip '{heard}' got no suggestion"));
            assert_eq!(h.group, *meant, "real slip '{heard}'");
        }
    }

    #[test]
    fn ordinary_words_rarely_produce_a_suggestion() {
        // Every 25th dictionary word of 4 to 9 letters, asked as if the user had said "open <word>".
        let idx = index();
        let mut asked = 0usize;
        let mut hits = Vec::new();
        for (i, line) in DATA.lines().enumerate() {
            if i % 25 != 0 {
                continue;
            }
            let w = line.split('\t').next().unwrap();
            if !(4..=9).contains(&w.len()) || w.contains('\'') {
                continue;
            }
            asked += 1;
            if let Some(h) = idx.nearest(w) {
                hits.push(format!("{w}->{}", h.group));
            }
        }
        let rate = hits.len() as f32 / asked as f32;
        eprintln!("ordinary words asked {asked}, suggestions {} ({:.1}%), e.g. {:?}", hits.len(), rate * 100.0, &hits[..hits.len().min(12)]);
        assert!(rate < 0.05, "too many unrelated words get a suggestion: {:.1}%", rate * 100.0);
    }

    /// Dev tool: `cargo test --lib tune_grid -- --ignored --nocapture` prints, for each threshold pair, how many typical slips are found,
    /// how many are wrong, and how many unrelated dictionary words get a suggestion.
    #[test]
    #[ignore]
    fn tune_grid() {
        let idx = index();
        let words: Vec<&str> = DATA
            .lines()
            .enumerate()
            .filter(|(i, _)| i % 25 == 0)
            .map(|(_, l)| l.split('\t').next().unwrap())
            .filter(|w| (4..=9).contains(&w.len()) && !w.contains('\''))
            .collect();
        println!("max_dist margin | slips right/wrong/none | real-8 right | unrelated words suggested");
        for max_dist in [0.14f32, 0.18, 0.22, 0.26, 0.30, 0.34] {
            for margin in [0.05f32, 0.08, 0.12, 0.16] {
                let (mut right, mut wrong, mut none) = (0, 0, 0);
                for (heard, meant) in SLIPS {
                    match idx.nearest_with(heard, max_dist, margin) {
                        Some(h) if h.group == *meant => right += 1,
                        Some(_) => wrong += 1,
                        None => none += 1,
                    }
                }
                let real8 = SLIPS[..8].iter().filter(|(h, m)| idx.nearest_with(h, max_dist, margin).map_or(false, |x| x.group == *m)).count();
                let fp = words.iter().filter(|w| idx.nearest_with(w, max_dist, margin).is_some()).count();
                println!("{max_dist:.2} {margin:.2} | {right}/{wrong}/{none} of {} | {real8}/8 | {:.1}% ({fp}/{})", SLIPS.len(), 100.0 * fp as f32 / words.len() as f32, words.len());
            }
        }
    }

    #[test]
    fn a_name_matches_itself_and_two_apps_never_tie_into_a_guess() {
        let idx = index();
        for a in APPS {
            let h = idx.nearest(&a.to_lowercase());
            if let Some(h) = h {
                assert_eq!(h.group, a.to_lowercase(), "{a}");
                assert!(h.dist < 0.01);
            }
        }
        // Two different apps that sound alike are a coin toss: no suggestion, even for the exact sound of one of them.
        let tie = PhoneticIndex::new([("pat".to_string(), "pat".to_string()), ("bat".to_string(), "bat".to_string())]);
        assert!(tie.nearest("pat").is_none(), "pat/bat are too close to pick between");
        // A clearly different rival leaves a confident pick.
        let clear = PhoneticIndex::new([("pat".to_string(), "pat".to_string()), ("calculator".to_string(), "calculator".to_string())]);
        assert_eq!(clear.nearest("pat").map(|h| h.group), Some("pat".to_string()));
    }

    #[test]
    fn several_names_for_one_app_are_not_rivals() {
        let idx = PhoneticIndex::new([
            ("brave".to_string(), "brave".to_string()),
            ("brave browser".to_string(), "brave".to_string()),
            ("chrome".to_string(), "chrome".to_string()),
        ]);
        let h = idx.nearest("brief").expect("both names belong to one app");
        assert_eq!(h.group, "brave");
    }

    #[test]
    fn footprint_and_speed_budgets() {
        assert!(index_heap_bytes() < 600_000, "index heap {} bytes", index_heap_bytes());
        let idx = index();
        let t = std::time::Instant::now();
        for _ in 0..200 {
            let _ = idx.nearest("spot if I");
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / 200.0;
        eprintln!("nearest() over {} names: {per:.3} ms per query, index heap {} KB", idx.len(), index_heap_bytes() / 1024);
        assert!(per < 20.0, "{per} ms per query is too slow even for a debug build");
    }

    #[test]
    fn empty_and_tiny_queries_never_match() {
        let idx = index();
        for q in ["", " ", "a", "to", "is it"] {
            assert!(idx.nearest(q).is_none(), "'{q}'");
        }
    }
}

#[cfg(test)]
mod close_tests {
    use super::*;

    fn idx(names: &[&str]) -> PhoneticIndex {
        PhoneticIndex::new(names.iter().map(|a| (a.to_lowercase(), a.to_lowercase())))
    }

    #[test]
    fn two_equally_close_apps_are_returned_as_a_pair() {
        let i = idx(&["Clock", "Block", "Calendar", "Mail", "Zoom"]);
        match i.nearest_or_close("lock") {
            Some(Found::Close(a, b)) => {
                let mut got = [a.label, b.label];
                got.sort();
                assert_eq!(got, ["block".to_string(), "clock".to_string()]);
            }
            other => panic!("expected a close pair, got {other:?}"),
        }
        // "potion" is as near to Notion as to Motion.
        let i = idx(&["Notion", "Motion", "Calendar"]);
        assert!(matches!(i.nearest_or_close("potion"), Some(Found::Close(..))));
    }

    #[test]
    fn a_clear_winner_is_not_a_question() {
        let i = idx(&["Clock", "Block", "Calendar", "Mail", "Zoom", "Cursor"]);
        assert!(matches!(i.nearest_or_close("calendar"), Some(Found::One(h)) if h.label == "calendar"));
        assert!(matches!(i.nearest_or_close("cursor"), Some(Found::One(_))));
    }

    #[test]
    fn nothing_close_gives_no_pair_even_if_two_names_exist() {
        let i = idx(&["Clock", "Block", "Calendar"]);
        assert_eq!(i.nearest_or_close("elephant parade tomorrow"), None);
        assert_eq!(i.nearest_or_close("a"), None);
    }

    #[test]
    fn names_of_the_same_app_are_not_rivals() {
        let i = PhoneticIndex::new([
            ("google chrome".to_string(), "google chrome".to_string()),
            ("chrome".to_string(), "google chrome".to_string()),
        ]);
        assert!(!matches!(i.nearest_or_close("crome"), Some(Found::Close(..))));
    }
}
