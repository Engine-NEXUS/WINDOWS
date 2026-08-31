//! Enhanced intent parser — Rust-side command understanding with app registry
//! fuzzy matching, analyse/repo/PR entity extraction, and NLU server fallback.
//!
//! Architecture:
//!   1. Deterministic regex patterns for command structure (open, analyse, search, media)
//!   2. App registry fuzzy matching for app names (uses ALL installed apps, not a fixed list)
//!   3. Entity extraction for repo names, PR numbers, owners
//!   4. NLU server (BERT-Mini) as a confidence booster — lazy-started Python sidecar
//!   5. Falls back to the frontend regex parser if NLU is unavailable
//!
//! This replaces the frontend TypeScript parser for better accuracy:
//!   - Uses the app registry (hundreds of installed apps) instead of a fixed list of 50
//!   - Handles "analyse PR 23 servx", "analyse servx repo", "analyse owner/repo"
//!   - Phonetic + Levenshtein matching against real installed app names
//!   - Confidence scoring with fallback to remote backend

use crate::app_registry;
use serde::{Deserialize, Serialize};

// ─── Types ─────────────────────────────────────────────────────────────────

/// Parsed intent — same shape as the frontend Intent type, plus new analyse intents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum ParsedIntent {
    #[serde(rename = "open_app")]
    OpenApp { target: String },
    #[serde(rename = "open_url")]
    OpenUrl { target: String, url: String },
    #[serde(rename = "open_architect")]
    OpenArchitect,
    #[serde(rename = "search")]
    Search { query: String },
    #[serde(rename = "analyse_repo")]
    AnalyseRepo { owner: Option<String>, repo: String },
    #[serde(rename = "analyse_pr")]
    AnalysePr {
        owner: Option<String>,
        repo: String,
        pr_number: u32,
    },
    #[serde(rename = "media_play_pause")]
    MediaPlayPause,
    #[serde(rename = "media_next")]
    MediaNext,
    #[serde(rename = "media_previous")]
    MediaPrevious,
    #[serde(rename = "media_stop")]
    MediaStop,
    /// NLU server result — used when the deterministic parser is uncertain
    /// and the NLU server returns a classification.
    #[serde(rename = "nlu_result")]
    NluResult {
        intent: String,
        slots: serde_json::Value,
        confidence: f32,
    },
    #[serde(rename = "unknown")]
    Unknown { raw: String },
}

/// Result of parsing a transcript.
#[derive(Debug, Clone, Serialize)]
pub struct ParseResult {
    pub intent: ParsedIntent,
    /// Confidence score 0.0–1.0. Deterministic matches are 1.0.
    /// NLU server matches are the model's confidence.
    pub confidence: f32,
    /// Source of the parse: "deterministic", "nlu", "fallback"
    pub source: String,
}

// ─── Deterministic parser ──────────────────────────────────────────────────

/// Parse a transcript into a structured intent using deterministic rules.
///
/// This is the primary parser. It handles:
/// - "open <app>" / "launch <app>" / etc. → open_app (with app registry fuzzy match)
/// - "analyse <repo>" / "analyse PR <num> <repo>" / "analyse <owner>/<repo>"
/// - "search for <query>" / "google <query>"
/// - "open architecture mapper"
/// - Media controls (pause, next, previous, stop)
/// - "open <url>" (direct URL)
pub fn parse_deterministic(transcript: &str) -> Option<ParseResult> {
    let text = transcript.trim().to_lowercase();
    let text = normalize_whitespace(&text);

    if text.is_empty() {
        return None;
    }

    // --- Open Architecture Mapper ---
    if is_architect_command(&text) {
        return Some(ParseResult {
            intent: ParsedIntent::OpenArchitect,
            confidence: 1.0,
            source: "deterministic".to_string(),
        });
    }

    // --- Media Control ---
    if let Some(media) = parse_media(&text) {
        return Some(ParseResult {
            intent: media,
            confidence: 1.0,
            source: "deterministic".to_string(),
        });
    }

    // --- Analyse commands ---
    // "analyse PR 23 servx", "analyse pr 23 in servx", "analyse pull request 23 servx"
    // "analyse servx repo", "analyse servx", "analyse owner/repo"
    // "analyse repo servx", "analyse the repo servx"
    if let Some(result) = parse_analyse_command(&text) {
        return Some(result);
    }

    // --- Open app / URL ---
    // "open whatsapp", "launch gemini", "start calculator", etc.
    if let Some(result) = parse_open_command(&text) {
        return Some(result);
    }

    // --- Search ---
    // "search for cats", "google cats", "look up cats"
    if let Some(result) = parse_search_command(&text) {
        return Some(result);
    }

    None
}

// ─── Open command ──────────────────────────────────────────────────────────

