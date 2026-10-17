//! Spoken words -> concrete objects (docs/features/140, P2/P3). NEXUS never acts on a spoken path
//! string: a folder alias ("downloads") becomes a real folder, an object phrase ("the pdfs",
//! "my latest screenshot") becomes a list of real directory entries that are read back before any
//! destructive step. An unclear or too-broad phrase resolves to nothing rather than to a guess.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::journal::TRASH_DIR;
use super::paths::{self, is_within};
use super::policy::Policy;

/// Entries considered per folder (a folder with more is not scanned further).
const SCAN_CAP: usize = 5000;
/// How many candidates are named when asking "which one?".
pub const AMBIGUOUS_NAMES: usize = 5;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub bytes: u64,
    pub modified: SystemTime,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub exts: Vec<String>,
    pub name_prefix: Option<String>,
    pub tokens: Vec<String>,
    pub newest: bool,
    pub plural: bool,
    pub dirs_only: bool,
}

#[derive(Debug)]
pub enum Resolved {
    Found(Vec<Candidate>),
    None,
    /// More than one thing fits and the phrase did not say "all" or "latest".
    Ambiguous(Vec<Candidate>),
    /// "everything" / "all my files": never resolved.
    Everything,
}

const STOP: &[&str] = &[
    "the", "a", "an", "my", "that", "this", "those", "these", "file", "files", "from", "in", "of", "on", "to", "please",
    "called", "named", "name", "item", "items", "stuff", "downloaded", "download", "just", "recent", "recently",
];

fn types(word: &str) -> Option<(&'static [&'static str], Option<&'static str>)> {
    const IMG: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "tiff"];
    Some(match word {
        "pdf" | "pdfs" => (&["pdf"], None),
        "image" | "images" | "picture" | "pictures" | "photo" | "photos" | "pic" | "pics" => (IMG, None),
        "screenshot" | "screenshots" => (IMG, Some("screenshot")),
        "video" | "videos" | "movie" | "movies" | "clip" | "clips" => (&["mp4", "mkv", "mov", "avi", "webm", "wmv"], None),
        "zip" | "zips" | "archive" | "archives" => (&["zip", "rar", "7z", "tar", "gz"], None),
        "document" | "documents" | "doc" | "docs" | "word" => (&["doc", "docx", "odt", "rtf", "txt"], None),
        "spreadsheet" | "spreadsheets" | "excel" | "sheet" | "sheets" => (&["xls", "xlsx", "csv", "ods"], None),
        "presentation" | "presentations" | "powerpoint" | "slides" | "ppt" => (&["ppt", "pptx", "odp"], None),
        "installer" | "installers" => (&["exe", "msi"], None),
        "music" | "audio" | "song" | "songs" | "mp3" | "mp3s" => (&["mp3", "wav", "flac", "m4a", "aac", "ogg"], None),
        _ => return None,
    })
}