/// Verbs that trigger an "open" command.
const OPEN_VERBS: &[&str] = &[
    "open", "launch", "start", "run", "fire up", "bring up", "show", "pull up",
    "go to", "visit", "browse to", "navigate to",
];

fn parse_open_command(text: &str) -> Option<ParseResult> {
    // Try each open verb
    for verb in OPEN_VERBS {
        let prefix = format!("{} ", verb);
        if text.starts_with(&prefix) {
            let target = &text[prefix.len()..];
            let target = target.trim();

            // Strip trailing "app", "application", "for me"
            let cleaned = strip_trailing_app_words(target);

            // Check for "in browser" / "website" / "site" escape hatch
            if let Some(result) = parse_browser_force(&cleaned) {
                return Some(result);
            }

            // Strip trailing "website"/"site"
            let cleaned_no_site = strip_trailing_site(&cleaned);

            // Direct URL: has a dot, no spaces
            if is_url_like(&cleaned_no_site) {
                let url = if cleaned_no_site.starts_with("http") {
                    cleaned_no_site.clone()
                } else {
                    format!("https://{}", cleaned_no_site)
                };
                return Some(ParseResult {
                    intent: ParsedIntent::OpenUrl {
                        target: cleaned_no_site.clone(),
                        url,
                    },
                    confidence: 1.0,
                    source: "deterministic".to_string(),
                });
            }

            // App name — resolve against the app registry
            let resolved = resolve_app_name(&cleaned_no_site);
            return Some(ParseResult {
                intent: ParsedIntent::OpenApp {
                    target: resolved.unwrap_or_else(|| cleaned_no_site.to_string()),
                },
                confidence: 1.0,
                source: "deterministic".to_string(),
            });
        }
    }

    None
}

/// Resolve an app name using the app registry with fuzzy matching.
/// Falls back to the original text if no match is found.
fn resolve_app_name(name: &str) -> Option<String> {
    let name = name.trim();

    // 1. Direct registry lookup (handles exact, prefix, contains, Levenshtein)
    if let Some(entry) = app_registry::lookup(name) {
        // Return the first search name (canonical form)
        if let Some(canonical) = entry.search_names.first() {
            tracing::debug!(
                "app registry match: '{}' → '{}' ({})",
                name,
                canonical,
                entry.display_name
            );
            return Some(canonical.clone());
        }
        return Some(entry.display_name.to_lowercase());
    }

    // 2. Phonetic correction against the app registry
    // This handles Whisper mishearings like "what's app" → "whatsapp"
    if let Some(corrected) = phonetic_app_lookup(name) {
        tracing::debug!("phonetic app match: '{}' → '{}'", name, corrected);
        return Some(corrected);
    }

    // 3. Try with spaces removed/added (e.g. "whats app" → "whatsapp", "googlechrome" → "google chrome")
    if let Some(corrected) = space_variation_lookup(name) {
        tracing::debug!("space variation match: '{}' → '{}'", name, corrected);
        return Some(corrected);
    }

    None
}

/// Try looking up the app name with space variations.
/// "whats app" → try "whatsapp", "googlechrome" → try "google chrome"
fn space_variation_lookup(name: &str) -> Option<String> {
    // Remove all spaces: "what's app" → "what'sapp"
    let no_spaces = name.replace(' ', "");
    if no_spaces != name {
        if let Some(entry) = app_registry::lookup(&no_spaces) {
            if let Some(canonical) = entry.search_names.first() {
                return Some(canonical.clone());
            }
        }
    }

    // Try adding a space at common boundaries (consonant→vowel transitions)
    // This is a simple heuristic for compound words
    let chars: Vec<char> = name.chars().collect();
    for i in 1..chars.len() {
        let prev = chars[i - 1];
        let curr = chars[i];
        // Insert space between consonant and vowel (e.g. "googlechrome" → "google chrome")
        if !is_vowel(prev) && is_vowel(curr) {
            let mut modified = name[..i].to_string();
            modified.push(' ');
            modified.push_str(&name[i..]);
            if let Some(entry) = app_registry::lookup(&modified) {
                if let Some(canonical) = entry.search_names.first() {
                    return Some(canonical.clone());
                }
            }
        }
    }

    None
}

fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