fn words(phrase: &str) -> Vec<String> {
    phrase
        .to_lowercase()
        .replace(['.', ','], " . ")
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

pub fn parse_object(phrase: &str) -> Query {
    let mut q = Query::default();
    let mut prev_dot = false;
    for w in words(phrase) {
        if w == "." {
            prev_dot = true;
            continue;
        }
        let was_dot = std::mem::replace(&mut prev_dot, false);
        if was_dot && w.chars().all(|c| c.is_ascii_alphanumeric()) && w.len() <= 5 {
            q.exts.push(w.clone());
            continue;
        }
        match w.as_str() {
            "all" | "every" | "each" | "both" | "everything" => {
                q.plural = true;
                if w == "everything" {
                    q.tokens.push("*everything*".into());
                }
                continue;
            }
            "latest" | "newest" | "last" | "most" => {
                q.newest = true;
                continue;
            }
            "folder" | "folders" | "directory" | "directories" => {
                q.dirs_only = true;
                if w.ends_with('s') {
                    q.plural = true;
                }
                continue;
            }
            _ => {}
        }
        if STOP.contains(&w.as_str()) {
            continue;
        }
        if let Some((exts, prefix)) = types(&w) {
            for e in exts {
                if !q.exts.iter().any(|x| x == e) {
                    q.exts.push((*e).into());
                }
            }
            if prefix.is_some() {
                q.name_prefix = prefix.map(|p| p.to_string());
            }
            if w.ends_with('s') && w != "ppt" {
                q.plural = true;
            }
            continue;
        }
        q.tokens.push(w);
    }
    q
}

/// "downloads" / "my desktop" / "documents folder" -> a real folder inside the allowed roots.
pub fn resolve_folder(spoken: &str, policy: &Policy, roots: &[PathBuf]) -> Option<PathBuf> {
    let w: Vec<String> = words(spoken)
        .into_iter()
        .filter(|w| !matches!(w.as_str(), "the" | "my" | "folder" | "directory" | "in" | "on" | "from" | "to" | "inside"))
        .collect();
    let joined = w.join(" ");
    let known = match joined.as_str() {
        "downloads" | "download" => dirs_next::download_dir(),
        "desktop" | "screen" => dirs_next::desktop_dir(),
        "documents" | "docs" | "document" => dirs_next::document_dir(),
        "pictures" | "photos" | "images" => dirs_next::picture_dir(),
        "videos" | "movies" => dirs_next::video_dir(),
        "music" => dirs_next::audio_dir(),
        _ => None,
    };
    let pick = |p: PathBuf| paths::canonical(&p).ok().filter(|c| c.is_dir() && roots.iter().any(|r| is_within(c, r)));
    if let Some(p) = known.and_then(pick) {
        return Some(p);
    }
    // a folder the user added in the Hub, by its last name ("projects")
    for extra in &policy.extra_roots {
        let p = PathBuf::from(extra);
        let last = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !last.is_empty() && last == joined {
            if let Some(c) = pick(p) {
                return Some(c);
            }
        }
    }
    None
}

fn candidate(entry: &std::fs::DirEntry) -> Option<Candidate> {
    let path = entry.path();
    if paths::is_reparse(&path) {
        return None;
    }
    let name = entry.file_name().to_string_lossy().to_string();
    if name == TRASH_DIR || name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
        return None;
    }
    let meta = entry.metadata().ok()?;
    Some(Candidate {
        path,
        name,
        is_dir: meta.is_dir(),
        bytes: if meta.is_dir() { 0 } else { meta.len() },
        modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
    })
}

pub fn list_folder(folder: &Path) -> Vec<Candidate> {
    let Ok(rd) = std::fs::read_dir(folder) else { return vec![] };
    let mut v: Vec<Candidate> = rd.filter_map(|e| e.ok()).take(SCAN_CAP).filter_map(|e| candidate(&e)).collect();
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    v
}

fn matches(c: &Candidate, q: &Query) -> bool {
    if q.dirs_only && !c.is_dir {
        return false;
    }
    let lower = c.name.to_lowercase();
    let (stem, ext) = match lower.rsplit_once('.') {
        Some((s, e)) if !c.is_dir => (s.to_string(), e.to_string()),
        _ => (lower.clone(), String::new()),
    };
    if !q.exts.is_empty() && !q.exts.iter().any(|e| *e == ext) {
        return false;
    }
    if let Some(p) = &q.name_prefix {
        if !stem.starts_with(p.as_str()) {
            return false;
        }
    }
    q.tokens.iter().all(|t| stem.contains(t.as_str()))
}

/// Does this directory entry fit the query? (An empty query fits everything.)
pub fn matches_query(c: &Candidate, q: &Query) -> bool {
    matches(c, q)
}