/// Phonetic app lookup — tries to match the spoken word against app names
/// using simple phonetic similarity (sound-alike matching).
///
/// This is a lightweight alternative to Double Metaphone that works against
/// the live app registry instead of a fixed list.
fn phonetic_app_lookup(name: &str) -> Option<String> {
    let name_lower = name.to_lowercase();
    let name_pho = simple_phonetic(&name_lower);

    // Get all app names from the registry
    let search_names = app_registry::all_search_names();

    let mut best_match: Option<(String, usize)> = None;

    for search_name in &search_names {
        let app_pho = simple_phonetic(search_name);
        if app_pho.is_empty() || name_pho.is_empty() {
            continue;
        }

        // Exact phonetic match
        if app_pho == name_pho {
            let score = if search_name.len() == name_lower.len() {
                3
            } else {
                2
            };
            if best_match.as_ref().map_or(true, |b| score > b.1) {
                best_match = Some((search_name.to_string(), score));
            }
        }
        // Partial phonetic match (first 2 chars)
        else if app_pho.len() >= 2 && name_pho.len() >= 2 {
            if app_pho[..2] == name_pho[..2] {
                let dist = levenshtein(&name_lower, search_name);
                if dist <= 3 && dist < name_lower.len() / 2 + 1 {
                    let score = 1;
                    if best_match.as_ref().map_or(true, |b| score > b.1) {
                        best_match = Some((search_name.to_string(), score));
                    }
                }
            }
        }
    }

    best_match.map(|(name, _)| name)
}

/// Simple phonetic encoding — removes vowels and normalizes consonant clusters.
/// This is a very lightweight phonetic representation (not as sophisticated as
/// Double Metaphone, but good enough for app name matching against the registry).
fn simple_phonetic(word: &str) -> String {
    let w = word.to_uppercase();
    let mut result = String::new();
    let chars: Vec<char> = w.chars().filter(|c| c.is_alphabetic()).collect();

    for (i, &c) in chars.iter().enumerate() {
        if i == 0 {
            result.push(c);
            continue;
        }

        // Skip vowels (except at start)
        if is_vowel(c) {
            continue;
        }

        // Normalize consonant clusters
        let prev = chars[i - 1];
        match c {
            // C and K sound the same
            'C' => {
                if prev != 'C' && prev != 'K' {
                    result.push('K');
                }
            }
            'K' => {
                if prev != 'C' && prev != 'K' {
                    result.push('K');
                }
            }
            // PH → F
            'H' => {
                if prev == 'P' {
                    // Replace last P with F
                    if let Some(last) = result.chars().last() {
                        if last == 'P' {
                            result.pop();
                            result.push('F');
                        }
                    }
                }
            }
            // Skip duplicate consonants
            _ => {
                if result.chars().last() != Some(c) {
                    result.push(c);
                }
            }
        }
    }

    result
}

// ─── Analyse command ───────────────────────────────────────────────────────

/// Parse "analyse" commands:
/// - "analyse PR 23 servx" / "analyse pr 23 in servx" / "analyse pull request 23 servx"
/// - "analyse servx repo" / "analyse the repo servx" / "analyse repo servx"
/// - "analyse servx" / "analyse owner/repo"
/// - "analyse PR 23 owner/repo"
fn parse_analyse_command(text: &str) -> Option<ParseResult> {
    // Must start with "analyse" or "analyze"
    let analyse_text = if text.starts_with("analyse ") {
        &text[8..]
    } else if text.starts_with("analyze ") {
        &text[8..]
    } else {
        return None;
    };

    let analyse_text = analyse_text.trim();

    // Pattern 1: "PR <num> [in|of|for|from] <repo>" or "pull request <num> ..."
    // e.g. "PR 23 servx", "PR 23 in servx", "pull request 23 servx"
    if let Some(result) = parse_pr_analyse(analyse_text) {
        return Some(result);
    }

    // Pattern 2: "<owner>/<repo>" — e.g. "zync-meet/zync", "eesh264/congi"
    if let Some(result) = parse_owner_repo_analyse(analyse_text) {
        return Some(result);
    }

    // Pattern 3: "<repo> repo" or "repo <repo>" or "the repo <repo>"
    // e.g. "servx repo", "repo servx", "the repo servx"
    if let Some(result) = parse_repo_keyword_analyse(analyse_text) {
        return Some(result);
    }

    // Pattern 4: Just "<repo>" — e.g. "analyse servx", "analyse zync"
    // Treat the whole remaining text as the repo name
    let repo = clean_repo_name(analyse_text);
    if !repo.is_empty() {
        return Some(ParseResult {
            intent: ParsedIntent::AnalyseRepo {
                owner: None,
                repo,
            },
            confidence: 0.9, // slightly lower — we're guessing this is a repo name
            source: "deterministic".to_string(),
        });
    }

    None
}