/// Resolve a parsed object phrase inside one folder (not recursive).
pub fn resolve(folder: &Path, q: &Query) -> Resolved {
    if q.tokens.iter().any(|t| t == "*everything*") && q.exts.is_empty() {
        return Resolved::Everything;
    }
    // no name, no type, no "latest": nothing to go on ("delete the files", "delete the folder")
    let open = q.exts.is_empty() && q.tokens.is_empty() && q.name_prefix.is_none();
    if open && !q.newest {
        return if q.plural { Resolved::Everything } else { Resolved::None };
    }
    let mut q2 = q.clone();
    q2.tokens.retain(|t| t != "*everything*");
    let found: Vec<Candidate> = list_folder(folder).into_iter().filter(|c| matches(c, &q2)).collect();
    if found.is_empty() {
        return Resolved::None;
    }
    if q.newest {
        return Resolved::Found(vec![found.into_iter().next().unwrap()]);
    }
    if found.len() > 1 && !q.plural {
        return Resolved::Ambiguous(found.into_iter().take(AMBIGUOUS_NAMES).collect());
    }
    Resolved::Found(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_kernel::paths::test_dir;

    fn touch(dir: &Path, name: &str, age_secs: u64) {
        let p = dir.join(name);
        std::fs::write(&p, b"data").unwrap();
        let t = SystemTime::now() - std::time::Duration::from_secs(age_secs);
        let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        let _ = f.set_modified(t);
    }

    fn folder() -> PathBuf {
        let d = test_dir("resolve");
        touch(&d, "tax report 2025.pdf", 400);
        touch(&d, "resume.pdf", 300);
        touch(&d, "Screenshot 2026-01-01.png", 200);
        touch(&d, "Screenshot 2026-01-02.png", 100);
        touch(&d, "holiday.jpg", 50);
        touch(&d, "notes.txt", 10);
        std::fs::create_dir_all(d.join("old stuff")).unwrap();
        std::fs::create_dir_all(d.join(TRASH_DIR)).unwrap();
        touch(&d, ".hidden.pdf", 5);
        d
    }

    fn names(r: Resolved) -> Vec<String> {
        match r {
            Resolved::Found(v) | Resolved::Ambiguous(v) => v.into_iter().map(|c| c.name).collect(),
            _ => vec![],
        }
    }

    #[test]
    fn parse_object_understands_types_plurals_and_recency() {
        let q = parse_object("the pdfs");
        assert_eq!((q.exts.clone(), q.plural), (vec!["pdf".to_string()], true));
        let q = parse_object("my latest screenshot");
        assert!(q.newest && q.name_prefix.as_deref() == Some("screenshot") && !q.plural);
        let q = parse_object("tax report");
        assert_eq!(q.tokens, vec!["tax", "report"]);
        let q = parse_object("the .pdf files");
        assert_eq!(q.exts, vec!["pdf"]);
        assert!(parse_object("old stuff folder").dirs_only);
    }

    #[test]
    fn plural_type_resolves_all_and_hidden_and_trash_are_never_listed() {
        let d = folder();
        let r = names(resolve(&d, &parse_object("the pdfs")));
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|n| n.ends_with(".pdf")) && !r.iter().any(|n| n.starts_with('.')));
        let all: Vec<String> = list_folder(&d).into_iter().map(|c| c.name).collect();
        assert!(!all.iter().any(|n| n == TRASH_DIR));
    }

    #[test]
    fn a_name_resolves_to_one_thing_and_a_vague_phrase_is_ambiguous_not_guessed() {
        let d = folder();
        assert_eq!(names(resolve(&d, &parse_object("tax report"))), vec!["tax report 2025.pdf"]);
        assert!(matches!(resolve(&d, &parse_object("the pdf")), Resolved::Ambiguous(_)));
        assert!(matches!(resolve(&d, &parse_object("screenshot")), Resolved::Ambiguous(v) if v.len() == 2));
    }

    #[test]
    fn latest_picks_the_newest_match_only() {
        let d = folder();
        assert_eq!(names(resolve(&d, &parse_object("latest screenshot"))), vec!["Screenshot 2026-01-02.png"]);
        assert_eq!(names(resolve(&d, &parse_object("the newest pdf"))), vec!["resume.pdf"]);
    }

    #[test]
    fn everything_and_empty_phrases_never_resolve() {
        let d = folder();
        for p in ["everything", "all my files", "all the files", "all", "the files", "the file", "files"] {
            assert!(matches!(resolve(&d, &parse_object(p)), Resolved::Everything | Resolved::None), "{p}");
        }
        assert!(matches!(resolve(&d, &parse_object("everything")), Resolved::Everything));
        assert!(matches!(resolve(&d, &parse_object("all my files")), Resolved::Everything));
    }

    #[test]
    fn folders_only_and_no_match() {
        let d = folder();
        assert_eq!(names(resolve(&d, &parse_object("old stuff folder"))), vec!["old stuff"]);
        assert!(matches!(resolve(&d, &parse_object("budget")), Resolved::None));
        assert!(matches!(resolve(&d, &parse_object("pdf budget")), Resolved::None));
    }

    #[test]
    fn folder_aliases_resolve_only_inside_the_roots() {
        let d = test_dir("alias");
        let proj = d.join("projects");
        std::fs::create_dir_all(&proj).unwrap();
        let policy = Policy { use_default_roots: false, extra_roots: vec![proj.to_string_lossy().to_string()], ..Policy::default() };
        let roots = vec![proj.clone()];
        assert_eq!(resolve_folder("the projects folder", &policy, &roots), Some(proj));
        // known aliases outside the roots resolve to nothing
        assert_eq!(resolve_folder("downloads", &policy, &roots), None);
        assert_eq!(resolve_folder("c drive", &policy, &roots), None);
    }
}