/// Parse "PR <num> [in|of|for|from] <repo>" patterns.
fn parse_pr_analyse(text: &str) -> Option<ParseResult> {
    // Match: "PR <num> [in|of|for|from] <repo>" or "pull request <num> ..."
    let pr_patterns = [
        regex::Regex::new(r"^pr\s*#?\s*(\d+)\s+(?:in|of|for|from)\s+(.+)$").ok()?,
        regex::Regex::new(r"^pr\s*#?\s*(\d+)\s+(.+)$").ok()?,
        regex::Regex::new(r"^pull\s+request\s*#?\s*(\d+)\s+(?:in|of|for|from)\s+(.+)$").ok()?,
        regex::Regex::new(r"^pull\s+request\s*#?\s*(\d+)\s+(.+)$").ok()?,
        // "PR <num> owner/repo"
        regex::Regex::new(r"^pr\s*#?\s*(\d+)\s+(\S+/\S+)$").ok()?,
    ];

    for pat in &pr_patterns {
        if let Some(caps) = pat.captures(text) {
            let pr_number: u32 = caps[1].parse().ok()?;
            let repo_part = caps[2].trim();

            // Check if repo_part is owner/repo format
            if let Some((owner, repo)) = parse_owner_repo(repo_part) {
                return Some(ParseResult {
                    intent: ParsedIntent::AnalysePr {
                        owner: Some(owner),
                        repo,
                        pr_number,
                    },
                    confidence: 1.0,
                    source: "deterministic".to_string(),
                });
            }

            // Just repo name
            let repo = clean_repo_name(repo_part);
            if !repo.is_empty() {
                return Some(ParseResult {
                    intent: ParsedIntent::AnalysePr {
                        owner: None,
                        repo,
                        pr_number,
                    },
                    confidence: 1.0,
                    source: "deterministic".to_string(),
                });
            }
        }
    }

    None
}

/// Parse "owner/repo" format.
fn parse_owner_repo_analyse(text: &str) -> Option<ParseResult> {
    if let Some((owner, repo)) = parse_owner_repo(text) {
        return Some(ParseResult {
            intent: ParsedIntent::AnalyseRepo {
                owner: Some(owner),
                repo,
            },
            confidence: 1.0,
            source: "deterministic".to_string(),
        });
    }
    None
}

/// Parse "<repo> repo" / "repo <repo>" / "the repo <repo>" patterns.
fn parse_repo_keyword_analyse(text: &str) -> Option<ParseResult> {
    // "the repo <name>" or "repo <name>"
    if let Some(rest) = text.strip_prefix("the repo ") {
        let repo = clean_repo_name(rest);
        if !repo.is_empty() {
            return Some(ParseResult {
                intent: ParsedIntent::AnalyseRepo {
                    owner: None,
                    repo,
                },
                confidence: 1.0,
                source: "deterministic".to_string(),
            });
        }
    }
    if let Some(rest) = text.strip_prefix("repo ") {
        let repo = clean_repo_name(rest);
        if !repo.is_empty() {
            return Some(ParseResult {
                intent: ParsedIntent::AnalyseRepo {
                    owner: None,
                    repo,
                },
                confidence: 1.0,
                source: "deterministic".to_string(),
            });
        }
    }
    // "<name> repo" — trailing "repo" keyword
    if let Some(rest) = text.strip_suffix(" repo") {
        let repo = clean_repo_name(rest);
        if !repo.is_empty() {
            return Some(ParseResult {
                intent: ParsedIntent::AnalyseRepo {
                    owner: None,
                    repo,
                },
                confidence: 1.0,
                source: "deterministic".to_string(),
            });
        }
    }
    // "<name> repository"
    if let Some(rest) = text.strip_suffix(" repository") {
        let repo = clean_repo_name(rest);
        if !repo.is_empty() {
            return Some(ParseResult {
                intent: ParsedIntent::AnalyseRepo {
                    owner: None,
                    repo,
                },
                confidence: 1.0,
                source: "deterministic".to_string(),
            });
        }
    }

    None
}

/// Parse "owner/repo" string into (owner, repo).
fn parse_owner_repo(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    if let Some(slash_idx) = text.find('/') {
        let owner = text[..slash_idx].trim().to_string();
        let repo = text[slash_idx + 1..].trim().to_string();
        // Validate: both parts should be non-empty and contain only valid chars
        if !owner.is_empty() && !repo.is_empty() && is_valid_repo_name(&owner) && is_valid_repo_name(&repo) {
            return Some((owner, repo));
        }
    }
    None
}

/// Check if a string is a valid GitHub repo/owner name.
/// GitHub names: alphanumeric, hyphens, underscores, dots.
fn is_valid_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !name.starts_with('-')
        && !name.starts_with('.')
}

/// Clean a repo name — strip articles, trailing keywords, whitespace.
fn clean_repo_name(text: &str) -> String {
    let text = text.trim();
    // Strip leading "the "
    let text = text.strip_prefix("the ").unwrap_or(text);
    // Strip trailing "repo", "repository", "project", "codebase"
    let text = text
        .strip_suffix(" repo")
        .or_else(|| text.strip_suffix(" repository"))
        .or_else(|| text.strip_suffix(" project"))
        .or_else(|| text.strip_suffix(" codebase"))
        .unwrap_or(text);
    text.trim().to_string()
}

// ─── Search command ────────────────────────────────────────────────────────

const SEARCH_VERBS: &[&str] = &[
    "search for", "search", "google", "look up", "find me", "find", "look for",
];

fn parse_search_command(text: &str) -> Option<ParseResult> {
    for verb in SEARCH_VERBS {
        let prefix = format!("{} ", verb);
        if text.starts_with(&prefix) {
            let query = text[prefix.len()..].trim();
            if !query.is_empty() {
                return Some(ParseResult {
                    intent: ParsedIntent::Search {
                        query: query.to_string(),
                    },
                    confidence: 1.0,
                    source: "deterministic".to_string(),
                });
            }
        }
    }
    None
}

// ─── Media control ─────────────────────────────────────────────────────────

fn parse_media(text: &str) -> Option<ParsedIntent> {
    if regex_match(text, r"^(?:pause|pause\s+music|pause\s+media|play|resume|resume\s+music|play\s*[/\s]*pause|toggle\s+media)$") {
        return Some(ParsedIntent::MediaPlayPause);
    }
    if regex_match(text, r"^(?:next|next\s+song|next\s+track|skip|skip\s+song|skip\s+track)$") {
        return Some(ParsedIntent::MediaNext);
    }
    if regex_match(text, r"^(?:previous|previous\s+song|previous\s+track|prev|prev\s+song|go\s+back\s+a\s+song)$") {
        return Some(ParsedIntent::MediaPrevious);
    }
    if regex_match(text, r"^(?:stop\s+music|stop\s+media|stop\s+playback)$") {
        return Some(ParsedIntent::MediaStop);
    }
    None
}

// ─── Architecture mapper ───────────────────────────────────────────────────

fn is_architect_command(text: &str) -> bool {
    regex_match(
        text,
        r"^(?:open|launch|start|show|bring\s+up|pull\s+up)\s+(?:the\s+)?(?:architecture|architect)(?:\s+(?:mapper|map|window|mapper\s+window))?$",
    ) || regex_match(
        text,
        r"^(?:open|launch|start|show)\s+the\s+(?:architecture|architect)(?:\s+(?:mapper|map|window))?$",
    )
}

// ─── Browser force ─────────────────────────────────────────────────────────

/// URL map for "open <app> in browser" commands.
const BROWSER_FORCE_URLS: &[(&str, &str)] = &[
    ("gmail", "https://mail.google.com"),
    ("google mail", "https://mail.google.com"),
    ("youtube", "https://www.youtube.com"),
    ("you tube", "https://www.youtube.com"),
    ("github", "https://github.com"),
    ("git hub", "https://github.com"),
    ("twitter", "https://twitter.com"),
    ("x", "https://x.com"),
    ("facebook", "https://facebook.com"),
    ("instagram", "https://instagram.com"),
    ("reddit", "https://reddit.com"),
    ("linkedin", "https://linkedin.com"),
    ("whatsapp", "https://web.whatsapp.com"),
    ("whatsapp web", "https://web.whatsapp.com"),
    ("spotify", "https://open.spotify.com"),
    ("netflix", "https://netflix.com"),
    ("amazon", "https://amazon.com"),
    ("google drive", "https://drive.google.com"),
    ("google docs", "https://docs.google.com"),
    ("google sheets", "https://sheets.google.com"),
    ("google slides", "https://slides.google.com"),
    ("google maps", "https://maps.google.com"),
    ("google calendar", "https://calendar.google.com"),
    ("google translate", "https://translate.google.com"),
    ("google photos", "https://photos.google.com"),
    ("google news", "https://news.google.com"),
    ("google meet", "https://meet.google.com"),
    ("google chat", "https://chat.google.com"),
    ("google play", "https://play.google.com"),
    ("play store", "https://play.google.com"),
    ("app store", "https://apps.apple.com"),
    ("chatgpt", "https://chat.openai.com"),
    ("chat gpt", "https://chat.openai.com"),
    ("open ai", "https://chat.openai.com"),
    ("openai", "https://chat.openai.com"),
    ("claude", "https://claude.ai"),
    ("figma", "https://figma.com"),
    ("notion", "https://notion.so"),
    ("slack", "https://slack.com"),
    ("discord", "https://discord.com/app"),
    ("twitch", "https://twitch.tv"),
    ("stack overflow", "https://stackoverflow.com"),
    ("stackoverflow", "https://stackoverflow.com"),
    ("wikipedia", "https://wikipedia.org"),
    ("chat", "https://chat.google.com"),
    ("maps", "https://maps.google.com"),
    ("translate", "https://translate.google.com"),
    ("calendar", "https://calendar.google.com"),
];

fn parse_browser_force(text: &str) -> Option<ParseResult> {
    // "open gmail in browser" / "open gmail website" / "open gmail site"
    let patterns = [
        regex::Regex::new(r"^(.+?)\s+in\s+(?:the\s+)?browser$").ok()?,
        regex::Regex::new(r"^(.+?)\s+website$").ok()?,
        regex::Regex::new(r"^(.+?)\s+site$").ok()?,
        regex::Regex::new(r"^(.+?)\s+on\s+(?:the\s+)?web$").ok()?,
        regex::Regex::new(r"^(.+?)\s+web\s+version$").ok()?,
    ];

    for pat in &patterns {
        if let Some(caps) = pat.captures(text) {
            let app_name = caps[1].trim();
            // Check URL map
            for (key, url) in BROWSER_FORCE_URLS {
                if *key == app_name {
                    return Some(ParseResult {
                        intent: ParsedIntent::OpenUrl {
                            target: app_name.to_string(),
                            url: url.to_string(),
                        },
                        confidence: 1.0,
                        source: "deterministic".to_string(),
                    });
                }
            }
            // Unknown app + "in browser" → construct URL if it looks like a domain
            if app_name.contains('.') && !app_name.contains(' ') {
                let url = if app_name.starts_with("http") {
                    app_name.to_string()
                } else {
                    format!("https://{}", app_name)
                };
                return Some(ParseResult {
                    intent: ParsedIntent::OpenUrl {
                        target: app_name.to_string(),
                        url,
                    },
                    confidence: 1.0,
                    source: "deterministic".to_string(),
                });
            }
        }
    }

    None
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_trailing_app_words(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_suffix(" app")
        .or_else(|| s.strip_suffix(" application"))
        .or_else(|| s.strip_suffix(" for me"))
        .unwrap_or(s);
    s.trim().to_string()
}

fn strip_trailing_site(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_suffix(" website")
        .or_else(|| s.strip_suffix(" site"))
        .unwrap_or(s);
    s.trim().to_string()
}

fn is_url_like(s: &str) -> bool {
    s.contains('.') && !s.contains(' ')
}

/// Levenshtein edit distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

// ─── Regex helper ──────────────────────────────────────────────────────────
// We use the `regex` crate for pattern matching. It's already in the dependency
// tree via other crates, but we need to add it explicitly to Cargo.toml.

/// Simple regex match helper.
fn regex_match(text: &str, pattern: &str) -> bool {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, regex::Regex>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut guard = cache.lock().unwrap();
    let re = guard
        .entry(pattern.to_string())
        .or_insert_with(|| regex::Regex::new(pattern).unwrap_or_else(|_| regex::Regex::new("$^").unwrap()));
    re.is_match(text)
}

// ─── Tauri command ─────────────────────────────────────────────────────────

/// Parse a transcript into a structured intent.
///
/// Tries the deterministic parser first (fast, zero-latency).
/// If the deterministic parser returns None or low confidence,
/// tries the NLU server (BERT-Mini, lazy-started Python sidecar).
/// Falls back to `unknown` if both fail.
#[tauri::command]
pub async fn parse_transcript(transcript: String) -> Result<ParseResult, String> {
    tracing::info!("[intent_parser] parsing: {:?}", transcript);

    // 1. Try deterministic parser
    if let Some(result) = parse_deterministic(&transcript) {
        tracing::info!(
            "[intent_parser] deterministic: {:?} (confidence={}, source={})",
            result.intent,
            result.confidence,
            result.source
        );
        return Ok(result);
    }

    // 2. Try NLU server (if available)
    if let Some(result) = crate::nlu_client::parse_via_nlu(&transcript).await {
        tracing::info!(
            "[intent_parser] nlu: {:?} (confidence={})",
            result.intent,
            result.confidence
        );
        return Ok(result);
    }

    // 3. Fallback: unknown
    tracing::info!("[intent_parser] no match, returning unknown");
    Ok(ParseResult {
        intent: ParsedIntent::Unknown {
            raw: transcript.clone(),
        },
        confidence: 0.0,
        source: "fallback".to_string(),
    })
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_app() {
        let result = parse_deterministic("open whatsapp");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));
    }

    #[test]
    fn test_open_gemini() {
        let result = parse_deterministic("open gemini");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));
    }

    #[test]
    fn test_open_chrome() {
        let result = parse_deterministic("open chrome");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));
    }

    #[test]
    fn test_launch_spotify() {
        let result = parse_deterministic("launch spotify");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));
    }

    #[test]
    fn test_open_app_with_app_suffix() {
        let result = parse_deterministic("open gmail app");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::OpenApp { target } = r.intent {
            assert_eq!(target, "gmail");
        } else {
            panic!("expected OpenApp");
        }
    }

    #[test]
    fn test_open_in_browser() {
        let result = parse_deterministic("open gmail in browser");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenUrl { .. }));
    }

    #[test]
    fn test_analyse_pr() {
        let result = parse_deterministic("analyse PR 23 servx");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::AnalysePr {
            repo, pr_number, ..
        } = r.intent
        {
            assert_eq!(repo, "servx");
            assert_eq!(pr_number, 23);
        } else {
            panic!("expected AnalysePr, got {:?}", r.intent);
        }
    }

    #[test]
    fn test_analyse_pr_with_in() {
        let result = parse_deterministic("analyse PR 23 in servx");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::AnalysePr {
            repo, pr_number, ..
        } = r.intent
        {
            assert_eq!(repo, "servx");
            assert_eq!(pr_number, 23);
        } else {
            panic!("expected AnalysePr");
        }
    }

    #[test]
    fn test_analyse_pr_owner_repo() {
        let result = parse_deterministic("analyse PR 5 zync-meet/zync");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::AnalysePr {
            owner,
            repo,
            pr_number,
        } = r.intent
        {
            assert_eq!(owner, Some("zync-meet".to_string()));
            assert_eq!(repo, "zync");
            assert_eq!(pr_number, 5);
        } else {
            panic!("expected AnalysePr");
        }
    }

    #[test]
    fn test_analyse_repo() {
        let result = parse_deterministic("analyse servx repo");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::AnalyseRepo { repo, owner } = r.intent {
            assert_eq!(repo, "servx");
            assert_eq!(owner, None);
        } else {
            panic!("expected AnalyseRepo");
        }
    }

    #[test]
    fn test_analyse_owner_repo() {
        let result = parse_deterministic("analyse zync-meet/zync");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::AnalyseRepo { owner, repo } = r.intent {
            assert_eq!(owner, Some("zync-meet".to_string()));
            assert_eq!(repo, "zync");
        } else {
            panic!("expected AnalyseRepo");
        }
    }

    #[test]
    fn test_analyse_just_repo() {
        let result = parse_deterministic("analyse servx");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::AnalyseRepo { repo, .. } = r.intent {
            assert_eq!(repo, "servx");
        } else {
            panic!("expected AnalyseRepo");
        }
    }

    #[test]
    fn test_analyse_zync() {
        let result = parse_deterministic("analyse zync");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::AnalyseRepo { repo, .. } = r.intent {
            assert_eq!(repo, "zync");
        } else {
            panic!("expected AnalyseRepo");
        }
    }

    #[test]
    fn test_analyze_american_spelling() {
        let result = parse_deterministic("analyze PR 23 servx");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::AnalysePr { .. }));
    }

    #[test]
    fn test_search() {
        let result = parse_deterministic("search for cats");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::Search { query } = r.intent {
            assert_eq!(query, "cats");
        } else {
            panic!("expected Search");
        }
    }

    #[test]
    fn test_google_search() {
        let result = parse_deterministic("google rust async programming");
        assert!(result.is_some());
        let r = result.unwrap();
        if let ParsedIntent::Search { query } = r.intent {
            assert_eq!(query, "rust async programming");
        } else {
            panic!("expected Search");
        }
    }

    #[test]
    fn test_media_pause() {
        let result = parse_deterministic("pause");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::MediaPlayPause));
    }

    #[test]
    fn test_media_next() {
        let result = parse_deterministic("next");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::MediaNext));
    }

    #[test]
    fn test_architect() {
        let result = parse_deterministic("open architecture mapper");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenArchitect));
    }

    #[test]
    fn test_architect_short() {
        let result = parse_deterministic("open architect");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenArchitect));
    }

    #[test]
    fn test_url_direct() {
        let result = parse_deterministic("open google.com");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenUrl { .. }));
    }

    #[test]
    fn test_unknown_command() {
        let result = parse_deterministic("what's the weather like");
        assert!(result.is_none()); // deterministic parser returns None for unknown
    }

    #[test]
    fn test_empty_input() {
        let result = parse_deterministic("");
        assert!(result.is_none());
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("chrome", "chrome"), 0);
        assert_eq!(levenshtein("chroem", "chrome"), 2);
        assert_eq!(levenshtein("whatsapp", "whatsapp"), 0);
    }

    #[test]
    fn test_is_valid_repo_name() {
        assert!(is_valid_repo_name("servx"));
        assert!(is_valid_repo_name("zync-meet"));
        assert!(is_valid_repo_name("zync_meet"));
        assert!(is_valid_repo_name("eesh264"));
        assert!(!is_valid_repo_name(""));
        assert!(!is_valid_repo_name("-invalid"));
        assert!(!is_valid_repo_name(".invalid"));
        assert!(!is_valid_repo_name("has space"));
    }

    #[test]
    fn test_parse_owner_repo() {
        assert_eq!(
            parse_owner_repo("zync-meet/zync"),
            Some(("zync-meet".to_string(), "zync".to_string()))
        );
        assert_eq!(
            parse_owner_repo("eesh264/congi"),
            Some(("eesh264".to_string(), "congi".to_string()))
        );
        assert_eq!(parse_owner_repo("no-slash"), None);
    }

    #[test]
    fn test_clean_repo_name() {
        assert_eq!(clean_repo_name("servx"), "servx");
        assert_eq!(clean_repo_name("the servx"), "servx");
        assert_eq!(clean_repo_name("servx repo"), "servx");
        assert_eq!(clean_repo_name("servx repository"), "servx");
    }

    // ─── Edge cases for Whisper mishearings ───────────────────────────────

    #[test]
    fn test_open_whats_app_mishearing() {
        // Whisper might transcribe "whatsapp" as "whats app" or "what's app"
        let result = parse_deterministic("open whats app");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));
    }

    #[test]
    fn test_open_gem_ini_mishearing() {
        // Whisper might transcribe "gemini" as "gem ini"
        let result = parse_deterministic("open gem ini");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));
    }

    #[test]
    fn test_open_you_tube_mishearing() {
        let result = parse_deterministic("open you tube");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));
    }

    #[test]
    fn test_open_chat_gpt_mishearing() {
        let result = parse_deterministic("open chat gpt");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));
    }

    #[test]
    fn test_analyse_pr_variations() {
        // Various PR command formats
        for cmd in &[
            "analyse PR 1 servx",
            "analyse PR 99 servx",
            "analyse pr 23 servx",
            "analyse PR 23 in servx",
            "analyse pull request 23 servx",
            "analyze PR 23 servx",
        ] {
            let result = parse_deterministic(cmd);
            assert!(result.is_some(), "failed to parse: {}", cmd);
            let r = result.unwrap();
            assert!(
                matches!(r.intent, ParsedIntent::AnalysePr { .. }),
                "expected AnalysePr for: {}, got {:?}",
                cmd,
                r.intent
            );
        }
    }

    #[test]
    fn test_analyse_repo_variations() {
        for cmd in &[
            "analyse servx",
            "analyse zync",
            "analyse servx repo",
            "analyse repo servx",
            "analyse the repo servx",
            "analyse zync-meet/zync",
            "analyse eesh264/congi",
            "analyze servx",
        ] {
            let result = parse_deterministic(cmd);
            assert!(result.is_some(), "failed to parse: {}", cmd);
            let r = result.unwrap();
            assert!(
                matches!(r.intent, ParsedIntent::AnalyseRepo { .. }),
                "expected AnalyseRepo for: {}, got {:?}",
                cmd,
                r.intent
            );
        }
    }

    #[test]
    fn test_open_verb_variations() {
        for verb in &[
            "open", "launch", "start", "run", "show", "pull up", "bring up",
            "fire up", "go to", "visit",
        ] {
            let cmd = format!("{} whatsapp", verb);
            let result = parse_deterministic(&cmd);
            assert!(result.is_some(), "failed to parse: {}", cmd);
            let r = result.unwrap();
            assert!(
                matches!(r.intent, ParsedIntent::OpenApp { .. }),
                "expected OpenApp for: {}, got {:?}",
                cmd,
                r.intent
            );
        }
    }

    #[test]
    fn test_case_insensitivity() {
        let result = parse_deterministic("OPEN WHATSAPP");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));

        let result = parse_deterministic("Analyse PR 23 Servx");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::AnalysePr { .. }));
    }

    #[test]
    fn test_extra_whitespace() {
        let result = parse_deterministic("open   whatsapp");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::OpenApp { .. }));

        let result = parse_deterministic("analyse  PR  23  servx");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(matches!(r.intent, ParsedIntent::AnalysePr { .. }));
    }

    #[test]
    fn test_pr_number_extraction() {
        // Verify PR numbers are correctly extracted
        let test_cases = [(1u32), (5), (23), (99), (100), (999)];
        for (i, expected_pr) in test_cases.iter().enumerate() {
            let cmd = format!("analyse PR {} servx", expected_pr);
            let result = parse_deterministic(&cmd);
            assert!(result.is_some());
            if let ParsedIntent::AnalysePr { pr_number, .. } = result.unwrap().intent {
                assert_eq!(pr_number, *expected_pr, "PR number mismatch for case {}", i);
            } else {
                panic!("expected AnalysePr");
            }
        }
    }
}
